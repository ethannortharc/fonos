//! The linear workflow engine: [`validate`] checks that a [`WorkflowDef`]'s
//! source → processors → outputs chain is type-correct and instantiable, and
//! [`run`] executes it, emitting exactly one terminal [`PipelineEvent`] per
//! run.
//!
//! # Terminal-event contract
//!
//! Every run that reaches [`Source::acquire`] emits **exactly one** terminal
//! event:
//!
//! * [`PipelineEvent::NoSpeech`] — the source produced empty text and
//!   [`Source::allows_empty`] is false (the default; a source like
//!   `src.instant` that opts in with `allows_empty() == true` skips this and
//!   proceeds instead, for "blank-open" composites).
//! * [`PipelineEvent::Failed`] — the source, a processor, the recorder, or an
//!   output failed (the raw error is run through [`classify_error`]).
//! * [`PipelineEvent::Delivered`] — every output accepted the final text.
//!
//! [`PipelineEvent::Processing`] is a non-terminal progress signal, emitted
//! once the source has produced non-empty input. Structural failures caught
//! *before* `acquire` (an unknown widget id, a broken kind chain, no outputs, a
//! factory error) emit **no** event and simply return `Err`; callers pre-flight
//! these with [`validate`].
//!
//! # Step tracing (test-run bench)
//!
//! Independent of the terminal-event contract above, `run` also emits a
//! [`PipelineEvent::StepStarted`] / [`PipelineEvent::StepFinished`] pair around
//! every component invocation (the source, each processor, each output, in
//! that order) — UI-agnostic tracing consumed by the Test Run bench. These are
//! non-terminal and additional to the terminal event; adapters that don't care
//! about step tracing (e.g. the desktop float pill) ignore them.
//!
//! [`RunCtx::mock_text`] lets a caller start the chain from literal text
//! instead of calling [`Source::acquire`] (the chain must start with a
//! text-consuming source; an audio source rejects it). [`RunCtx::dry_run`]
//! intercepts output delivery — each output's `StepFinished` is emitted with
//! `intercepted: true` and `ms: 0` instead of calling [`Output::deliver`] — and
//! skips the history recorder; `Processing` / `Delivered` are still emitted as
//! usual. The recorder is also skipped (independent of `dry_run`) whenever
//! `final_text` is empty — an instant→composite run's blank open has nothing
//! worth a top-level history entry; the composite's own session widgets
//! record their own containers.

use std::sync::Arc;

use crate::config::AppConfig;
use crate::error_class::classify_error;
use crate::pipeline::{EventSink, PipelineEvent};
use crate::workflow::builtin::{built_in_widgets, built_in_workflows};
use crate::workflow::model::{Data, DataKind, WidgetDef, WorkflowDef};
use crate::workflow::registry::{Output, Processor, Registry, RunCtx, Source};

/// The result of a successful [`run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    /// The first text to appear in the pipeline: the selection text for a text
    /// source, or the transcript for an audio source once STT has run.
    pub raw_text: String,
    /// The text delivered to every output — the pipeline's final datum, after
    /// all processors have run.
    pub final_text: String,
    /// The history entry id returned by the recorder, or `None` when the run
    /// had no recorder configured.
    pub entry_id: Option<i64>,
}

/// Overlay `custom` onto `base`: a custom entry whose id equals a base entry's
/// replaces that base entry **wholesale** (in place, preserving position); a
/// custom entry with a new id is appended. The overlay is not a field-level
/// merge — the custom definition wins entirely.
fn overlay_by_id<T, F>(base: Vec<T>, custom: &[T], id_of: F) -> Vec<T>
where
    T: Clone,
    F: Fn(&T) -> &str,
{
    let mut result = base;
    for entry in custom {
        match result.iter_mut().find(|e| id_of(e) == id_of(entry)) {
            Some(slot) => *slot = entry.clone(),
            None => result.push(entry.clone()),
        }
    }
    result
}

/// The effective widget set: the built-ins, with each config entry in
/// [`AppConfig::widgets`] either replacing the built-in of the same id
/// wholesale or, if its id is new, appended.
pub fn effective_widgets(config: &AppConfig) -> Vec<WidgetDef> {
    overlay_by_id(built_in_widgets(), &config.widgets, |w| w.id.as_str())
}

/// The effective workflow set: the built-ins overlaid by
/// [`AppConfig::workflows`], with the same replace-by-id / append semantics as
/// [`effective_widgets`].
pub fn effective_workflows(config: &AppConfig) -> Vec<WorkflowDef> {
    overlay_by_id(built_in_workflows(), &config.workflows, |w| w.id.as_str())
}

/// Resolve a `workflow-{id}` hotkey label to the workflow id that should
/// actually run.
///
/// Every `workflow-{id}` label resolves directly to its own `{id}` — strips
/// the `workflow-` prefix and returns what's left, unchanged. (Workbench P1,
/// spec §3c: the former `workflow-wf.dictation` → `active_voice_workflow`
/// redirect is gone, along with the `&AppConfig` parameter it needed. The
/// floating pill now owns its own global hotkey and reads
/// `active_voice_workflow` itself at dispatch time — see the `"pill"` hotkey
/// arm in `main.rs` — so wf.dictation's own trigger chips, if any, always run
/// wf.dictation like any other recipe.)
pub fn resolve_trigger_target(label: &str) -> String {
    label.strip_prefix("workflow-").unwrap_or(label).to_string()
}

/// The **names** of every workflow in `workflows` that references `widget_id`
/// in its source, processors, or outputs. The settings layer uses this to
/// refuse deleting a widget a workflow still depends on, listing the referrers
/// by name. Returns an empty vec when nothing references the id.
pub fn widget_referenced_by(widget_id: &str, workflows: &[WorkflowDef]) -> Vec<String> {
    workflows
        .iter()
        .filter(|wf| {
            wf.source == widget_id
                || wf.processors.iter().any(|p| p == widget_id)
                || wf.outputs.iter().any(|o| o == widget_id)
        })
        .map(|wf| wf.name.clone())
        .collect()
}

/// The live components a workflow resolves to: its source, its processors (in
/// order), and its outputs (in order).
type Instantiated = (Arc<dyn Source>, Vec<Arc<dyn Processor>>, Vec<Arc<dyn Output>>);

/// Resolve `wf`'s widget ids to live components via `reg`, in source →
/// processors → outputs order. Fails fast — before anything observable — if an
/// id is unknown to `widgets` or a factory rejects the widget; emits no events.
fn instantiate(reg: &Registry, wf: &WorkflowDef, widgets: &[WidgetDef]) -> Result<Instantiated, String> {
    let find = |id: &str| -> Result<&WidgetDef, String> {
        widgets
            .iter()
            .find(|w| w.id == id)
            .ok_or_else(|| format!("workflow '{}': unknown widget id '{}'", wf.id, id))
    };
    let source = reg.make_source(find(&wf.source)?)?;
    let mut processors = Vec::with_capacity(wf.processors.len());
    for pid in &wf.processors {
        processors.push(reg.make_processor(find(pid)?)?);
    }
    let mut outputs = Vec::with_capacity(wf.outputs.len());
    for oid in &wf.outputs {
        outputs.push(reg.make_output(find(oid)?)?);
    }
    Ok((source, processors, outputs))
}

/// Check the `DataKind` chain of an already-instantiated workflow: there is at
/// least one output, each processor accepts what its upstream produces, the
/// pipeline ends in [`DataKind::Text`] (so a `final_text` can be captured), and
/// every output accepts that final kind. Returns `Err` on the first mismatch.
fn check_chain(
    wf: &WorkflowDef,
    source: &Arc<dyn Source>,
    processors: &[Arc<dyn Processor>],
    outputs: &[Arc<dyn Output>],
) -> Result<(), String> {
    if outputs.is_empty() {
        return Err(format!("workflow '{}': must declare at least one output", wf.id));
    }
    let mut kind = source.output_kind();
    for (pid, proc) in wf.processors.iter().zip(processors) {
        if proc.input_kind() != kind {
            return Err(format!(
                "workflow '{}': processor '{}' accepts {:?} but its input is {:?}",
                wf.id,
                pid,
                proc.input_kind(),
                kind
            ));
        }
        kind = proc.output_kind();
    }
    if kind != DataKind::Text {
        return Err(format!(
            "workflow '{}': pipeline ends in {:?}, but outputs require Text",
            wf.id, kind
        ));
    }
    for (oid, output) in wf.outputs.iter().zip(outputs) {
        if output.accepts() != kind {
            return Err(format!(
                "workflow '{}': output '{}' accepts {:?} but its input is {:?}",
                wf.id,
                oid,
                output.accepts(),
                kind
            ));
        }
    }
    Ok(())
}

/// Validate that `wf` is runnable against `reg` and `widgets`: every referenced
/// widget id exists and instantiates, it has at least one output, the
/// `DataKind` chain is continuous from source through processors, the pipeline
/// ends in [`DataKind::Text`], and every output accepts that final kind.
///
/// Returns `Err` with a human-readable reason on the first problem found, and
/// emits no events. [`run`] performs the same checks before doing anything
/// observable.
pub fn validate(reg: &Registry, wf: &WorkflowDef, widgets: &[WidgetDef]) -> Result<(), String> {
    let (source, processors, outputs) = instantiate(reg, wf, widgets)?;
    check_chain(wf, &source, &processors, &outputs)
}

/// A text rendering of `d` for step-trace previews: the text itself
/// (truncated to 4000 chars) for [`Data::Text`], the literal `"[audio]"` for
/// [`Data::Audio`].
fn preview_of(d: &Data) -> String {
    match d {
        // `String::truncate` takes a *byte* index and panics if that index
        // isn't a UTF-8 char boundary — a multi-byte string (e.g. CJK text,
        // 3 bytes/char) longer than ~1334 chars would panic here. Truncate
        // by chars instead so any valid `String` input is safe regardless of
        // encoding width.
        Data::Text(t) => t.chars().take(4000).collect(),
        Data::Audio(_) => "[audio]".to_string(),
    }
}

/// Emit a [`PipelineEvent::StepStarted`] for step `index` (`step_id`/`role`
/// identify the component). A free function rather than a closure so it
/// borrows only what each call site passes, with no capture-conflict risk
/// against the surrounding `run` body's other borrows of `ctx`/`wf`.
fn step_start(events: &dyn EventSink, workflow: &str, index: usize, step_id: &str, role: &str) {
    events.emit(PipelineEvent::StepStarted {
        workflow: workflow.to_string(),
        step_id: step_id.to_string(),
        index,
        role: role.to_string(),
    });
}

/// Emit a [`PipelineEvent::StepFinished`] for step `index`. See [`step_start`]
/// for why this is a free function rather than a closure.
#[allow(clippy::too_many_arguments)]
fn step_done(
    events: &dyn EventSink,
    workflow: &str,
    index: usize,
    step_id: &str,
    role: &str,
    preview: String,
    ms: u64,
    error: Option<String>,
    intercepted: bool,
) {
    events.emit(PipelineEvent::StepFinished {
        workflow: workflow.to_string(),
        step_id: step_id.to_string(),
        index,
        role: role.to_string(),
        preview,
        ms,
        error,
        intercepted,
    });
}

/// Execute `wf` end to end, honoring the terminal-event contract documented on
/// this module.
///
/// The sequence is: instantiate + check the chain (fail fast, no event) →
/// `acquire` (or substitute [`RunCtx::mock_text`]; empty text ⇒ `NoSpeech`,
/// unless [`Source::allows_empty`] is true) → emit `Processing` → run
/// processors (capturing the first text as `raw_text`) → record to history
/// (skipped when [`RunCtx::dry_run`], or when `final_text` is empty) →
/// deliver to every output in declared order (intercepted when `dry_run`) →
/// emit `Delivered`. The first failure at or after `acquire` emits
/// `Failed(classify_error(err))` and returns `Err`; delivery stops at the
/// first failing output. A [`PipelineEvent::StepStarted`] /
/// [`PipelineEvent::StepFinished`] pair brackets every component invocation
/// (source, then each processor, then each output) — see the module-level
/// "Step tracing" docs.
pub async fn run(
    reg: &Registry,
    wf: &WorkflowDef,
    widgets: &[WidgetDef],
    ctx: &RunCtx,
) -> Result<RunOutcome, String> {
    let events = ctx.events.as_ref();

    // 1. Fail fast: instantiate everything and check the chain, before any
    //    observable side effect. Structural errors emit no event.
    let (source, processors, outputs) = instantiate(reg, wf, widgets)?;
    check_chain(wf, &source, &processors, &outputs)?;

    // 2. Acquire the initial datum — or, for a test run, substitute
    //    `ctx.mock_text` in place of calling `source.acquire`. Mock text
    //    requires a text-consuming chain head; an audio source rejects it
    //    before anything observable happens beyond this step's own trace.
    step_start(events, &wf.id, 0, &wf.source, "source");
    let t0 = std::time::Instant::now();
    let mut current = if let Some(text) = &ctx.mock_text {
        if source.output_kind() == DataKind::Audio {
            let e = "mock text input requires a text-consuming chain".to_string();
            step_done(events, &wf.id, 0, &wf.source, "source", String::new(), 0, Some(e.clone()), false);
            return Err(e);
        }
        Data::Text(text.clone())
    } else {
        match source.acquire(ctx).await {
            Ok(data) => data,
            Err(e) => {
                step_done(
                    events,
                    &wf.id,
                    0,
                    &wf.source,
                    "source",
                    String::new(),
                    t0.elapsed().as_millis() as u64,
                    Some(e.clone()),
                    false,
                );
                ctx.events.emit(PipelineEvent::Failed(classify_error(&e)));
                return Err(e);
            }
        }
    };
    step_done(
        events,
        &wf.id,
        0,
        &wf.source,
        "source",
        preview_of(&current),
        t0.elapsed().as_millis() as u64,
        None,
        false,
    );
    if let Data::Text(text) = &current {
        if text.is_empty() && !source.allows_empty() {
            ctx.events.emit(PipelineEvent::NoSpeech);
            return Err("empty input".to_string());
        }
    }

    // 3. Input is in hand; processing has begun.
    ctx.events.emit(PipelineEvent::Processing);

    // 4. Run processors in order, capturing the first text datum as raw_text.
    let mut raw_text = match &current {
        Data::Text(text) => Some(text.clone()),
        Data::Audio(_) => None,
    };
    for (i, proc) in processors.iter().enumerate() {
        let sid = &wf.processors[i];
        let index = 1 + i;
        step_start(events, &wf.id, index, sid, "processor");
        let t = std::time::Instant::now();
        current = match proc.process(current, ctx).await {
            Ok(data) => data,
            Err(e) => {
                step_done(
                    events,
                    &wf.id,
                    index,
                    sid,
                    "processor",
                    String::new(),
                    t.elapsed().as_millis() as u64,
                    Some(e.clone()),
                    false,
                );
                // 8. Processor failure is terminal.
                ctx.events.emit(PipelineEvent::Failed(classify_error(&e)));
                return Err(e);
            }
        };
        step_done(
            events,
            &wf.id,
            index,
            sid,
            "processor",
            preview_of(&current),
            t.elapsed().as_millis() as u64,
            None,
            false,
        );
        if raw_text.is_none() {
            if let Data::Text(text) = &current {
                raw_text = Some(text.clone());
            }
        }
    }

    // The final datum must be text so a `final_text` exists. `check_chain`
    // guarantees this; guard defensively rather than panic.
    let final_text = match &current {
        Data::Text(text) => text.clone(),
        Data::Audio(_) => {
            let e = format!("workflow '{}': pipeline produced audio, expected text", wf.id);
            ctx.events.emit(PipelineEvent::Failed(classify_error(&e)));
            return Err(e);
        }
    };
    let raw_text = raw_text.unwrap_or_else(|| final_text.clone());

    // 5. Record to history between processing and delivery — skipped
    //    entirely in a dry run, and also skipped when `final_text` is empty
    //    (a blank-open instant→composite run: session widgets record their
    //    own containers, so an empty top-level entry would just be litter).
    //    A recorder failure is terminal, consistent with the other
    //    post-acquire steps.
    let mut entry_id = None;
    if !ctx.dry_run && !final_text.is_empty() {
        if let Some(recorder) = &ctx.recorder {
            // Contract: a RunRecorder that returns Err fails the run (single
            // terminal Failed). Recorders whose write is non-essential (e.g.
            // history logging) should absorb their own errors and return Ok —
            // see DbRecorder.
            match recorder.record(wf, &raw_text, &final_text) {
                Ok(id) => {
                    entry_id = Some(id);
                    // Scoped so the std mutex guard is dropped before the next
                    // `.await`; the lock is never held across an await point.
                    ctx.meta
                        .lock()
                        .expect("run ctx meta mutex poisoned")
                        .insert("entry_id".to_string(), serde_json::json!(id));
                }
                Err(e) => {
                    ctx.events.emit(PipelineEvent::Failed(classify_error(&e)));
                    return Err(e);
                }
            }
        }
    }

    // 6. Deliver to each output in declared order; stop at the first failure.
    //    A dry run intercepts delivery: emit an `intercepted` StepFinished
    //    instead of calling `Output::deliver`.
    let base = 1 + processors.len();
    for (j, output) in outputs.iter().enumerate() {
        let sid = &wf.outputs[j];
        let index = base + j;
        step_start(events, &wf.id, index, sid, "output");
        if ctx.dry_run {
            step_done(events, &wf.id, index, sid, "output", preview_of(&current), 0, None, true);
            continue;
        }
        let t = std::time::Instant::now();
        if let Err(e) = output.deliver(&current, ctx).await {
            step_done(
                events,
                &wf.id,
                index,
                sid,
                "output",
                String::new(),
                t.elapsed().as_millis() as u64,
                Some(e.clone()),
                false,
            );
            ctx.events.emit(PipelineEvent::Failed(classify_error(&e)));
            return Err(e);
        }
        step_done(
            events,
            &wf.id,
            index,
            sid,
            "output",
            preview_of(&current),
            t.elapsed().as_millis() as u64,
            None,
            false,
        );
    }

    // 7. Every output accepted the result. Carry the raw transcript, the final
    //    text, and this run's workflow id so surfaces can show both texts
    //    labeled by the workflow that produced them.
    ctx.events.emit(PipelineEvent::Delivered {
        raw: raw_text.clone(),
        final_text: final_text.clone(),
        workflow: Some(wf.id.clone()),
    });
    Ok(RunOutcome { raw_text, final_text, entry_id })
}

#[cfg(test)]
mod tests {
    use crate::pipeline::{EventSink, PipelineEvent};
    use crate::workflow::engine;
    use crate::workflow::model::*;
    use crate::workflow::registry::*;
    use std::sync::{Arc, Mutex};

    /// Records every emitted event in order.
    struct Capture(Mutex<Vec<PipelineEvent>>);
    impl EventSink for Capture {
        fn emit(&self, e: PipelineEvent) {
            self.0.lock().unwrap().push(e);
        }
    }

    /// A text source that yields a fixed string (same as the Task 2 tests).
    struct FixedText(String);
    #[async_trait::async_trait]
    impl Source for FixedText {
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn acquire(&self, _ctx: &RunCtx) -> Result<Data, String> {
            Ok(Data::Text(self.0.clone()))
        }
    }

    /// A text source that always yields empty text and opts into
    /// [`Source::allows_empty`] — stands in for `src.instant` (Task 2's
    /// blank-open support) without pulling in the desktop-side registry.
    struct AllowEmptyText;
    #[async_trait::async_trait]
    impl Source for AllowEmptyText {
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn acquire(&self, _ctx: &RunCtx) -> Result<Data, String> {
            Ok(Data::Text(String::new()))
        }
        fn allows_empty(&self) -> bool {
            true
        }
    }

    /// An audio source, for exercising the audio → STT kind chain.
    struct FixedAudio;
    #[async_trait::async_trait]
    impl Source for FixedAudio {
        fn output_kind(&self) -> DataKind {
            DataKind::Audio
        }
        async fn acquire(&self, _ctx: &RunCtx) -> Result<Data, String> {
            Ok(Data::Audio(AudioBuf { samples: vec![0i16; 4], sample_rate: 16000 }))
        }
    }

    /// A source whose `acquire` always fails, to exercise the acquire-failure
    /// path (a single `Failed`, with no `Processing` beforehand).
    struct FailingSource;
    #[async_trait::async_trait]
    impl Source for FailingSource {
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn acquire(&self, _ctx: &RunCtx) -> Result<Data, String> {
            Err("acquire boom".to_string())
        }
    }

    /// Uppercases its text input.
    struct Upper;
    #[async_trait::async_trait]
    impl Processor for Upper {
        fn input_kind(&self) -> DataKind {
            DataKind::Text
        }
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn process(&self, i: Data, _: &RunCtx) -> Result<Data, String> {
            Ok(Data::Text(i.into_text()?.to_uppercase()))
        }
    }

    /// Emits a 5000-char CJK string (3 bytes/char in UTF-8) regardless of
    /// input, to exercise `preview_of`'s truncation on a multibyte string
    /// well past the 4000-char cap — a regression guard for the panic fixed
    /// in Task 6 Fix Round 1 (`String::truncate` takes a byte index and
    /// isn't char-boundary-safe).
    struct CjkFlood;
    #[async_trait::async_trait]
    impl Processor for CjkFlood {
        fn input_kind(&self) -> DataKind {
            DataKind::Text
        }
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn process(&self, _i: Data, _: &RunCtx) -> Result<Data, String> {
            Ok(Data::Text("字".repeat(5000)))
        }
    }

    /// A fake STT: audio in, fixed transcript out.
    struct Stt;
    #[async_trait::async_trait]
    impl Processor for Stt {
        fn input_kind(&self) -> DataKind {
            DataKind::Audio
        }
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn process(&self, i: Data, _: &RunCtx) -> Result<Data, String> {
            i.into_audio()?;
            Ok(Data::Text("transcribed".to_string()))
        }
    }

    /// Always fails, to exercise the processor-failure path.
    struct Failing;
    #[async_trait::async_trait]
    impl Processor for Failing {
        fn input_kind(&self) -> DataKind {
            DataKind::Text
        }
        fn output_kind(&self) -> DataKind {
            DataKind::Text
        }
        async fn process(&self, _i: Data, _: &RunCtx) -> Result<Data, String> {
            Err("processor boom".to_string())
        }
    }

    /// Records what it delivered (`.0`) and the `entry_id` visible in
    /// `ctx.meta` at delivery time (`.1`), so tests can prove the recorder ran
    /// before delivery.
    struct Sink(Mutex<Vec<String>>, Mutex<Option<i64>>);
    #[async_trait::async_trait]
    impl Output for Sink {
        fn accepts(&self) -> DataKind {
            DataKind::Text
        }
        async fn deliver(&self, r: &Data, ctx: &RunCtx) -> Result<(), String> {
            let seen = ctx.meta.lock().unwrap().get("entry_id").and_then(|v| v.as_i64());
            *self.1.lock().unwrap() = seen;
            if let Data::Text(t) = r {
                self.0.lock().unwrap().push(t.clone());
            }
            Ok(())
        }
    }

    /// An output whose `deliver` always fails, to exercise the
    /// output-failure path — delivery must stop at the first failing output.
    struct FailingSink;
    #[async_trait::async_trait]
    impl Output for FailingSink {
        fn accepts(&self) -> DataKind {
            DataKind::Text
        }
        async fn deliver(&self, _result: &Data, _ctx: &RunCtx) -> Result<(), String> {
            Err("boom".to_string())
        }
    }

    /// A recorder that captures the `(raw, final)` pair it was handed and
    /// returns a fixed entry id.
    struct Rec(Mutex<Vec<(String, String)>>);
    impl RunRecorder for Rec {
        fn record(&self, _wf: &WorkflowDef, raw: &str, final_text: &str) -> Result<i64, String> {
            self.0.lock().unwrap().push((raw.to_string(), final_text.to_string()));
            Ok(42)
        }
    }

    fn widget(id: &str, role: WidgetRole, type_tag: &str, props: serde_json::Value) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role,
            type_tag: type_tag.to_string(),
            name: id.to_string(),
            icon: String::new(),
            props,
            builtin: false,
        }
    }

    fn workflow(source: &str, processors: &[&str], outputs: &[&str]) -> WorkflowDef {
        WorkflowDef {
            id: "wf.t".to_string(),
            name: "t".to_string(),
            icon: String::new(),
            hotkey: String::new(),
            triggers: Vec::new(),
            source: source.to_string(),
            processors: processors.iter().map(|s| s.to_string()).collect(),
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            builtin: false,
        }
    }

    /// A registry with all the test component `type_tag`s registered; the
    /// `"sink"` output factory hands out clones of `sink` so tests can inspect
    /// what was delivered.
    fn registry(sink: Arc<Sink>) -> Registry {
        let mut reg = Registry::default();
        reg.register_source(
            "fixed",
            Box::new(|props| {
                let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                Ok(Arc::new(FixedText(text)) as Arc<dyn Source>)
            }),
        );
        reg.register_source("audio", Box::new(|_| Ok(Arc::new(FixedAudio) as Arc<dyn Source>)));
        reg.register_source(
            "failing_source",
            Box::new(|_| Ok(Arc::new(FailingSource) as Arc<dyn Source>)),
        );
        reg.register_source("instant", Box::new(|_| Ok(Arc::new(AllowEmptyText) as Arc<dyn Source>)));
        reg.register_processor("upper", Box::new(|_| Ok(Arc::new(Upper) as Arc<dyn Processor>)));
        reg.register_processor(
            "cjk_flood",
            Box::new(|_| Ok(Arc::new(CjkFlood) as Arc<dyn Processor>)),
        );
        reg.register_processor("stt", Box::new(|_| Ok(Arc::new(Stt) as Arc<dyn Processor>)));
        reg.register_processor("fail", Box::new(|_| Ok(Arc::new(Failing) as Arc<dyn Processor>)));
        reg.register_output("sink", Box::new(move |_| Ok(sink.clone() as Arc<dyn Output>)));
        reg.register_output(
            "failing_sink",
            Box::new(|_| Ok(Arc::new(FailingSink) as Arc<dyn Output>)),
        );
        reg
    }

    /// The happy-path wiring: `src.t` (fixed) → `p.upper` → `out.sink`.
    fn setup(text: &str) -> (Registry, Vec<WidgetDef>, WorkflowDef, Arc<Sink>) {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.t", WidgetRole::Source, "fixed", serde_json::json!({ "text": text })),
            widget("p.upper", WidgetRole::Processor, "upper", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.t", &["p.upper"], &["out.sink"]);
        (reg, widgets, wf, sink)
    }

    #[tokio::test]
    async fn run_happy_path_emits_processing_then_delivered() {
        let (reg, widgets, wf, sink) = setup("hello");
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert_eq!(out.raw_text, "hello");
        assert_eq!(out.final_text, "HELLO");
        assert_eq!(sink.0.lock().unwrap().as_slice(), ["HELLO"]);
        // Filter out the per-step trace events (covered by their own test);
        // this test's concern is the terminal-event contract.
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert!(matches!(ev[0], PipelineEvent::Processing));
        assert!(matches!(
            &ev[1],
            PipelineEvent::Delivered { raw, final_text, workflow }
                if raw == "hello" && final_text == "HELLO" && workflow.as_deref() == Some("wf.t")
        ));
        assert_eq!(ev.len(), 2);
    }

    /// A source that does not override `allows_empty` (the default `false`,
    /// same as the real `microphone` / `selection` adapters) still yields
    /// `NoSpeech` on empty text — Task 2's `allows_empty` opt-in must not
    /// weaken this default.
    #[tokio::test]
    async fn empty_text_source_emits_no_speech_only() {
        let (reg, widgets, wf, sink) = setup("");
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert_eq!(err, "empty input");
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert_eq!(ev.len(), 1);
        assert!(matches!(ev[0], PipelineEvent::NoSpeech));
        assert!(sink.0.lock().unwrap().is_empty(), "nothing should be delivered");
    }

    /// Task 2: an `allows_empty` source (standing in for `src.instant`) with
    /// empty text runs the chain to completion — no `NoSpeech` short-circuit
    /// — and delivers the empty text to its output. This is the "blank-open"
    /// contract that later session-composite recipes (call/agent/meeting via
    /// `src.instant`) depend on.
    #[tokio::test]
    async fn allows_empty_source_runs_to_delivered_with_empty_text() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.instant", WidgetRole::Source, "instant", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.instant", &[], &["out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert_eq!(out.final_text, "");
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert_eq!(ev.len(), 2, "no NoSpeech; Processing then Delivered");
        assert!(matches!(ev[0], PipelineEvent::Processing));
        assert!(matches!(&ev[1], PipelineEvent::Delivered { final_text, .. } if final_text.is_empty()));
        assert_eq!(sink.0.lock().unwrap().as_slice(), [""], "empty text is still delivered");
    }

    /// Task 2: an instant→composite run whose `final_text` is empty must not
    /// create a History entry — session-composite widgets (call/agent/
    /// meeting) record their own containers, so a top-level empty entry
    /// would just be litter. Proven independent of `dry_run` (which is
    /// false here) by asserting the recorder itself was never invoked.
    #[tokio::test]
    async fn instant_chain_with_empty_final_text_skips_recorder() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.instant", WidgetRole::Source, "instant", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.instant", &[], &["out.sink"]);
        let rec = Arc::new(Rec(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: Arc::new(Capture(Mutex::new(vec![]))),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: Some(rec.clone()),
            mock_text: None,
            dry_run: false,
        };
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert_eq!(out.entry_id, None, "recorder must be skipped for an empty final_text");
        assert!(rec.0.lock().unwrap().is_empty(), "recorder must not be called");
        assert!(
            ctx.meta.lock().unwrap().get("entry_id").is_none(),
            "no entry_id written to meta when the recorder is skipped"
        );
    }

    #[tokio::test]
    async fn processor_failure_emits_single_failed() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.t", WidgetRole::Source, "fixed", serde_json::json!({ "text": "hello" })),
            widget("p.fail", WidgetRole::Processor, "fail", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.t", &["p.fail"], &["out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert_eq!(err, "processor boom");
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], PipelineEvent::Processing));
        assert!(matches!(ev[1], PipelineEvent::Failed(_)));
        assert!(sink.0.lock().unwrap().is_empty(), "no delivery after a processor failure");
    }

    #[tokio::test]
    async fn recorder_runs_between_process_and_deliver_and_sets_meta() {
        let (reg, widgets, wf, sink) = setup("hello");
        let rec = Arc::new(Rec(Mutex::new(vec![])));
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: Some(rec.clone()),
            mock_text: None,
            dry_run: false,
        };
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert_eq!(out.entry_id, Some(42));
        // The recorder was handed the raw and final text.
        assert_eq!(
            rec.0.lock().unwrap().as_slice(),
            [("hello".to_string(), "HELLO".to_string())]
        );
        // The output saw entry_id in meta at deliver time → recorder ran first.
        assert_eq!(*sink.1.lock().unwrap(), Some(42));
        // The engine persisted entry_id into ctx.meta.
        assert_eq!(
            ctx.meta.lock().unwrap().get("entry_id").and_then(|v| v.as_i64()),
            Some(42)
        );
        assert_eq!(sink.0.lock().unwrap().as_slice(), ["HELLO"]);
    }

    #[test]
    fn validate_rejects_broken_chains() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink);
        let widgets = vec![
            widget("src.text", WidgetRole::Source, "fixed", serde_json::json!({ "text": "hi" })),
            widget("src.audio", WidgetRole::Source, "audio", serde_json::Value::Null),
            widget("p.upper", WidgetRole::Processor, "upper", serde_json::Value::Null),
            widget("p.stt", WidgetRole::Processor, "stt", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];

        // (a) audio source feeding a text processor (no STT) → kind mismatch.
        let wf_a = workflow("src.audio", &["p.upper"], &["out.sink"]);
        let e = engine::validate(&reg, &wf_a, &widgets).unwrap_err();
        assert!(e.contains("Audio"), "expected a kind-mismatch error, got: {e}");

        // (b) no outputs → error.
        let wf_b = workflow("src.text", &["p.upper"], &[]);
        assert!(engine::validate(&reg, &wf_b, &widgets).is_err());

        // (c) reference to a missing widget id → error naming that id.
        let wf_c = workflow("src.text", &["p.nope"], &["out.sink"]);
        let e = engine::validate(&reg, &wf_c, &widgets).unwrap_err();
        assert!(e.contains("p.nope"), "error should name the missing id, got: {e}");

        // (d) continuous kind chain + matching output → ok, for both a text
        //     chain and an audio → STT chain.
        let wf_ok_text = workflow("src.text", &["p.upper"], &["out.sink"]);
        assert!(engine::validate(&reg, &wf_ok_text, &widgets).is_ok());
        let wf_ok_audio = workflow("src.audio", &["p.stt"], &["out.sink"]);
        assert!(engine::validate(&reg, &wf_ok_audio, &widgets).is_ok());
    }

    #[test]
    fn resolve_trigger_target_is_direct_for_every_workflow_id() {
        // Workbench P1, spec §3c: no more redirect — every `workflow-{id}`
        // label strips the prefix and resolves to its own id, including
        // `wf.dictation` itself (the pill now owns the "run whatever
        // active_voice_workflow names" behavior; see the `"pill"` hotkey arm
        // in `main.rs`).
        assert_eq!(engine::resolve_trigger_target("workflow-wf.listen"), "wf.listen");
        assert_eq!(engine::resolve_trigger_target("workflow-wf.dictation"), "wf.dictation");
        assert_eq!(engine::resolve_trigger_target("workflow-wf.custom-abc"), "wf.custom-abc");

        // A label with no `workflow-` prefix passes through unchanged.
        assert_eq!(engine::resolve_trigger_target("wf.dictation"), "wf.dictation");
    }

    #[test]
    fn widget_referenced_by_finds_source_processor_and_output_refs() {
        let mk = |name: &str, source: &str, processors: &[&str], outputs: &[&str]| -> WorkflowDef {
            WorkflowDef {
                id: format!("wf.{name}"),
                name: name.to_string(),
                icon: String::new(),
                hotkey: String::new(),
                triggers: Vec::new(),
                source: source.to_string(),
                processors: processors.iter().map(|s| s.to_string()).collect(),
                outputs: outputs.iter().map(|s| s.to_string()).collect(),
                builtin: false,
            }
        };
        let workflows = vec![
            mk("as_source", "w.target", &["p.x"], &["out.x"]),
            mk("as_processor", "src.x", &["w.target"], &["out.x"]),
            mk("as_output", "src.x", &["p.x"], &["w.target"]),
            mk("unrelated", "src.x", &["p.x"], &["out.x"]),
        ];
        // Referenced in the source, a processor, or an output → the workflow's
        // name is returned; the workflow that never mentions it is excluded.
        let refs = engine::widget_referenced_by("w.target", &workflows);
        assert_eq!(refs, vec!["as_source", "as_processor", "as_output"]);
        // A widget nothing references yields an empty list.
        assert!(engine::widget_referenced_by("w.nobody", &workflows).is_empty());
    }

    #[tokio::test]
    async fn run_pre_acquire_failure_emits_zero_events() {
        let (reg, widgets, _wf, sink) = setup("hello");
        // "p.nope" isn't in `widgets` — instantiate fails resolving the
        // processor id before `source.acquire` ever runs (same broken chain
        // as `validate_rejects_broken_chains` case (c), exercised via `run`
        // to pin that it also emits no events).
        let wf = workflow("src.t", &["p.nope"], &["out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert!(err.contains("p.nope"), "error should name the missing id, got: {err}");
        assert!(
            cap.0.lock().unwrap().is_empty(),
            "pre-acquire structural failure must emit no events"
        );
        assert!(sink.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_output_failure_emits_single_failed_and_skips_remaining() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.t", WidgetRole::Source, "fixed", serde_json::json!({ "text": "hello" })),
            widget("p.upper", WidgetRole::Processor, "upper", serde_json::Value::Null),
            widget("out.fail", WidgetRole::Output, "failing_sink", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        // The failing output is listed first — delivery must stop there and
        // never reach the recording sink declared after it.
        let wf = workflow("src.t", &["p.upper"], &["out.fail", "out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert_eq!(err, "boom");
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], PipelineEvent::Processing));
        assert!(matches!(ev[1], PipelineEvent::Failed(_)));
        assert!(
            sink.0.lock().unwrap().is_empty(),
            "delivery must stop at the first failing output"
        );
    }

    #[tokio::test]
    async fn source_acquire_failure_emits_single_failed() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.fail", WidgetRole::Source, "failing_source", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.fail", &[], &["out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert_eq!(err, "acquire boom");
        let ev: Vec<_> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
            .cloned()
            .collect();
        assert_eq!(ev.len(), 1);
        assert!(matches!(ev[0], PipelineEvent::Failed(_)));
        assert!(sink.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_emits_step_trace_and_dry_run_intercepts_outputs() {
        // src.t (fixed text, overridden by mock_text) → p.upper → out.sink.
        let (reg, widgets, wf, sink) = setup("unused");
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: Some("hello".into()),
            dry_run: true,
        };
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert!(!out.final_text.is_empty());

        let evs = cap.0.lock().unwrap();
        // Order: Step(source) → Processing → Step(processor) → Step(output, intercepted).
        let steps: Vec<_> = evs
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::StepFinished { role, intercepted, error, .. } => {
                    Some((role.clone(), *intercepted, error.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], ("source".into(), false, None));
        assert_eq!(steps[1], ("processor".into(), false, None));
        assert_eq!(steps[2], ("output".into(), true, None), "output must be intercepted");

        // dry_run must skip actual delivery to the fake output.
        assert!(sink.0.lock().unwrap().is_empty(), "dry-run must not deliver to the output");
    }

    #[tokio::test]
    async fn mock_text_on_audio_chain_is_rejected() {
        // src.audio (Audio kind) → p.stt → out.sink.
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.audio", WidgetRole::Source, "audio", serde_json::Value::Null),
            widget("p.stt", WidgetRole::Processor, "stt", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.audio", &["p.stt"], &["out.sink"]);
        let ctx = RunCtx {
            events: Arc::new(Capture(Mutex::new(vec![]))),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: Some("x".into()),
            dry_run: true,
        };
        let err = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap_err();
        assert!(err.contains("mock text"), "expected error mentioning mock text, got: {err}");
    }

    /// Regression guard for the Task 6 review finding: a processor that
    /// outputs a 5000-char CJK string (3 bytes/char in UTF-8, so 15000
    /// bytes — none of the multibyte char boundaries land on the naive
    /// 4000-*byte* truncation point) must not panic when the engine builds
    /// its step-trace preview. Before the fix, `preview_of` called
    /// `String::truncate(4000)`, which truncates by *byte* index and panics
    /// if that index isn't a UTF-8 char boundary; every step preview on a
    /// long multibyte transcript (e.g. Chinese dictation) hit this on the
    /// hot path in `run`. The fix truncates by chars, so the run must
    /// succeed and every previewed step must cap at exactly 4000 chars.
    #[tokio::test]
    async fn run_survives_multibyte_text_past_preview_cap() {
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let reg = registry(sink.clone());
        let widgets = vec![
            widget("src.t", WidgetRole::Source, "fixed", serde_json::json!({ "text": "hi" })),
            widget("p.cjk", WidgetRole::Processor, "cjk_flood", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.t", &["p.cjk"], &["out.sink"]);
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
            mock_text: None,
            dry_run: false,
        };

        // Would panic inside `run` (via `preview_of`) before the fix.
        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();
        assert_eq!(out.final_text.chars().count(), 5000, "full text is carried through untruncated");

        // Every StepFinished preview is capped at exactly 4000 chars, not
        // bytes — the processor and output steps both saw the 5000-char
        // CJK string.
        let previews: Vec<String> = cap
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                PipelineEvent::StepFinished { preview, error: None, .. } => Some(preview.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(previews.len(), 3, "source, processor, and output steps all finished");
        assert_eq!(previews[0], "hi", "source preview is short, untruncated");
        for preview in &previews[1..] {
            assert_eq!(preview.chars().count(), 4000, "preview must be capped at 4000 chars");
            assert!(preview.chars().all(|c| c == '字'), "preview content must still be valid CJK text");
        }
    }

    #[tokio::test]
    async fn silent_dictation_take_records_nothing() {
        // Product decision (P2): empty transcripts are non-events everywhere (agent/call/meeting precedent) — History gets no empty rows.
        /// A processor that returns empty text from audio input.
        struct EmptyTextFromAudio;
        #[async_trait::async_trait]
        impl Processor for EmptyTextFromAudio {
            fn input_kind(&self) -> DataKind {
                DataKind::Audio
            }
            fn output_kind(&self) -> DataKind {
                DataKind::Text
            }
            async fn process(&self, i: Data, _: &RunCtx) -> Result<Data, String> {
                i.into_audio()?;
                Ok(Data::Text(String::new()))
            }
        }

        // Set up: audio source → empty-text processor → output sink, with recorder
        let sink = Arc::new(Sink(Mutex::new(vec![]), Mutex::new(None)));
        let mut reg = registry(sink.clone());
        reg.register_processor(
            "empty_text",
            Box::new(|_| Ok(Arc::new(EmptyTextFromAudio) as Arc<dyn Processor>)),
        );

        let widgets = vec![
            widget("src.audio", WidgetRole::Source, "audio", serde_json::Value::Null),
            widget("p.empty", WidgetRole::Processor, "empty_text", serde_json::Value::Null),
            widget("out.sink", WidgetRole::Output, "sink", serde_json::Value::Null),
        ];
        let wf = workflow("src.audio", &["p.empty"], &["out.sink"]);

        let rec = Arc::new(Rec(Mutex::new(vec![])));
        let cap = Arc::new(Capture(Mutex::new(vec![])));
        let ctx = RunCtx {
            events: cap.clone(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: Some(rec.clone()),
            mock_text: None,
            dry_run: false,
        };

        let out = engine::run(&reg, &wf, &widgets, &ctx).await.unwrap();

        // Assert: recorder was NOT invoked
        assert!(rec.0.lock().unwrap().is_empty(), "silent take should not invoke recorder");
        // Assert: Delivered event was emitted with empty final_text
        let ev = cap.0.lock().unwrap();
        assert!(
            ev.iter().any(|e| matches!(
                e,
                PipelineEvent::Delivered { raw: _, final_text, workflow }
                    if final_text.is_empty()
            )),
            "should emit Delivered with empty final_text"
        );
        assert_eq!(out.entry_id, None, "no recorder invocation, so no entry_id");
    }
}

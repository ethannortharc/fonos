//! Test Run (试运行) bench commands: run a recipe or a single widget with
//! per-step trace events on `bench:event`, outputs intercepted unless
//! `deliver` is true. Never records to history, never lights the pill
//! (BenchEventSink does not emit float:*).

use std::sync::{Arc, Mutex};

use fonos_core::pipeline::{EventSink, PipelineEvent};
use fonos_core::workflow::model::{Data, DataKind, WidgetRole};
use fonos_core::workflow::registry::RunCtx;
use serde_json::json;
use tauri::Emitter;

/// Bench-only [`EventSink`]: maps every [`PipelineEvent`] to a single
/// `bench:event` payload and nothing else. Unlike [`crate::adapters::PillEventSink`]
/// it never emits `float:*` / `workflow:done` — `Delivered.workflow` is always
/// `None` from bench call sites, so even if it weren't, this sink ignores the
/// field entirely and only ever emits on the one `bench:event` channel.
pub struct BenchEventSink(pub tauri::AppHandle);

impl EventSink for BenchEventSink {
    fn emit(&self, event: PipelineEvent) {
        let payload = match event {
            PipelineEvent::StepStarted { workflow, step_id, index, role } =>
                json!({"type":"step_started","workflow":workflow,"step_id":step_id,"index":index,"role":role}),
            PipelineEvent::StepFinished { workflow, step_id, index, role, preview, ms, error, intercepted } =>
                json!({"type":"step_finished","workflow":workflow,"step_id":step_id,"index":index,
                       "role":role,"preview":preview,"ms":ms,"error":error,"intercepted":intercepted}),
            PipelineEvent::Processing => json!({"type":"processing"}),
            PipelineEvent::NoSpeech => json!({"type":"no_speech"}),
            PipelineEvent::Delivered { raw, final_text, .. } =>
                json!({"type":"done","raw":raw,"final":final_text}),
            PipelineEvent::Failed(surfaced) => json!({"type":"failed","message":surfaced.message}),
        };
        let _ = self.0.emit("bench:event", payload);
    }
}

/// Run `workflow_id` end to end through the platform-independent engine,
/// tracing every step on `bench:event` instead of the float pill. `mock_text`
/// substitutes for the source's `acquire()` (a text-consuming chain only);
/// `deliver` false (the bench default) intercepts every output — no
/// clipboard/insert/notebook/speak side effect and no history row.
///
/// Shares `run_workflow`'s in-flight guard: a bench run and a real hotkey-
/// triggered run can't overlap.
#[tauri::command(rename_all = "snake_case")]
pub async fn bench_run_workflow(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::AppState>,
    workflow_id: String,
    mock_text: Option<String>,
    deliver: bool,
) -> Result<(), String> {
    let _guard = super::workflow_exec::InFlightGuard::try_acquire()
        .ok_or_else(|| "a run is already in flight".to_string())?;
    let (wf, widgets, registry) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let wfs = fonos_core::workflow::engine::effective_workflows(&config);
        let wf = wfs
            .into_iter()
            .find(|w| w.id == workflow_id)
            .ok_or_else(|| format!("unknown workflow {workflow_id}"))?;
        (wf, fonos_core::workflow::engine::effective_widgets(&config), state.registry.clone())
    };
    // Structural pre-flight: a chain problem here (dangling id, kind
    // mismatch, no output) has no `run()` step to attach an error event to,
    // so without this check the command would return `Ok(())` with no
    // bench:event at all and the UI would sit "Running…" forever. Rejecting
    // the invoke() promise here is the only way this class of failure can
    // reach the frontend.
    fonos_core::workflow::engine::validate(&registry, &wf, &widgets)?;
    let ctx = RunCtx {
        events: Arc::new(BenchEventSink(app)),
        meta: Mutex::new(serde_json::Map::new()),
        recorder: None,
        mock_text,
        dry_run: !deliver,
    };
    // Errors are already reported via bench:event (step_finished.error /
    // failed); the command itself doesn't return them as an Err.
    let _ = fonos_core::workflow::engine::run(&registry, &wf, &widgets, &ctx).await;
    Ok(())
}

/// Run a single widget (source, processor, or output) in isolation, tracing
/// it as a one-step run on `bench:event`. Text-consuming processors take
/// `input_text` directly; an audio-consuming processor (e.g. STT) instead
/// captures live from the `microphone` widget (the frontend stops the
/// capture via the existing `finish_capture` command). `deliver` false
/// intercepts an output widget's actual delivery.
#[tauri::command(rename_all = "snake_case")]
pub async fn bench_run_widget(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::AppState>,
    widget_id: String,
    input_text: Option<String>,
    deliver: bool,
) -> Result<(), String> {
    let _guard = super::workflow_exec::InFlightGuard::try_acquire()
        .ok_or_else(|| "a run is already in flight".to_string())?;
    let (def, mic_def, registry) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let widgets = fonos_core::workflow::engine::effective_widgets(&config);
        let def = widgets
            .iter()
            .find(|w| w.id == widget_id)
            .cloned()
            .ok_or_else(|| format!("unknown widget {widget_id}"))?;
        let mic = widgets.iter().find(|w| w.type_tag == "microphone").cloned();
        (def, mic, state.registry.clone())
    };
    let sink = BenchEventSink(app);
    let ctx = RunCtx {
        events: Arc::new(BenchEventSink(sink.0.clone())),
        meta: Mutex::new(serde_json::Map::new()),
        recorder: None,
        mock_text: None,
        dry_run: !deliver,
    };
    let started = |sid: &str, role: &str| {
        sink.emit(PipelineEvent::StepStarted {
            workflow: String::new(), step_id: sid.into(), index: 0, role: role.into(),
        })
    };
    let finished = |sid: &str, role: &str, preview: String, ms: u64,
                    error: Option<String>, intercepted: bool| {
        sink.emit(PipelineEvent::StepFinished {
            workflow: String::new(), step_id: sid.into(), index: 0, role: role.into(),
            preview, ms, error, intercepted,
        })
    };
    let text_preview = |d: &Data| match d {
        Data::Text(t) => t.clone(),
        Data::Audio(_) => "[audio]".to_string(),
    };
    let t0 = std::time::Instant::now();
    let result: Result<String, String> = match def.role {
        WidgetRole::Source => {
            let src = registry.make_source(&def)?;
            started(&def.id, "source");
            src.acquire(&ctx).await.map(|d| text_preview(&d))
        }
        WidgetRole::Processor => {
            let proc = registry.make_processor(&def)?;
            let input = if proc.input_kind() == DataKind::Audio {
                // Audio-consuming processors (e.g. STT): capture live from the
                // microphone widget; the frontend stops it via finish_capture.
                let mic = mic_def.ok_or("no microphone widget available")?;
                let src = registry.make_source(&mic)?;
                started(&def.id, "processor");
                src.acquire(&ctx).await?
            } else {
                started(&def.id, "processor");
                Data::Text(input_text.unwrap_or_default())
            };
            proc.process(input, &ctx).await.map(|d| text_preview(&d))
        }
        WidgetRole::Output => {
            let out = registry.make_output(&def)?;
            started(&def.id, "output");
            let data = Data::Text(input_text.unwrap_or_default());
            if deliver {
                out.deliver(&data, &ctx).await.map(|_| text_preview(&data))
            } else {
                // Intercepted: per engine.rs's dry-run contract, an
                // intercepted output reports `ms: 0` (no delivery work ran),
                // not elapsed wall-clock time.
                finished(&def.id, "output", text_preview(&data), 0, None, true);
                sink.emit(PipelineEvent::Delivered {
                    raw: String::new(), final_text: text_preview(&data), workflow: None,
                });
                return Ok(());
            }
        }
    };
    let role = match def.role {
        WidgetRole::Source => "source",
        WidgetRole::Processor => "processor",
        WidgetRole::Output => "output",
    };
    match result {
        Ok(preview) => {
            finished(&def.id, role, preview.clone(), t0.elapsed().as_millis() as u64, None, false);
            sink.emit(PipelineEvent::Delivered { raw: String::new(), final_text: preview, workflow: None });
        }
        Err(e) => {
            finished(&def.id, role, String::new(), t0.elapsed().as_millis() as u64, Some(e), false);
        }
    }
    Ok(())
}

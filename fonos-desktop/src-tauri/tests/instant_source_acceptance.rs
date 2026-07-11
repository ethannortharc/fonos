//! Workbench P2 Task 2 — instant source + blank-open engine acceptance test.
//!
//! Proves that the real, production `InstantSource`
//! (`fonos_desktop::commands::workflow_widgets::InstantSource`, registered
//! under the `"instant"` type_tag by `build_registry`) runs end to end
//! through the real, unmodified `fonos_core::workflow::engine::run`: no
//! `NoSpeech` short-circuit on its empty text, and no history entry recorded
//! for the resulting empty `final_text` — the two engine changes Task 2
//! makes ([`Source::allows_empty`] and the recorder's empty-`final_text`
//! skip).
//!
//! Mirrors `uppercase_acceptance.rs`'s pattern: local test doubles for the
//! output and recorder (no live `tauri::AppHandle` needed, since
//! `InstantSource` — unlike `SelectionSource`/`MicSource` — has no
//! `AppHandle` dependency at all), the real production source type wired
//! into a bare `Registry`.
//!
//! Run with:
//!   cargo test --manifest-path fonos-desktop/src-tauri/Cargo.toml --test instant_source_acceptance

use std::sync::{Arc, Mutex};

use fonos_core::pipeline::{EventSink, PipelineEvent};
use fonos_core::workflow::engine;
use fonos_core::workflow::model::{Data, DataKind, WidgetDef, WidgetRole, WorkflowDef};
use fonos_core::workflow::registry::{Output, Registry, RunCtx, RunRecorder, Source};

use fonos_desktop::commands::workflow_widgets::InstantSource;

/// Captures whatever text is delivered to it (mirrors `uppercase_acceptance.rs`'s
/// `CapturingSink`).
struct CapturingSink(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl Output for CapturingSink {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }
    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        if let Data::Text(t) = result {
            self.0.lock().unwrap().push(t.clone());
        }
        Ok(())
    }
}

/// Records every event emitted during the run, so the test can assert on the
/// terminal-event contract (no `NoSpeech`).
struct CapturingEvents(Mutex<Vec<PipelineEvent>>);
impl EventSink for CapturingEvents {
    fn emit(&self, e: PipelineEvent) {
        self.0.lock().unwrap().push(e);
    }
}

/// Records every `(raw, final)` pair it is handed. Used to prove the
/// recorder is never invoked for an empty `final_text`.
struct CapturingRecorder(Mutex<Vec<(String, String)>>);
impl RunRecorder for CapturingRecorder {
    fn record(&self, _wf: &WorkflowDef, raw: &str, final_text: &str) -> Result<i64, String> {
        self.0.lock().unwrap().push((raw.to_string(), final_text.to_string()));
        Ok(1)
    }
}

fn widget(id: &str, role: WidgetRole, type_tag: &str) -> WidgetDef {
    WidgetDef {
        id: id.to_string(),
        role,
        type_tag: type_tag.to_string(),
        name: id.to_string(),
        icon: String::new(),
        props: serde_json::json!({}),
        builtin: true,
    }
}

#[tokio::test]
async fn instant_source_runs_to_delivered_with_no_no_speech_and_no_history_record() {
    let sink = Arc::new(CapturingSink(Mutex::new(vec![])));
    let events = Arc::new(CapturingEvents(Mutex::new(vec![])));
    let recorder = Arc::new(CapturingRecorder(Mutex::new(vec![])));

    // A minimal registry: the real production `InstantSource` registered
    // under "instant" exactly as `build_registry` registers it, and a test
    // "sink" output.
    let mut reg = Registry::default();
    reg.register_source("instant", Box::new(|_props| Ok(Arc::new(InstantSource) as Arc<dyn Source>)));
    {
        let sink = sink.clone();
        reg.register_output("sink", Box::new(move |_| Ok(sink.clone() as Arc<dyn Output>)));
    }

    let widgets = vec![
        widget("src.instant", WidgetRole::Source, "instant"),
        widget("out.sink", WidgetRole::Output, "sink"),
    ];
    let wf = WorkflowDef {
        id: "wf.t".into(),
        name: "t".into(),
        icon: String::new(),
        hotkey: String::new(),
        triggers: Vec::new(),
        source: "src.instant".into(),
        processors: vec![],
        outputs: vec!["out.sink".into()],
        builtin: false,
    };

    let ctx = RunCtx {
        events: events.clone(),
        meta: Mutex::new(serde_json::Map::new()),
        recorder: Some(recorder.clone()),
        mock_text: None,
        dry_run: false,
    };

    let outcome = engine::run(&reg, &wf, &widgets, &ctx)
        .await
        .expect("an allows_empty source's empty text must not short-circuit the run");

    assert_eq!(outcome.final_text, "");
    assert_eq!(sink.0.lock().unwrap().as_slice(), [""], "empty text is still delivered");

    // No NoSpeech: Processing then Delivered only.
    let terminal: Vec<_> = events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|e| !matches!(e, PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. }))
        .cloned()
        .collect();
    assert_eq!(terminal.len(), 2);
    assert!(matches!(terminal[0], PipelineEvent::Processing));
    assert!(matches!(&terminal[1], PipelineEvent::Delivered { final_text, .. } if final_text.is_empty()));

    // No history record for the empty final_text.
    assert_eq!(outcome.entry_id, None, "recorder must be skipped for an empty final_text");
    assert!(recorder.0.lock().unwrap().is_empty(), "recorder must not be invoked");
}

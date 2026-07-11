//! Workflow P1 Task 16 — extensibility acceptance test.
//!
//! Proves that a brand-new workflow component (`UppercaseProcessor`, added
//! entirely in `src/commands/widget_uppercase.rs` plus one registration line
//! in `workflow_widgets.rs::build_registry`) runs end to end through the
//! real, unmodified `fonos_core::workflow::engine::run` — i.e. the
//! extensibility abstraction holds without any engine/model/registry changes.
//!
//! The source and output here are local test doubles (mirroring the
//! `FixedText` / `Sink` doubles fonos-core's own engine tests use) rather than
//! the real `selection` / `clipboard` desktop adapters, because those need a
//! live `tauri::AppHandle` that this crate's test harness does not construct
//! (no test here builds one, and the `tauri` dependency doesn't enable the
//! `test` feature) — using them would require touching Cargo.toml, which is
//! outside this task's allowed diff. `UppercaseProcessor` itself is the real
//! production type from `fonos_desktop::commands::widget_uppercase`.
//!
//! Run with:
//!   cargo test --manifest-path fonos-desktop/src-tauri/Cargo.toml --test uppercase_acceptance

use std::sync::{Arc, Mutex};

use fonos_core::workflow::engine;
use fonos_core::workflow::model::{Data, DataKind, WidgetDef, WidgetRole, WorkflowDef};
use fonos_core::workflow::registry::{Output, Processor, Registry, RunCtx, Source};

use fonos_desktop::commands::widget_uppercase::UppercaseProcessor;

/// Fixed-text test source (mirrors fonos-core's own `FixedText` double). The
/// test isn't exercising source behavior, just standing in for `selection`.
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

/// Captures whatever text is delivered to it (mirrors fonos-core's own `Sink`
/// double). Stands in for `clipboard`.
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

fn widget(id: &str, role: WidgetRole, type_tag: &str) -> WidgetDef {
    WidgetDef {
        id: id.to_string(),
        role,
        type_tag: type_tag.to_string(),
        name: id.to_string(),
        icon: String::new(),
        props: serde_json::Value::Null,
        builtin: false,
    }
}

#[tokio::test]
async fn uppercase_processor_runs_end_to_end_through_the_real_engine() {
    let sink = Arc::new(CapturingSink(Mutex::new(vec![])));

    // A minimal registry: a test "fixed" source, the real production
    // `UppercaseProcessor` registered under "uppercase" exactly as
    // `workflow_widgets.rs::build_registry` registers it, and a test "sink"
    // output.
    let mut reg = Registry::default();
    reg.register_source(
        "fixed",
        Box::new(|props| {
            let text = props
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Arc::new(FixedText(text)) as Arc<dyn Source>)
        }),
    );
    reg.register_processor(
        "uppercase",
        Box::new(|_props| Ok(Arc::new(UppercaseProcessor) as Arc<dyn Processor>)),
    );
    {
        let sink = sink.clone();
        reg.register_output("sink", Box::new(move |_| Ok(sink.clone() as Arc<dyn Output>)));
    }

    let widgets = vec![
        WidgetDef {
            props: serde_json::json!({ "text": "hello world" }),
            ..widget("src.t", WidgetRole::Source, "fixed")
        },
        widget("p.uppercase", WidgetRole::Processor, "uppercase"),
        widget("out.sink", WidgetRole::Output, "sink"),
    ];
    let wf = WorkflowDef {
        id: "wf.t".into(),
        name: "t".into(),
        icon: String::new(),
        hotkey: String::new(),
        triggers: Vec::new(),
        source: "src.t".into(),
        processors: vec!["p.uppercase".into()],
        outputs: vec!["out.sink".into()],
        builtin: false,
    };

    let ctx = RunCtx::for_test();
    let outcome = engine::run(&reg, &wf, &widgets, &ctx)
        .await
        .expect("selection -> uppercase -> sink should run to completion");

    assert_eq!(outcome.raw_text, "hello world");
    assert_eq!(outcome.final_text, "HELLO WORLD");
    assert_eq!(sink.0.lock().unwrap().as_slice(), ["HELLO WORLD"]);
}

//! Component traits ([`Source`], [`Processor`], [`Output`]), the shared
//! per-run context ([`RunCtx`]), and the [`Registry`] that instantiates
//! configured [`crate::workflow::model::WidgetDef`]s into live components by
//! `type_tag`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::pipeline::EventSink;
use crate::workflow::model::{Data, DataKind, WidgetDef, WidgetRole, WorkflowDef};

/// Shared, per-run context passed to every component invocation.
///
/// Holds the platform event sink, out-of-band metadata components use to
/// pass side information to each other (e.g. the source app name, an
/// in-progress history entry id), and an optional recorder for persisting
/// completed runs to history.
pub struct RunCtx {
    /// Sink for pipeline lifecycle events (`Processing`, `Delivered`, ...).
    pub events: Arc<dyn EventSink>,
    /// Target language for translation-capable processors. Empty means no
    /// translation requested.
    pub translate_target: String,
    /// Out-of-band channel between components: sources write `"app_name"` /
    /// `"editable"`; the recorder writes `"entry_id"`; a speak output writes
    /// `"audio_ref"`. A JSON object, guarded by a mutex since components run
    /// with shared access to the context.
    pub meta: Mutex<serde_json::Map<String, serde_json::Value>>,
    /// Called by the engine after processing completes and before delivery,
    /// to persist a history entry. `None` means don't record (used in
    /// tests).
    pub recorder: Option<Arc<dyn RunRecorder>>,
}

impl RunCtx {
    /// Build a bare-bones [`RunCtx`] for tests: events go nowhere,
    /// `translate_target` is empty, `meta` is an empty object, and there is
    /// no recorder. Used by this crate's own component tests and by
    /// desktop-side integration tests.
    pub fn for_test() -> RunCtx {
        RunCtx {
            events: Arc::new(NullSink),
            translate_target: String::new(),
            meta: Mutex::new(serde_json::Map::new()),
            recorder: None,
        }
    }
}

/// An [`EventSink`] that discards every event. Backs [`RunCtx::for_test`].
struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: crate::pipeline::PipelineEvent) {}
}

/// Persists a completed run to history. Implemented by platform shells that
/// have a database.
pub trait RunRecorder: Send + Sync {
    /// Record one completed run and return its new history entry id. The
    /// engine writes the returned id into `ctx.meta["entry_id"]`.
    fn record(&self, wf: &WorkflowDef, raw_text: &str, final_text: &str) -> Result<i64, String>;
}

/// Produces the initial [`Data`] for a workflow run (e.g. selection,
/// microphone).
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    /// The [`DataKind`] this source produces.
    fn output_kind(&self) -> DataKind;
    /// Acquire the initial data for a run.
    async fn acquire(&self, ctx: &RunCtx) -> Result<Data, String>;
}

/// Transforms [`Data`] as it flows through a workflow (e.g. STT, LLM
/// polish).
#[async_trait::async_trait]
pub trait Processor: Send + Sync {
    /// The [`DataKind`] this processor accepts.
    fn input_kind(&self) -> DataKind;
    /// The [`DataKind`] this processor produces.
    fn output_kind(&self) -> DataKind;
    /// Transform `input` into the next stage's data.
    async fn process(&self, input: Data, ctx: &RunCtx) -> Result<Data, String>;
}

/// Delivers the final [`Data`] of a workflow run somewhere (e.g. insert,
/// clipboard).
#[async_trait::async_trait]
pub trait Output: Send + Sync {
    /// The [`DataKind`] this output accepts.
    fn accepts(&self) -> DataKind;
    /// Deliver `result` to this output's destination.
    async fn deliver(&self, result: &Data, ctx: &RunCtx) -> Result<(), String>;
}

/// Builds a [`Source`] from a widget's `props`.
pub type SourceFactory =
    Box<dyn Fn(&serde_json::Value) -> Result<Arc<dyn Source>, String> + Send + Sync>;
/// Builds a [`Processor`] from a widget's `props`.
pub type ProcessorFactory =
    Box<dyn Fn(&serde_json::Value) -> Result<Arc<dyn Processor>, String> + Send + Sync>;
/// Builds an [`Output`] from a widget's `props`.
pub type OutputFactory =
    Box<dyn Fn(&serde_json::Value) -> Result<Arc<dyn Output>, String> + Send + Sync>;

/// Maps component `type_tag`s to factories, and instantiates configured
/// [`WidgetDef`]s into live components, checking that the widget's declared
/// [`WidgetRole`] matches the requested component kind.
#[derive(Default)]
pub struct Registry {
    sources: HashMap<String, SourceFactory>,
    processors: HashMap<String, ProcessorFactory>,
    outputs: HashMap<String, OutputFactory>,
}

impl Registry {
    /// Register a [`Source`] factory under `type_tag`.
    pub fn register_source(&mut self, type_tag: &str, f: SourceFactory) {
        self.sources.insert(type_tag.to_string(), f);
    }

    /// Register a [`Processor`] factory under `type_tag`.
    pub fn register_processor(&mut self, type_tag: &str, f: ProcessorFactory) {
        self.processors.insert(type_tag.to_string(), f);
    }

    /// Register an [`Output`] factory under `type_tag`.
    pub fn register_output(&mut self, type_tag: &str, f: OutputFactory) {
        self.outputs.insert(type_tag.to_string(), f);
    }

    /// Instantiate `def` as a [`Source`]. Errors if `def.role` is not
    /// [`WidgetRole::Source`] or if `def.type_tag` is not registered.
    pub fn make_source(&self, def: &WidgetDef) -> Result<Arc<dyn Source>, String> {
        if def.role != WidgetRole::Source {
            return Err(format!(
                "widget '{}' (type_tag '{}') has role {:?}, expected Source",
                def.id, def.type_tag, def.role
            ));
        }
        let factory = self.sources.get(&def.type_tag).ok_or_else(|| {
            format!(
                "widget '{}': no source registered for type_tag '{}'",
                def.id, def.type_tag
            )
        })?;
        factory(&def.props)
    }

    /// Instantiate `def` as a [`Processor`]. Errors if `def.role` is not
    /// [`WidgetRole::Processor`] or if `def.type_tag` is not registered.
    pub fn make_processor(&self, def: &WidgetDef) -> Result<Arc<dyn Processor>, String> {
        if def.role != WidgetRole::Processor {
            return Err(format!(
                "widget '{}' (type_tag '{}') has role {:?}, expected Processor",
                def.id, def.type_tag, def.role
            ));
        }
        let factory = self.processors.get(&def.type_tag).ok_or_else(|| {
            format!(
                "widget '{}': no processor registered for type_tag '{}'",
                def.id, def.type_tag
            )
        })?;
        factory(&def.props)
    }

    /// Instantiate `def` as an [`Output`]. Errors if `def.role` is not
    /// [`WidgetRole::Output`] or if `def.type_tag` is not registered.
    pub fn make_output(&self, def: &WidgetDef) -> Result<Arc<dyn Output>, String> {
        if def.role != WidgetRole::Output {
            return Err(format!(
                "widget '{}' (type_tag '{}') has role {:?}, expected Output",
                def.id, def.type_tag, def.role
            ));
        }
        let factory = self.outputs.get(&def.type_tag).ok_or_else(|| {
            format!(
                "widget '{}': no output registered for type_tag '{}'",
                def.id, def.type_tag
            )
        })?;
        factory(&def.props)
    }

    /// List the `type_tag`s registered for `role`.
    pub fn known_type_tags(&self, role: WidgetRole) -> Vec<String> {
        match role {
            WidgetRole::Source => self.sources.keys().cloned().collect(),
            WidgetRole::Processor => self.processors.keys().cloned().collect(),
            WidgetRole::Output => self.outputs.keys().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::*;

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

    fn test_ctx() -> RunCtx {
        RunCtx::for_test()
    }

    #[tokio::test]
    async fn registry_instantiates_by_type_tag_and_checks_role() {
        let mut reg = Registry::default();
        reg.register_source(
            "fixed",
            Box::new(|props| {
                let text = props
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(std::sync::Arc::new(FixedText(text)) as std::sync::Arc<dyn Source>)
            }),
        );
        let def = WidgetDef {
            id: "src.t".into(),
            role: WidgetRole::Source,
            type_tag: "fixed".into(),
            name: "t".into(),
            icon: String::new(),
            props: serde_json::json!({"text":"hello"}),
            builtin: false,
        };
        let s = reg.make_source(&def).unwrap();
        assert_eq!(
            s.acquire(&test_ctx()).await.unwrap().into_text().unwrap(),
            "hello"
        );

        // role 不匹配报错
        let bad = WidgetDef {
            role: WidgetRole::Output,
            ..def.clone()
        };
        assert!(reg.make_source(&bad).is_err());
        // 未注册 tag 报错
        let unknown = WidgetDef {
            type_tag: "nope".into(),
            ..def
        };
        assert!(reg.make_source(&unknown).is_err());
    }
}

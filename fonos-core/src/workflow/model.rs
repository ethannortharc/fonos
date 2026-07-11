//! Core data model for the workflow engine: the [`Data`] envelope that flows
//! between components, and the persisted definitions ([`WidgetDef`],
//! [`WorkflowDef`]) that describe how components are wired together.

use serde::{Deserialize, Serialize};

/// The shape of data a [`Data`] value carries, and what a widget accepts or
/// produces.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    /// Raw audio samples.
    Audio,
    /// Plain text.
    Text,
}

/// Raw PCM audio buffer.
#[derive(Debug, Clone)]
pub struct AudioBuf {
    /// Signed 16-bit PCM samples.
    pub samples: Vec<i16>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
}

/// The value that flows between a source, its processors, and its outputs.
#[derive(Debug, Clone)]
pub enum Data {
    /// Audio payload.
    Audio(AudioBuf),
    /// Text payload.
    Text(String),
}

impl Data {
    /// The [`DataKind`] of this value.
    pub fn kind(&self) -> DataKind {
        match self {
            Data::Audio(_) => DataKind::Audio,
            Data::Text(_) => DataKind::Text,
        }
    }

    /// Unwrap into text, or an error if this value is audio.
    pub fn into_text(self) -> Result<String, String> {
        match self {
            Data::Text(s) => Ok(s),
            Data::Audio(_) => Err("expected text, got audio".into()),
        }
    }

    /// Unwrap into an audio buffer, or an error if this value is text.
    pub fn into_audio(self) -> Result<AudioBuf, String> {
        match self {
            Data::Audio(a) => Ok(a),
            Data::Text(_) => Err("expected audio, got text".into()),
        }
    }
}

/// The role a widget plays in a workflow pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetRole {
    /// Produces the initial [`Data`] for a run (e.g. selection, microphone).
    Source,
    /// Transforms [`Data`] (e.g. STT, LLM polish).
    Processor,
    /// Delivers the final [`Data`] somewhere (e.g. insert, clipboard).
    Output,
}

/// A configured widget instance: a named, persisted configuration of a
/// component `type_tag`, ready to be instantiated by the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetDef {
    /// Globally unique id. Builtins use a role prefix, e.g. `"src.selection"`,
    /// `"stt.default"`, `"llm.polish"`, `"out.insert"`.
    pub id: String,
    /// The role this widget plays in a pipeline.
    pub role: WidgetRole,
    /// Which registered component implementation to instantiate, e.g.
    /// `"selection"`, `"microphone"`, `"stt"`, `"llm"`, `"insert"`,
    /// `"replace"`, `"clipboard"`, `"notebook"`, `"speak"`, `"panel"`,
    /// `"uppercase"`.
    pub type_tag: String,
    /// Display name.
    pub name: String,
    /// Display icon (emoji or icon key).
    #[serde(default)]
    pub icon: String,
    /// Component-specific configuration, interpreted by the matching
    /// factory in the registry.
    #[serde(default)]
    pub props: serde_json::Value,
    /// Whether this widget ships with the app (builtins cannot be deleted).
    #[serde(default)]
    pub builtin: bool,
}

/// Props that hold references to other widget instances, per `type_tag`.
/// Workbench P2's composite widgets (`dialog`/`call`/`agent`/`meeting`, built
/// in T4/T6-T9) embed a capability widget's id directly as a string prop
/// value instead of instantiating their own — e.g. a `call` widget's
/// `stt_widget` prop names the `stt`-type widget it delegates to. This table
/// is the single declaration of which props are ref props for which
/// composite, consulted by [`crate::workflow::engine::widget_referenced_by`]
/// (pierced usage/delete guards) and by the desktop's `save_widget` composite
/// validation (never composite→composite, target type must match). A
/// `type_tag` absent here (every non-composite type) has no ref props.
pub fn widget_ref_props(type_tag: &str) -> &'static [&'static str] {
    match type_tag {
        "dialog" => &["llm_widget"],
        "call" => &["stt_widget", "llm_widget"],
        "agent" => &["llm_widget"],
        "meeting" => &["stt_widget", "llm_widget"],
        _ => &[],
    }
}

/// Session composites — may reference capability widgets (via
/// [`widget_ref_props`]), but never each other. Used to reject a composite's
/// ref prop pointing at another composite (a cycle risk the linear engine
/// isn't built to resolve) and to decide, in
/// [`crate::workflow::engine::widget_referenced_by`], which referrer names
/// need the "(widget)" suffix.
pub fn is_composite(type_tag: &str) -> bool {
    matches!(type_tag, "dialog" | "call" | "agent" | "meeting")
}

/// A usage-side entry point attached to a workflow. Tagged enum (`kind`)
/// so new trigger kinds (e.g. a selection popup menu) can be added
/// without a schema break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Global hotkey. `capture` is only meaningful for microphone-source
    /// workflows: "hold" (key-down starts, key-up finishes) or "toggle"
    /// (press starts, press again finishes). None means "hold".
    Hotkey {
        /// The key combo, e.g. `"cmd+shift+e"`.
        combo: String,
        /// `"hold"` or `"toggle"`; `None` means "hold". See variant docs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        capture: Option<String>,
    },
    /// A slot in the float pill's roller (microphone workflows only).
    PillSlot {
        /// Position in the roller, ascending.
        #[serde(default)]
        order: i64,
    },
}

/// A configured workflow: a source, an ordered chain of processors, and one
/// or more outputs, referenced by widget id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDef {
    /// Globally unique id. Builtins use fixed ids, e.g. `"wf.dictation"`,
    /// `"wf.listen"`; custom workflows use `"wf.custom-{uuid}"`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Display icon (emoji or icon key).
    #[serde(default)]
    pub icon: String,
    /// Hotkey tag that triggers this workflow. Empty means no trigger.
    #[serde(default)]
    pub hotkey: String,
    /// Usage-side triggers. Replaces the legacy `hotkey` field, which is
    /// kept only for config back-compat (consumed by
    /// migrate::migrate_hotkeys_to_triggers and normalized by save_workflow).
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    /// Id of the [`WidgetDef`] used as this workflow's source.
    pub source: String,
    /// Ids of the [`WidgetDef`]s used as this workflow's processors, in
    /// execution order.
    #[serde(default)]
    pub processors: Vec<String>,
    /// Ids of the [`WidgetDef`]s used as this workflow's outputs, in
    /// delivery order. Must be non-empty; enforced by the engine.
    pub outputs: Vec<String>,
    /// Whether this workflow ships with the app (builtins cannot be deleted).
    #[serde(default)]
    pub builtin: bool,
}

impl WorkflowDef {
    /// (index-in-triggers, combo, capture-with-default) for every Hotkey chip.
    pub fn hotkey_triggers(&self) -> impl Iterator<Item = (usize, &str, &str)> {
        self.triggers.iter().enumerate().filter_map(|(i, t)| match t {
            Trigger::Hotkey { combo, capture } => {
                Some((i, combo.as_str(), capture.as_deref().unwrap_or("hold")))
            }
            _ => None,
        })
    }

    /// The pill-roller slot order, if this workflow carries a PillSlot chip.
    pub fn pill_order(&self) -> Option<i64> {
        self.triggers.iter().find_map(|t| match t {
            Trigger::PillSlot { order } => Some(*order),
            _ => None,
        })
    }
}

/// Fixed pixel dimensions for a floating panel window (e.g. a Dialog output).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelSize {
    /// Panel width in logical pixels.
    #[serde(default = "dw")]
    pub width: u32,
    /// Panel height in logical pixels.
    #[serde(default = "dh")]
    pub height: u32,
}

fn dw() -> u32 {
    420
}

fn dh() -> u32 {
    320
}

impl Default for PanelSize {
    fn default() -> Self {
        Self { width: 420, height: 320 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_size_defaults_420x320() {
        assert_eq!(PanelSize::default(), PanelSize { width: 420, height: 320 });
        let from_empty: PanelSize = serde_json::from_str("{}").unwrap();
        assert_eq!(from_empty, PanelSize::default());
    }

    #[test]
    fn data_kind_and_conversions() {
        let t = Data::Text("hi".into());
        assert_eq!(t.kind(), DataKind::Text);
        assert_eq!(t.into_text().unwrap(), "hi");
        let a = Data::Audio(AudioBuf { samples: vec![0i16; 4], sample_rate: 16000 });
        assert_eq!(a.kind(), DataKind::Audio);
        assert!(a.into_text().is_err());
    }

    #[test]
    fn widget_def_serde_roundtrip() {
        let w = WidgetDef {
            id: "llm.polish".into(), role: WidgetRole::Processor, type_tag: "llm".into(),
            name: "润色".into(), icon: "✨".into(),
            props: serde_json::json!({"system": "s"}), builtin: true,
        };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"role\":\"processor\""));
        let back: WidgetDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, w);
    }

    #[test]
    fn workflow_def_defaults() {
        let json = r#"{"id":"wf.x","name":"X","source":"src.selection","outputs":["out.insert"]}"#;
        let wf: WorkflowDef = serde_json::from_str(json).unwrap();
        assert!(wf.hotkey.is_empty() && wf.processors.is_empty() && !wf.builtin);
    }

    #[test]
    fn trigger_serde_tagged() {
        let t: Trigger = serde_json::from_str(r#"{"kind":"hotkey","combo":"cmd+shift+e"}"#).unwrap();
        assert_eq!(t, Trigger::Hotkey { combo: "cmd+shift+e".into(), capture: None });
        let t2: Trigger =
            serde_json::from_str(r#"{"kind":"hotkey","combo":"cmd+shift+space","capture":"toggle"}"#)
                .unwrap();
        assert_eq!(t2, Trigger::Hotkey { combo: "cmd+shift+space".into(), capture: Some("toggle".into()) });
        let p: Trigger = serde_json::from_str(r#"{"kind":"pill_slot"}"#).unwrap();
        assert_eq!(p, Trigger::PillSlot { order: 0 });
        // 序列化不携带空 capture
        assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"kind":"hotkey","combo":"cmd+shift+e"}"#);
    }

    #[test]
    fn workflow_def_triggers_default_and_helpers() {
        // 旧配置（无 triggers 字段）必须能解析为 triggers=[]
        let json = r#"{"id":"wf.x","name":"X","source":"src.selection","outputs":["out.insert"]}"#;
        let wf: WorkflowDef = serde_json::from_str(json).unwrap();
        assert!(wf.triggers.is_empty());
        let wf2 = WorkflowDef {
            triggers: vec![
                Trigger::PillSlot { order: 10 },
                Trigger::Hotkey { combo: "cmd+shift+e".into(), capture: None },
                Trigger::Hotkey { combo: "cmd+shift+t".into(), capture: Some("toggle".into()) },
            ],
            ..wf
        };
        let hks: Vec<_> = wf2.hotkey_triggers().collect();
        assert_eq!(hks, vec![(1, "cmd+shift+e", "hold"), (2, "cmd+shift+t", "toggle")]);
        assert_eq!(wf2.pill_order(), Some(10));
    }
}

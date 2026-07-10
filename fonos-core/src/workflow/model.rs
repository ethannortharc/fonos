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

#[cfg(test)]
mod tests {
    use super::*;

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
}

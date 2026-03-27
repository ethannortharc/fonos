//! Mode system — built-in + custom modes for LLM post-processing.
//!
//! Custom modes are persisted to `{data_dir}/com.fonos.app/modes.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{Error, Result};

/// A processing mode that defines how spoken text is transformed by an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mode {
    /// Human-readable display name for the mode.
    pub name: String,
    /// Short description of what the mode does.
    #[serde(default)]
    pub description: String,
    /// Emoji or short icon string displayed next to the mode name.
    #[serde(default = "default_icon")]
    pub icon: String,
    /// Optional system prompt sent to the LLM.
    #[serde(default)]
    pub system: Option<String>,
    /// Optional user message template; use `{text}` as a placeholder for the transcribed text.
    #[serde(default)]
    pub user_template: Option<String>,
    /// LLM sampling temperature (0.0 = deterministic).
    #[serde(default = "default_temp")]
    pub temperature: f64,
    /// Override LLM model identifier; empty string means use the configured LLM profile.
    #[serde(default)]
    pub model: String,
    /// Override STT model identifier; empty string means use the configured STT profile.
    #[serde(default)]
    pub stt_model: String,
    /// Optional initial prompt hint for STT (guides vocabulary/style recognition).
    #[serde(default)]
    pub stt_prompt: String,
    /// STT sampling temperature (0.0 = most deterministic). Only used if > 0.
    #[serde(default)]
    pub stt_temperature: f64,
    /// Maximum tokens to request from the LLM.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Desired output language; `"auto"` means preserve the input language.
    #[serde(default = "default_output_lang")]
    pub output_language: String,
    /// Whether to automatically paste the result into the focused text field.
    #[serde(default = "default_true")]
    pub auto_paste: bool,
    /// Whether to press Enter after pasting the result.
    #[serde(default)]
    pub auto_press_enter: bool,
}

impl Default for Mode {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            icon: default_icon(),
            system: None,
            user_template: None,
            temperature: default_temp(),
            model: String::new(),
            stt_model: String::new(),
            stt_prompt: String::new(),
            stt_temperature: 0.0,
            max_tokens: default_max_tokens(),
            output_language: default_output_lang(),
            auto_paste: true,
            auto_press_enter: false,
        }
    }
}

fn default_temp() -> f64 { 0.1 }
fn default_icon() -> String { "⚙️".into() }
fn default_max_tokens() -> u32 { 4096 }
fn default_output_lang() -> String { "auto".into() }
fn default_true() -> bool { true }

/// Returns the set of built-in modes that ship with Fonos.
pub fn built_in_modes() -> BTreeMap<String, Mode> {
    let mut m = BTreeMap::new();

    m.insert("raw".into(), Mode {
        name: "Raw".into(),
        description: "No processing, direct STT output".into(),
        icon: "📝".into(),
        temperature: 0.0,
        ..Default::default()
    });

    m.insert("polish".into(), Mode {
        name: "Polish".into(),
        description: "Speech to natural writing, preserves emotion and tone".into(),
        icon: "✨".into(),
        system: Some("You are a speech-to-writing assistant.".into()),
        user_template: Some(concat!(
            "Convert the following spoken text into natural, well-written text. ",
            "Preserve the speaker's intent, emotion, and tone intensity — if they are angry, ",
            "the output should feel angry; if they are excited, it should feel excited. ",
            "Remove only speech artifacts (filler words, false starts, repetitions). ",
            "Do not add new ideas. Do not make the tone more formal or neutral unless ",
            "the original tone is neutral. ",
            "Keep the original language. Output ONLY the polished text.\n\n",
            "{text}"
        ).into()),
        temperature: 0.1,
        ..Default::default()
    });

    m.insert("formal".into(), Mode {
        name: "Formal".into(),
        description: "Professional business writing".into(),
        icon: "👔".into(),
        system: Some("You are a professional writing assistant.".into()),
        user_template: Some(concat!(
            "Rewrite the following spoken text as professional written communication. ",
            "Clear, concise, neutral tone. Remove colloquialisms and emotional expressions. ",
            "Keep the original language. Output ONLY the rewritten text.\n\n",
            "{text}"
        ).into()),
        temperature: 0.2,
        ..Default::default()
    });

    m.insert("translate".into(), Mode {
        name: "Translate".into(),
        description: "Translate to target language (configured in Settings)".into(),
        icon: "🌐".into(),
        system: Some("You are a translator.".into()),
        user_template: Some(concat!(
            "Translate the following text to {target_lang}. ",
            "Preserve the tone and intent. ",
            "Output ONLY the translation.\n\n",
            "{text}"
        ).into()),
        temperature: 0.3,
        ..Default::default()
    });

    m
}

/// Load custom modes from `{data_dir}/com.fonos.app/modes.json`.
///
/// Returns an empty map if the file does not exist or cannot be parsed.
pub fn load_custom_modes() -> BTreeMap<String, Mode> {
    let path = custom_modes_path();
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

/// Save custom modes to `{data_dir}/com.fonos.app/modes.json`.
pub fn save_custom_modes(modes: &BTreeMap<String, Mode>) -> Result<()> {
    let path = custom_modes_path();
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| Error::Config(e.to_string()))?;
    let json = serde_json::to_string_pretty(modes).map_err(|e| Error::Config(e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| Error::Config(format!("save modes: {e}")))
}

/// Returns all modes: built-in modes merged with any user-defined custom modes.
///
/// Custom modes with the same key as a built-in override the built-in.
pub fn all_modes() -> BTreeMap<String, Mode> {
    let mut all = built_in_modes();
    all.extend(load_custom_modes());
    all
}

/// Returns the path to the custom modes JSON file.
fn custom_modes_path() -> PathBuf {
    dirs::data_dir()
        .expect("could not resolve data directory")
        .join("com.fonos.app")
        .join("modes.json")
}

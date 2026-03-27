//! Application configuration — load, save, and defaults.
//!
//! Config is stored as JSON at the platform's data directory:
//! `{data_dir}/com.fonos.app/config.json`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Error, Result};

/// Application configuration persisted to disk as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Global hotkey combo for triggering dictation.
    pub hotkey_dictation: String,
    /// Global hotkey combo for triggering TTS playback.
    pub hotkey_tts: String,
    /// Default dictation processing mode (e.g. `"raw"`, `"polish"`).
    pub dictation_mode: String,
    /// Default TTS voice identifier.
    pub default_voice: String,
    /// TTS playback speed multiplier (1.0 = normal).
    pub tts_speed: f64,
    /// Preferred audio input device name, or `"default"`.
    pub audio_input_device: String,
    /// Preferred audio output device name, or `"default"`.
    pub audio_output_device: String,
    /// Whether to show the floating recording indicator pill.
    pub show_floating_indicator: bool,
    /// STT language hint (BCP-47 tag or `"auto"`).
    pub stt_language: String,
    /// Named model profiles: JSON array of `{id, name, provider, api_key, model, base_url, capabilities[]}`.
    pub model_profiles: Vec<serde_json::Value>,
    /// Which profile ID to use for speech-to-text.
    pub stt_profile: String,
    /// Which profile ID to use for text-to-speech.
    pub tts_profile: String,
    /// Which profile ID to use for LLM post-processing.
    pub llm_profile: String,
    /// System prompt used by the "clean" dictation mode.
    pub clean_prompt: String,
    /// Source language for translation mode (`"auto"` = detect).
    pub translate_source: String,
    /// Target language for translation mode.
    pub translate_target: String,

    // ── Agent settings ────────────────────────────────────────────────────

    /// Which model profile ID to use for agent LLM calls (independent from `llm_profile`).
    /// Empty string means "fall back to `llm_profile`".
    pub agent_llm_profile: String,
    /// System prompt injected into every agent LLM request.
    pub agent_system_prompt: String,
    /// Extra commands to allow beyond the built-in safety allowlist.
    pub agent_safety_allowlist: Vec<String>,
    /// Extra commands to block beyond the built-in safety blocklist.
    pub agent_safety_blocklist: Vec<String>,
    /// Maximum wall-clock seconds allowed for a single skill execution (default 30).
    pub agent_timeout_secs: u64,
    /// Maximum number of user/assistant turn pairs to keep in conversation context (default 20).
    pub agent_max_turns: usize,
    /// Whether to speak agent responses via TTS after each reply.
    pub agent_tts_enabled: bool,
    /// Global hotkey combo for press-and-hold agent voice input.
    pub hotkey_agent: String,
    /// Global hotkey combo for toggling the agent panel view.
    pub hotkey_agent_panel: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey_dictation: "cmd+shift+space".to_string(),
            hotkey_tts: "cmd+shift+s".to_string(),
            dictation_mode: "raw".to_string(),
            default_voice: "default".to_string(),
            tts_speed: 1.0,
            audio_input_device: "default".to_string(),
            audio_output_device: "default".to_string(),
            show_floating_indicator: true,
            stt_language: "auto".to_string(),
            model_profiles: vec![],
            stt_profile: String::new(),
            tts_profile: String::new(),
            llm_profile: String::new(),
            clean_prompt: "Remove filler words (um, uh, 嗯, 就是说, 然后), fix punctuation and capitalization. Output ONLY the cleaned text, nothing else.".to_string(),
            translate_source: "auto".to_string(),
            translate_target: "English".to_string(),
            agent_llm_profile: String::new(),
            agent_system_prompt: "You are a helpful macOS desktop assistant. You can run shell commands, execute AppleScript, control apps, read the clipboard, and query system information. Your responses will be spoken aloud, so keep them to 1-2 sentences maximum. Give only the essential answer — no explanations, no caveats, no formatting. When you run a command, report just the key result.".to_string(),
            agent_safety_allowlist: Vec::new(),
            agent_safety_blocklist: Vec::new(),
            agent_timeout_secs: 30,
            agent_max_turns: 20,
            agent_tts_enabled: false,
            hotkey_agent: "cmd+shift+a".to_string(),
            hotkey_agent_panel: "cmd+shift+g".to_string(),
        }
    }
}

impl AppConfig {
    /// Returns the directory where the config file is stored.
    ///
    /// On macOS this resolves to `~/Library/Application Support/com.fonos.app`.
    pub fn config_dir() -> PathBuf {
        dirs::data_dir()
            .expect("could not resolve data directory")
            .join("com.fonos.app")
    }

    /// Returns the full path to the config JSON file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    /// Load config from disk.
    ///
    /// Missing fields fall back to defaults via `#[serde(default)]`.
    /// If the file does not exist or cannot be read, returns the default config.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the config to disk using an atomic write (write to temp file, then rename).
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::Config(e.to_string()))?;

        let path = Self::config_path();
        let tmp_path = path.with_extension("json.tmp");

        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(&tmp_path, &json).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| Error::Config(e.to_string()))?;

        Ok(())
    }
}

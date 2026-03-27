//! Tauri command handlers exposed to the frontend via invoke().

pub mod agent;
pub mod config;
pub mod dictation;
pub mod llm;
pub mod stats;
pub mod tts;
pub mod voices;

use std::sync::{Arc, Mutex};

use crate::audio::capture::AudioCapture;
use crate::audio::playback::AudioPlayback;
use fonos_core::config::AppConfig;

/// Hide the agent-panel window and stop any TTS playback.
#[tauri::command]
pub fn hide_agent_panel(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use tauri::Manager;
    // Stop TTS so the user doesn't hear audio after the panel is gone
    let _ = tts::stop_playback(state);
    if let Some(w) = app.get_webview_window("agent-panel") {
        let _ = w.hide();
    }
    Ok(())
}

/// Resize the agent-panel window, keeping it centered at its current position.
#[tauri::command]
pub fn resize_agent_panel(app: tauri::AppHandle, width: u32, height: u32) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("agent-panel") {
        let old_size = w.outer_size().map_err(|e| e.to_string())?;
        let old_pos = w.outer_position().map_err(|e| e.to_string())?;

        // Keep the top-left corner anchored (let height grow downward)
        let new_x = old_pos.x + (old_size.width as i32 - width as i32) / 2;
        let new_y = old_pos.y;

        w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width, height)))
            .map_err(|e| e.to_string())?;
        w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, new_y)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Resize the float window and adjust position so the pill stays at the bottom of the screen.
#[tauri::command]
pub fn resize_float(app: tauri::AppHandle, width: u32, height: u32) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("float") {
        let old_size = w.outer_size().map_err(|e| e.to_string())?;
        let old_pos = w.outer_position().map_err(|e| e.to_string())?;

        let bottom = old_pos.y + old_size.height as i32;
        let new_y = bottom - height as i32;
        let new_x = old_pos.x + (old_size.width as i32 - width as i32) / 2;

        w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width, height)))
            .map_err(|e| e.to_string())?;
        w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, new_y)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Service connection info: URL + optional API key + model name
pub struct ServiceConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub provider: String,
}

/// Build a ServiceConfig from a JSON model profile entry.
pub fn config_from_profile(profile: &serde_json::Value) -> ServiceConfig {
    let url = profile["base_url"].as_str().unwrap_or("").to_string();
    ServiceConfig {
        base_url: if url.is_empty() {
            let provider = profile["provider"].as_str().unwrap_or("");
            match provider {
                "omlx" => "http://localhost:8000".to_string(),
                "ollama" => "http://localhost:11434".to_string(),
                "openai" => "https://api.openai.com".to_string(),
                "anthropic" => "https://api.anthropic.com".to_string(),
                "google" => "https://generativelanguage.googleapis.com".to_string(),
                _ => String::new(),
            }
        } else {
            url.trim_end_matches('/').to_string()
        },
        api_key: profile["api_key"].as_str().unwrap_or("").to_string(),
        model: profile["model"].as_str().unwrap_or("").to_string(),
        provider: profile["provider"].as_str().unwrap_or("").to_string(),
    }
}

fn empty_service_config() -> ServiceConfig {
    ServiceConfig { base_url: String::new(), api_key: String::new(), model: String::new(), provider: String::new() }
}

/// Get connection info for a service by reading the active model profile.
pub fn get_service_config(state: &AppState, service: &str) -> ServiceConfig {
    let Ok(config) = state.config.lock() else { return empty_service_config(); };

    let profile_id = match service {
        "stt" => &config.stt_profile,
        "tts" => &config.tts_profile,
        _ => &config.llm_profile,
    };

    if profile_id.is_empty() { return empty_service_config(); }

    config.model_profiles.iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .map(config_from_profile)
        .unwrap_or_else(empty_service_config)
}

/// Get connection info for a specific model profile by its ID.
pub fn get_service_config_for_profile(state: &AppState, profile_id: &str) -> ServiceConfig {
    let Ok(config) = state.config.lock() else { return empty_service_config(); };

    config.model_profiles.iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .map(config_from_profile)
        .unwrap_or_else(empty_service_config)
}

/// Shared application state.
pub struct AppState {
    pub audio_capture: Arc<Mutex<Option<AudioCapture>>>,
    pub audio_playback: Arc<Mutex<Option<AudioPlayback>>>,
    pub config: Arc<Mutex<AppConfig>>,
    pub db: Arc<Mutex<rusqlite::Connection>>,
    /// Mutable agent state: skill registry + conversation context.
    /// Uses `tokio::sync::Mutex` so the lock can be held across `.await` points
    /// in async Tauri commands.
    pub agent: Arc<tokio::sync::Mutex<agent::AgentState>>,
}

//! Platform adapters implementing the fonos-core pipeline ports.
//!
//! The core pipeline speaks [`PipelineEvent`] and [`TextSink`]; these adapters
//! translate to the desktop surfaces: the float pill's `float:*` Tauri events
//! and CGEvent/xdotool text injection.

use fonos_core::pipeline::{EventSink, PipelineEvent, TextSink};
use tauri::Emitter;

/// Emits pipeline events as the float pill's `float:*` Tauri events.
pub struct PillEventSink(pub tauri::AppHandle);

impl EventSink for PillEventSink {
    fn emit(&self, event: PipelineEvent) {
        match event {
            PipelineEvent::Processing => {
                let _ = self.0.emit("float:processing", ());
            }
            PipelineEvent::Delivered(text) => {
                let _ = self.0.emit("float:stop", &text);
            }
            PipelineEvent::NoSpeech => {
                let _ = self.0.emit("float:stop", "");
            }
            PipelineEvent::Failed(surfaced) => {
                crate::error_surface::emit_surfaced(&self.0, &surfaced);
            }
        }
    }
}

/// Delivers text at the cursor via the injection module, resolving the
/// per-app strategy from the live config.
pub struct InjectionTextSink(pub std::sync::Arc<std::sync::Mutex<fonos_core::config::AppConfig>>);

impl TextSink for InjectionTextSink {
    fn inject(&self, text: &str) -> Result<(), String> {
        let cfg = self.0.lock().map(|c| c.clone()).unwrap_or_default();
        crate::injection::inject_text(text, &cfg).map(|_| ())
    }

    fn press_enter(&self) -> Result<(), String> {
        crate::injection::press_enter()
    }
}

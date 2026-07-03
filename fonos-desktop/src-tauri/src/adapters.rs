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

use fonos_core::sts::{AudioOut, TurnEvent, TurnSink};
use std::sync::Mutex;

/// Renders STS conversation turns on the float pill: busy while the reply is
/// being produced/spoken, Done with the reply text at the end, classified
/// errors on failure.
pub struct PillTurnSink {
    app: tauri::AppHandle,
    reply: Mutex<String>,
}

impl PillTurnSink {
    /// Wrap an app handle.
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app, reply: Mutex::new(String::new()) }
    }
}

impl TurnSink for PillTurnSink {
    fn emit(&self, event: TurnEvent) {
        match event {
            TurnEvent::Transcript(_) | TurnEvent::SpeakingStarted => {
                let _ = self.app.emit("float:processing", ());
            }
            TurnEvent::Reply(text) => {
                *self.reply.lock().unwrap() = text;
            }
            TurnEvent::SpeakingDone => {}
            TurnEvent::TurnDone => {
                let reply = self.reply.lock().unwrap().clone();
                let _ = self.app.emit("float:stop", &reply);
            }
            TurnEvent::Failed(surfaced) => {
                crate::error_surface::emit_surfaced(&self.app, &surfaced);
            }
        }
    }
}

/// Plays WAVs through the shared output device, returning when playback has
/// (by duration) completed. Uses the WAV header duration rather than holding
/// the playback lock, so pause/stop stay responsive.
pub struct PlaybackAudioOut(
    pub std::sync::Arc<std::sync::Mutex<Option<crate::audio::playback::AudioPlayback>>>,
);

#[async_trait::async_trait]
impl AudioOut for PlaybackAudioOut {
    async fn play_wav(&self, wav: Vec<u8>) -> Result<(), String> {
        let duration = fonos_core::listen::wav_duration_secs(&wav).unwrap_or(0.0);
        {
            let mut guard = self.0.lock().map_err(|e| e.to_string())?;
            if guard.is_none() {
                *guard = Some(
                    crate::audio::playback::AudioPlayback::new().map_err(|e| e.to_string())?,
                );
            }
            guard
                .as_ref()
                .unwrap()
                .play_wav(wav)
                .map_err(|e| e.to_string())?;
        }
        tokio::time::sleep(std::time::Duration::from_secs_f64(duration + 0.15)).await;
        Ok(())
    }
}

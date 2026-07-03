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

/// Streams synthesized PCM straight into the shared output device: audio is
/// audible while later sentences are still being generated. `finish()` polls
/// the queue without holding the playback lock, so pause/stop stay live.
pub struct PlaybackAudioOut {
    playback: std::sync::Arc<std::sync::Mutex<Option<crate::audio::playback::AudioPlayback>>>,
    format: Mutex<(u32, u16)>,
}

impl PlaybackAudioOut {
    /// Wrap the shared playback slot from `AppState`.
    pub fn new(
        playback: std::sync::Arc<std::sync::Mutex<Option<crate::audio::playback::AudioPlayback>>>,
    ) -> Self {
        Self { playback, format: Mutex::new((16000, 1)) }
    }
}

impl fonos_core::tts::PcmSink for PlaybackAudioOut {
    fn begin(&self, sample_rate: u32, channels: u16) -> Result<(), String> {
        *self.format.lock().unwrap() = (sample_rate, channels);
        let mut guard = self.playback.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard =
                Some(crate::audio::playback::AudioPlayback::new().map_err(|e| e.to_string())?);
        }
        Ok(())
    }

    fn push(&self, pcm: &[u8]) -> Result<(), String> {
        let (rate, channels) = *self.format.lock().unwrap();
        let guard = self.playback.lock().map_err(|e| e.to_string())?;
        guard
            .as_ref()
            .ok_or("playback not initialized")?
            .append_pcm(rate, channels, pcm)
            .map_err(|e| e.to_string())
    }
}

#[async_trait::async_trait]
impl AudioOut for PlaybackAudioOut {
    async fn finish(&self) -> Result<(), String> {
        loop {
            let empty = {
                let guard = self.playback.lock().map_err(|e| e.to_string())?;
                guard.as_ref().map(|p| p.queue_empty()).unwrap_or(true)
            };
            if empty {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    }
}

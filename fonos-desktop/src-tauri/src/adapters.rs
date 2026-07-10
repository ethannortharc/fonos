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
            PipelineEvent::Delivered { raw, final_text, workflow } => {
                // Pill contract unchanged: the final text drives `float:stop`.
                let _ = self.0.emit("float:stop", &final_text);
                // Engine (workflow) runs additionally surface the raw transcript
                // and final result so the Dictation feed can show both, labeled
                // by the run's workflow. This sink is the single emitter of
                // `workflow:done`; non-workflow deliveries carry `None` and skip
                // it.
                if let Some(workflow_id) = workflow {
                    let _ = self.0.emit(
                        "workflow:done",
                        serde_json::json!({
                            "raw": raw,
                            "final": final_text,
                            "workflow_id": workflow_id,
                        }),
                    );
                }
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
///
/// Constructed only on the Linux dictation path (`stop_and_process_dictation`,
/// `cfg(target_os = "linux")`); macOS dictation now runs through the workflow
/// engine's `insert` output, so this is dead code on the macOS build.
#[allow(dead_code)]
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
use std::sync::{Arc, Mutex};

/// Bridges STS turn events to renderers: always mirrors them onto the main
/// window as `sts:event` (the Conversation page subscribes), and — for
/// hotkey-initiated turns — also drives the float pill lifecycle.
pub struct TurnEventBridge {
    app: tauri::AppHandle,
    pill: bool,
    /// The reply text of the turn in flight, updated on [`TurnEvent::Reply`].
    /// Shared so the call loop's barge monitor can read what is being spoken
    /// right now (the content the echo verifier compares a snippet against).
    reply: Arc<Mutex<String>>,
}

impl TurnEventBridge {
    /// `pill: true` for hotkey turns (pill shows progress); `false` for turns
    /// started from the in-app Conversation page.
    pub fn new(app: tauri::AppHandle, pill: bool) -> Self {
        Self { app, pill, reply: Arc::new(Mutex::new(String::new())) }
    }

    /// A shared handle to the live reply text (updated on each
    /// [`TurnEvent::Reply`]). Call mode reads this in its barge monitor to
    /// compare a suspected-barge snippet against what the assistant is saying.
    pub fn reply_handle(&self) -> Arc<Mutex<String>> {
        Arc::clone(&self.reply)
    }

    fn page(&self, kind: &str, text: &str) {
        let _ = self
            .app
            .emit("sts:event", serde_json::json!({ "kind": kind, "text": text }));
    }
}

impl TurnSink for TurnEventBridge {
    fn emit(&self, event: TurnEvent) {
        match event {
            TurnEvent::Transcript(t) => {
                self.page("transcript", &t);
                if self.pill {
                    let _ = self.app.emit("float:processing", ());
                }
            }
            TurnEvent::Reply(text) => {
                self.page("reply", &text);
                *self.reply.lock().unwrap() = text;
            }
            TurnEvent::SpeakingStarted => {
                self.page("speaking_started", "");
                if self.pill {
                    let _ = self.app.emit("float:processing", ());
                }
            }
            TurnEvent::SpeakingDone => self.page("speaking_done", ""),
            TurnEvent::TurnDone => {
                self.page("turn_done", "");
                if self.pill {
                    let reply = self.reply.lock().unwrap().clone();
                    let _ = self.app.emit("float:stop", &reply);
                }
            }
            TurnEvent::Failed(surfaced) => {
                self.page("error", &surfaced.message);
                if self.pill {
                    crate::error_surface::emit_surfaced(&self.app, &surfaced);
                }
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

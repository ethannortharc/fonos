//! Platform adapters implementing the fonos-core pipeline ports.
//!
//! The core pipeline speaks [`PipelineEvent`]; these adapters translate it to
//! the desktop surfaces — most visibly the float pill's `float:*` Tauri events.
//! (Text injection at the cursor is handled inside the engine's own `insert`
//! output, not here — see `commands::workflow_widgets`.)

use fonos_core::pipeline::{EventSink, PipelineEvent};
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
            PipelineEvent::EmptyInput => {
                // A dedicated pill event, NOT `float:stop("")`: float.html's
                // stopRec("") renders its "no speech" state, whose label
                // literally reads "No speech"/"未识别到语音" — factually wrong
                // here, since nothing was ever listened for. `float:empty-input`
                // gets float.html's own state (same red-cross flash visuals and
                // ~1.2s duration as stopRec's error branch) with an accurate
                // label ("No text selected"/"未选中文本"). Deliberately NOT
                // `float:error` (error_surface.rs) either: that surface is for
                // classified *errors* — clickable, System-Settings-paned,
                // English-only — semantically wrong for a localized notice.
                //
                // The richer, actionable explanation still rides on the OS
                // notice below — but that notice's permission can be denied
                // (silently, by design: a notice must never gate the flow), so
                // the pill's own label must never depend on it being shown.
                let _ = self.0.emit("float:empty-input", ());
                crate::tray::notify_empty_input(&self.0);
            }
            PipelineEvent::Failed(surfaced) => {
                crate::error_surface::emit_surfaced(&self.0, &surfaced);
            }
            // Per-step test-run tracing has no float pill surface; the Test
            // Run bench (a later task) uses its own sink for these.
            PipelineEvent::StepStarted { .. } | PipelineEvent::StepFinished { .. } => {}
        }
    }
}

use fonos_core::sts::{AudioOut, TurnEvent, TurnSink};
use std::sync::{Arc, Mutex};

/// Bridges STS turn events to renderers: always mirrors them app-wide as
/// `sts:event` (the call-panel satellite subscribes), and — for
/// hotkey-initiated turns — also drives the float pill lifecycle (`pill:
/// true` has no live caller since the walkie mode's retirement, Workbench P2
/// Task 9; the call loop always constructs this with `pill: false`).
pub struct TurnEventBridge {
    app: tauri::AppHandle,
    pill: bool,
    /// The reply text of the turn in flight, updated on [`TurnEvent::Reply`].
    /// Shared so the call loop's barge monitor can read what is being spoken
    /// right now (the content the echo verifier compares a snippet against).
    reply: Arc<Mutex<String>>,
}

impl TurnEventBridge {
    /// `pill: true` for hotkey turns (pill shows progress); `false` for
    /// call-loop turns (the call panel renders progress instead).
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

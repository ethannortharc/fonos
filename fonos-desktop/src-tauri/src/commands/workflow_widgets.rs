//! Desktop adapters that bridge the platform-independent workflow component
//! traits ([`Source`], [`Processor`]) to real macOS capture: the selection
//! grabber, the two-phase microphone source, and the STT processor.
//!
//! These are the concrete widgets a workflow's `type_tag`s resolve to. They are
//! **not** registered with a `Registry` yet — Task 8 (`build_registry`) wires
//! them in — so until then the compiler reports them as dead code.
//!
//! Capture discipline: [`MicSource`] does not re-implement the `IS_RECORDING` +
//! `state.audio_capture` lock invariant; it reuses [`dictation::start_recording`]
//! (start) and [`dictation::stop_and_drain`] (stop + drain) so the single lock
//! discipline in `dictation.rs` stays authoritative.

use serde::Deserialize;

use tauri::Manager;

use fonos_core::workflow::model::{AudioBuf, Data, DataKind};
use fonos_core::workflow::registry::{Processor, RunCtx, Source};

use super::dictation;
use super::AppState;

/// Signalled by [`finish_active_capture`] to end the microphone capture phase of
/// an in-flight [`MicSource`] run (hotkey released in hold mode, or pressed a
/// second time in toggle mode).
///
/// `notify_waiters()` only wakes waiters that are *already* registered, so
/// [`MicSource::acquire`] enables its `Notified` before bringing the mic up.
static CAPTURE_DONE: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// End the microphone capture phase of the active [`MicSource`] run. Called by
/// the trigger layer on hotkey up (hold) / second press (toggle); it only sends
/// the signal — starting/stopping the stream stays inside `MicSource::acquire`.
pub fn finish_active_capture() {
    CAPTURE_DONE.notify_waiters();
}

// ─── Selection source ────────────────────────────────────────────────────────

/// Source that grabs the current selection from the frontmost app as text, and
/// stashes the app name + editability into `ctx.meta` for downstream components
/// (e.g. a `replace` output that must refocus and paste into the same app).
pub struct SelectionSource;

#[async_trait::async_trait]
impl Source for SelectionSource {
    fn output_kind(&self) -> DataKind {
        DataKind::Text
    }

    async fn acquire(&self, ctx: &RunCtx) -> Result<Data, String> {
        let selection = super::selection::grab_selection().await?;

        // Hand the source app context to downstream components via meta. The
        // grab completed above, so no std Mutex is held across an await here.
        if let Ok(mut meta) = ctx.meta.lock() {
            meta.insert(
                "app_name".to_string(),
                serde_json::Value::String(selection.app_name.clone()),
            );
            meta.insert(
                "editable".to_string(),
                serde_json::Value::Bool(selection.editable),
            );
        }

        // Empty selection is returned as-is; the engine maps empty text to NoSpeech.
        Ok(Data::Text(selection.text))
    }
}

// ─── Microphone source (two-phase) ───────────────────────────────────────────

/// Two-phase microphone source: start capture, block until
/// [`finish_active_capture`] fires, then stop and hand the drained samples on as
/// audio.
///
/// `capture` (`"hold"` | `"toggle"`) records how the trigger layer decides *when*
/// to call [`finish_active_capture`]; from this source's view both modes are the
/// same start → await → stop sequence, so `acquire` does not branch on it.
pub struct MicSource {
    /// `"hold"` (finish on key release) or `"toggle"` (finish on second press).
    pub capture: String,
    /// Handle used to reach `AppState` and drive the existing capture path.
    pub app: tauri::AppHandle,
}

#[async_trait::async_trait]
impl Source for MicSource {
    fn output_kind(&self) -> DataKind {
        DataKind::Audio
    }

    async fn acquire(&self, _ctx: &RunCtx) -> Result<Data, String> {
        eprintln!("fonos: MicSource acquire (capture={})", self.capture);

        // Register as a CAPTURE_DONE waiter BEFORE bringing the mic up: a very
        // fast finish (immediate release / re-press) must not slip through the
        // gap between start and await, and notify_waiters() drops signals that
        // have no registered waiter.
        let notified = CAPTURE_DONE.notified();
        tokio::pin!(notified);
        let _ = notified.as_mut().enable();

        // Phase 1 — start capture through dictation's existing start path, which
        // owns the IS_RECORDING + audio_capture lock discipline. skip_float=false
        // so the recording pill appears exactly as in a normal dictation.
        {
            let state = self.app.state::<AppState>();
            dictation::start_recording(self.app.clone(), state, Some(false)).await?;
        }

        // Phase 2 — block until the trigger layer signals the end of capture.
        notified.await;

        // Stop + drain via the shared stop-path inner (same lock discipline as
        // stop_recording, minus its transcribe/inject/stats side effects). A
        // `None` (nothing was recording) yields an empty buffer.
        let samples = {
            let state = self.app.state::<AppState>();
            dictation::stop_and_drain(state.inner())?.unwrap_or_default()
        };

        Ok(Data::Audio(AudioBuf {
            samples,
            sample_rate: 16000,
        }))
    }
}

// ─── STT processor ───────────────────────────────────────────────────────────

/// Configuration for [`SttProcessor`], deserialized from a widget's `props`.
#[derive(Debug, Clone, Deserialize)]
pub struct SttProps {
    /// STT model profile id: `"apple-speech"` sentinel, a specific profile id,
    /// or empty to fall back to the global `"stt"` profile.
    #[serde(default)]
    pub model_profile: String,
    /// Whisper initial prompt (style/vocabulary hint) for this widget.
    #[serde(default)]
    pub stt_prompt: String,
    /// Extra vocab book ids mounted on top of the global books.
    #[serde(default)]
    pub vocab_books: Vec<String>,
    /// STT sampling temperature (0.0 = most deterministic).
    #[serde(default)]
    pub temperature: f64,
}

/// Processor that transcribes an audio buffer to text via the shared STT core.
///
/// Mirrors `dictation::transcribe_core`: resolve the STT service by profile
/// (`"apple-speech"` sentinel / profile id / global `"stt"`), bias with the
/// effective vocab books (global ∪ this widget's), transcribe, then apply the
/// same deterministic vocab post-correction.
pub struct SttProcessor {
    /// Handle used to reach `AppState` for service + vocab resolution.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: SttProps,
}

#[async_trait::async_trait]
impl Processor for SttProcessor {
    fn input_kind(&self) -> DataKind {
        DataKind::Audio
    }

    fn output_kind(&self) -> DataKind {
        DataKind::Text
    }

    async fn process(&self, input: Data, _ctx: &RunCtx) -> Result<Data, String> {
        let audio = input.into_audio()?;

        // Resolve everything that needs AppState up front, then drop the State +
        // config lock before the STT await — no std Mutex (or State) held across
        // an .await point.
        let (svc, language, vocab_books) = {
            let state = self.app.state::<AppState>();

            // Service resolution mirrors transcribe_core's rule.
            let svc = if self.props.model_profile == "apple-speech" {
                fonos_core::llm::ServiceConfig {
                    base_url: String::new(),
                    api_key: String::new(),
                    model: "apple-speech".to_string(),
                    provider: "apple".to_string(),
                    stt_api: "whisper".to_string(),
                }
            } else if !self.props.model_profile.is_empty() {
                super::get_service_config_for_profile(&state, &self.props.model_profile)
            } else {
                super::get_service_config(&state, "stt")
            };

            // Effective vocab books = global ∪ this widget's, cloned out of the
            // config lock so they outlive the transcription await.
            let (language, books) = {
                let config = state.config.lock().map_err(|e| e.to_string())?;
                let books: Vec<fonos_core::vocab::VocabBook> = fonos_core::vocab::effective_books(
                    &config.vocab_books,
                    &config.global_vocab_books,
                    &self.props.vocab_books,
                )
                .into_iter()
                .cloned()
                .collect();
                (config.stt_language.clone(), books)
            };

            (svc, language, books)
        };

        let vocab_refs: Vec<&fonos_core::vocab::VocabBook> = vocab_books.iter().collect();
        let vocab_terms = fonos_core::vocab::collect_terms(&vocab_refs);

        // Shared STT core — no float-pill side effects; the engine surfaces any
        // failure as a Failed event via classify_error.
        let out = dictation::stt_transcribe(
            audio.samples,
            svc,
            language,
            self.props.stt_prompt.clone(),
            vocab_terms,
            self.props.temperature,
        )
        .await?;

        // Same deterministic post-correction the dictation path applies.
        let transcript = if vocab_refs.is_empty() {
            out.transcript
        } else {
            fonos_core::vocab::apply_rules(&out.transcript, &vocab_refs)
        };

        Ok(Data::Text(transcript))
    }
}

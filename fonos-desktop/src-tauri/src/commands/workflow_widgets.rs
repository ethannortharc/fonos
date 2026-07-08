//! Desktop adapters that bridge the platform-independent workflow component
//! traits ([`Source`], [`Processor`], [`Output`]) to real macOS behavior: the
//! selection grabber, the two-phase microphone source, the STT / LLM
//! processors, and the six terminal outputs (insert / replace / clipboard /
//! notebook / speak / panel).
//!
//! These are the concrete widgets a workflow's `type_tag`s resolve to.
//! [`build_registry`] wires every factory into one [`Registry`], which the
//! engine uses to instantiate a [`crate::commands::workflow_widgets`] widget
//! from its persisted `WidgetDef`.
//!
//! Capture discipline: [`MicSource`] does not re-implement the `IS_RECORDING` +
//! `state.audio_capture` lock invariant; it reuses [`dictation::start_recording`]
//! (start) and [`dictation::stop_and_drain`] (stop + drain) so the single lock
//! discipline in `dictation.rs` stays authoritative.
//!
//! Lock discipline: `ctx.meta` is a `std::sync::Mutex`, so every component here
//! reads/writes it in a tight scope and drops the guard **before** any `.await`
//! (and never holds a `tauri::State` or config lock across an await either).

use std::sync::Arc;

use serde::Deserialize;

use tauri::Manager;

use fonos_core::workflow::llm_step::{run_llm_step, LlmProps};
use fonos_core::workflow::model::{AudioBuf, Data, DataKind};
use fonos_core::workflow::registry::{Output, Processor, Registry, RunCtx, Source};

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

// ─── LLM processor ────────────────────────────────────────────────────────────

/// Processor that runs one LLM step over text: resolve the service (by profile
/// id, or the global `"llm"` profile when `model_profile` is empty), assemble
/// the effective vocab glossary (global ∪ this widget's books), and call the
/// shared pure runner [`run_llm_step`].
///
/// Mirrors the LLM half of `commands/text_action.rs`, but takes the translation
/// target from [`RunCtx::translate_target`] (set by the engine) rather than the
/// live config.
pub struct LlmProcessor {
    /// Handle used to reach `AppState` for service + vocab resolution.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: LlmProps,
}

#[async_trait::async_trait]
impl Processor for LlmProcessor {
    fn input_kind(&self) -> DataKind {
        DataKind::Text
    }

    fn output_kind(&self) -> DataKind {
        DataKind::Text
    }

    async fn process(&self, input: Data, ctx: &RunCtx) -> Result<Data, String> {
        let text = input.into_text()?;

        // Resolve the LLM service and precompute the glossary block up front,
        // then drop the State + config lock before the network await.
        let (service, glossary) = {
            let state = self.app.state::<AppState>();
            let service = if self.props.model_profile.is_empty() {
                super::get_service_config(&state, "llm")
            } else {
                super::get_service_config_for_profile(&state, &self.props.model_profile)
            };
            let glossary = {
                let config = state.config.lock().map_err(|e| e.to_string())?;
                let books = fonos_core::vocab::effective_books(
                    &config.vocab_books,
                    &config.global_vocab_books,
                    &self.props.vocab_books,
                );
                fonos_core::vocab::build_glossary_block(&fonos_core::vocab::collect_terms(&books))
            };
            (service, glossary)
        };

        // `ctx.translate_target` is a plain field (no lock), so borrowing it
        // across the await is fine.
        let out = run_llm_step(
            &self.props,
            &text,
            &service,
            &ctx.translate_target,
            glossary.as_deref(),
        )
        .await?;
        Ok(Data::Text(out))
    }
}

// ─── Outputs (terminal) ───────────────────────────────────────────────────────

/// Borrow the text payload of a terminal [`Data`], erroring on audio. Every
/// output here accepts [`DataKind::Text`], so the engine's kind check makes the
/// audio arm unreachable in practice — it stays as a defensive guard.
fn expect_text(data: &Data) -> Result<&str, String> {
    match data {
        Data::Text(t) => Ok(t.as_str()),
        Data::Audio(_) => Err("workflow output expected text, got audio".to_string()),
    }
}

/// Read a string field out of `ctx.meta`, dropping the guard within the
/// expression (meta is a `std::sync::Mutex`, never held across an await).
fn read_meta_string(ctx: &RunCtx, key: &str) -> Option<String> {
    ctx.meta
        .lock()
        .ok()?
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read an integer field out of `ctx.meta` (see [`read_meta_string`]).
fn read_meta_i64(ctx: &RunCtx, key: &str) -> Option<i64> {
    ctx.meta.lock().ok()?.get(key).and_then(|v| v.as_i64())
}

/// `insert`: inject the text at the cursor of the frontmost app.
///
/// Delivery reuses [`crate::injection::inject_text`], which resolves the
/// strategy from the live `AppConfig` (global default + per-app overrides). The
/// widget's `strategy` prop is schema-only in P1 — exactly as the legacy
/// dictation `InjectionTextSink` behaved — and is not yet consumed here.
pub struct InsertOutput {
    /// Handle used to read the live `AppConfig` for strategy resolution.
    pub app: tauri::AppHandle,
    /// Press Return after inserting (send-on-insert).
    pub press_enter: bool,
}

#[async_trait::async_trait]
impl Output for InsertOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        let text = expect_text(result)?;

        // Clone the live config out of the lock (its per-app overrides drive the
        // strategy) so no std Mutex is held across the blocking inject.
        let config = {
            let state = self.app.state::<AppState>();
            let cfg = state.config.lock().map_err(|e| e.to_string())?;
            cfg.clone()
        };
        crate::injection::inject_text(text, &config)?;

        // Optional Return, after the same settle delay the core pipeline uses.
        // A press_enter failure is non-fatal — the text already landed.
        if self.press_enter {
            tokio::time::sleep(std::time::Duration::from_millis(
                fonos_core::pipeline::PRESS_ENTER_DELAY_MS,
            ))
            .await;
            if let Err(e) = crate::injection::press_enter() {
                eprintln!("fonos: workflow insert — press_enter failed (non-fatal): {e}");
            }
        }
        Ok(())
    }
}

/// `replace`: paste over the current selection in the source app.
pub struct ReplaceOutput;

#[async_trait::async_trait]
impl Output for ReplaceOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, ctx: &RunCtx) -> Result<(), String> {
        let text = expect_text(result)?.to_string();
        // The source app the selection came from (SelectionSource wrote it);
        // meta guard is read + dropped before the await.
        let app_name = read_meta_string(ctx, "app_name");
        super::selection::replace_selection(text, app_name).await
    }
}

/// `clipboard`: copy the text to the system clipboard.
pub struct ClipboardOutput;

#[async_trait::async_trait]
impl Output for ClipboardOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        let text = expect_text(result)?;
        let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard error: {e}"))?;
        cb.set_text(text)
            .map_err(|e| format!("clipboard set error: {e}"))
    }
}

/// `notebook`: link the already-recorded history entry to a notebook container.
///
/// The entry is written by the engine's recorder before delivery; this output
/// only relinks its container (no text copy).
pub struct NotebookOutput {
    /// Handle used to reach the history DB.
    pub app: tauri::AppHandle,
    /// Target container id; `0` means "resolve the Quick Note notebook at run
    /// time" (same lookup as `set_default_note_target`).
    pub container_id: i64,
}

#[async_trait::async_trait]
impl Output for NotebookOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, _result: &Data, ctx: &RunCtx) -> Result<(), String> {
        // No entry_id ⇒ the recorder did not run ⇒ nothing to link.
        let entry_id = read_meta_i64(ctx, "entry_id")
            .ok_or("notebook output: no entry_id in run context (recorder did not run)")?;

        let state = self.app.state::<AppState>();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        // container_id 0 ⇒ Quick Note by title (None if it doesn't exist yet,
        // which stores the entry uncontained, matching set_default_note_target).
        let container_id = if self.container_id == 0 {
            db.query_row(
                "SELECT id FROM containers WHERE container_type='notebook' AND title='Quick Note' LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .ok()
        } else {
            Some(self.container_id)
        };
        fonos_core::storage::update_entry_container(&db, entry_id, container_id)
            .map_err(|e| e.to_string())
    }
}

/// `speak`: synthesize the text to a WAV, persist it under the app data dir,
/// and link it to the recorded entry — the TTS half of `listen.rs::do_create`.
pub struct SpeakOutput {
    /// Handle used to reach `AppState` for TTS service resolution + the DB.
    pub app: tauri::AppHandle,
    /// TTS profile id, or empty to fall back to the global `"tts"` profile.
    pub voice_profile: String,
    /// Voice identifier passed to the backend (cloned voices resolve to a
    /// reference-audio path via `resolve_voice`).
    pub voice: String,
}

#[async_trait::async_trait]
impl Output for SpeakOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, ctx: &RunCtx) -> Result<(), String> {
        let text = expect_text(result)?;

        // Resolve the TTS service and build the engine up front, dropping the
        // State before the synthesis await (no State/lock held across await).
        let engine = {
            let state = self.app.state::<AppState>();
            let tts_svc = if self.voice_profile.is_empty() {
                super::get_service_config(&state, "tts")
            } else {
                super::get_service_config_for_profile(&state, &self.voice_profile)
            };
            if tts_svc.base_url.trim().is_empty() {
                return Err(
                    "No TTS profile configured — pick one in Settings > Speech.".to_string(),
                );
            }
            fonos_core::tts::HttpTts {
                service: tts_svc,
                voice: super::tts::resolve_voice(&self.voice),
                speed: 1.0,
            }
        };

        // Chunk + synthesize + concat via the shared core helper.
        let wav = fonos_core::listen::synthesize_long_text(text, &engine).await?;

        // Persist under the app data dir (same placement as do_create).
        let dir = fonos_core::config::AppConfig::config_dir().join("listen");
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not create listen dir: {e}"))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = dir.join(format!("listen_{stamp}.wav"));
        std::fs::write(&path, &wav).map_err(|e| format!("could not write audio: {e}"))?;
        let path_str = path.to_string_lossy().to_string();

        // Publish the audio ref (for the UI / any later output) and read the
        // entry id in one short meta critical section, dropped before the DB
        // write.
        let entry_id = {
            let mut meta = ctx.meta.lock().map_err(|e| e.to_string())?;
            meta.insert(
                "audio_ref".to_string(),
                serde_json::Value::String(path_str.clone()),
            );
            meta.get("entry_id").and_then(|v| v.as_i64())
        };

        // Link the WAV to the recorded entry, if the recorder ran.
        if let Some(id) = entry_id {
            let state = self.app.state::<AppState>();
            let db = state.db.lock().map_err(|e| e.to_string())?;
            fonos_core::storage::update_entry_audio_ref(&db, id, &path_str)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// `panel`: show the result in the shared text-action popup, reusing that
/// module's window + recv protocol ([`super::text_action::show_panel_at_cursor`]
/// / [`super::text_action::panel_js`]).
pub struct PanelOutput {
    /// Handle used to position/reveal the panel and run its JS.
    pub app: tauri::AppHandle,
    /// Render-as-markdown hint forwarded to the panel (P2 consumes it).
    pub markdown: bool,
}

#[async_trait::async_trait]
impl Output for PanelOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, ctx: &RunCtx) -> Result<(), String> {
        let text = expect_text(result)?;

        // Snapshot the meta the panel's footer buttons need (source app for
        // Insert, entry id for Save, editability to enable Insert) before any
        // await — meta is a std Mutex.
        let (app_name, editable, entry_id) = {
            let meta = ctx.meta.lock().map_err(|e| e.to_string())?;
            (
                meta.get("app_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                meta.get("editable").and_then(|v| v.as_bool()).unwrap_or(false),
                meta.get("entry_id").and_then(|v| v.as_i64()).unwrap_or(0),
            )
        };

        super::text_action::show_panel_at_cursor(&self.app).await;

        // recvStart resets the panel and carries `markdown` as its 4th arg (the
        // panel JS ignores extra positional args for now). The output runs
        // post-processing, so there is no source/mode context to show; P1 uses a
        // neutral header and lets recvResult carry the text + footer context.
        let icon_j = serde_json::to_string("🪟").unwrap_or_default();
        let name_j = serde_json::to_string("").unwrap_or_default();
        let sel_j = serde_json::to_string("").unwrap_or_default();
        super::text_action::panel_js(
            &self.app,
            &format!("recvStart({icon_j}, {name_j}, {sel_j}, {})", self.markdown),
        );

        let text_j = serde_json::to_string(text).unwrap_or_default();
        let app_j = serde_json::to_string(&app_name).unwrap_or_default();
        super::text_action::panel_js(
            &self.app,
            &format!("recvResult({text_j}, {entry_id}, {app_j}, {editable})"),
        );
        Ok(())
    }
}

// ─── Registry assembly ────────────────────────────────────────────────────────

/// Build the workflow [`Registry`] with every desktop factory registered by
/// `type_tag`: the sources (`selection`, `microphone`), processors (`stt`,
/// `llm`), and the six terminal outputs (`insert`, `replace`, `clipboard`,
/// `notebook`, `speak`, `panel`).
///
/// Each factory closure captures `app.clone()` and re-clones per instantiation
/// so a widget can be built many times. Task 16's `uppercase` demo processor
/// registers itself in its own task and is intentionally not added here.
pub fn build_registry(app: tauri::AppHandle) -> Registry {
    let mut reg = Registry::default();

    // ── Sources ──────────────────────────────────────────────────────────────
    reg.register_source(
        "selection",
        Box::new(|_props| Ok(Arc::new(SelectionSource) as Arc<dyn Source>)),
    );
    {
        let app = app.clone();
        reg.register_source(
            "microphone",
            Box::new(move |props| {
                let capture = props
                    .get("capture")
                    .and_then(|v| v.as_str())
                    .unwrap_or("hold")
                    .to_string();
                Ok(Arc::new(MicSource {
                    capture,
                    app: app.clone(),
                }) as Arc<dyn Source>)
            }),
        );
    }

    // ── Processors ───────────────────────────────────────────────────────────
    {
        let app = app.clone();
        reg.register_processor(
            "stt",
            Box::new(move |props| {
                let props: SttProps = serde_json::from_value(props.clone())
                    .map_err(|e| format!("stt props: {e}"))?;
                Ok(Arc::new(SttProcessor {
                    app: app.clone(),
                    props,
                }) as Arc<dyn Processor>)
            }),
        );
    }
    {
        let app = app.clone();
        reg.register_processor(
            "llm",
            Box::new(move |props| {
                let props: LlmProps = serde_json::from_value(props.clone())
                    .map_err(|e| format!("llm props: {e}"))?;
                Ok(Arc::new(LlmProcessor {
                    app: app.clone(),
                    props,
                }) as Arc<dyn Processor>)
            }),
        );
    }

    // ── Outputs ──────────────────────────────────────────────────────────────
    {
        let app = app.clone();
        reg.register_output(
            "insert",
            Box::new(move |props| {
                let press_enter = props
                    .get("press_enter")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(Arc::new(InsertOutput {
                    app: app.clone(),
                    press_enter,
                }) as Arc<dyn Output>)
            }),
        );
    }
    reg.register_output(
        "replace",
        Box::new(|_props| Ok(Arc::new(ReplaceOutput) as Arc<dyn Output>)),
    );
    reg.register_output(
        "clipboard",
        Box::new(|_props| Ok(Arc::new(ClipboardOutput) as Arc<dyn Output>)),
    );
    {
        let app = app.clone();
        reg.register_output(
            "notebook",
            Box::new(move |props| {
                let container_id = props
                    .get("container_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                Ok(Arc::new(NotebookOutput {
                    app: app.clone(),
                    container_id,
                }) as Arc<dyn Output>)
            }),
        );
    }
    {
        let app = app.clone();
        reg.register_output(
            "speak",
            Box::new(move |props| {
                let voice_profile = props
                    .get("voice_profile")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let voice = props
                    .get("voice")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                Ok(Arc::new(SpeakOutput {
                    app: app.clone(),
                    voice_profile,
                    voice,
                }) as Arc<dyn Output>)
            }),
        );
    }
    {
        let app = app.clone();
        reg.register_output(
            "panel",
            Box::new(move |props| {
                let markdown = props
                    .get("markdown")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(Arc::new(PanelOutput {
                    app: app.clone(),
                    markdown,
                }) as Arc<dyn Output>)
            }),
        );
    }

    reg
}

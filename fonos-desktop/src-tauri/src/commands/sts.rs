//! STS conversation commands (issue #24): hold-to-talk → transcript →
//! core::sts::run_turn (vocab + chat stages) → spoken reply. The pipeline
//! lives in fonos-core; this module resolves config and provides adapters.

use super::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// One conversation turn at a time; hotkey autorepeat and double-taps are
/// dropped while a turn is in flight.
static TURN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Whether a conversation turn is currently running (hotkey key-down checks
/// this so it never starts a second recording mid-turn — the abandoned
/// recording used to corrupt the pill's state display).
pub fn turn_in_flight() -> bool {
    TURN_IN_FLIGHT.load(Ordering::SeqCst)
}

/// Hotkey key-up entry point: stop recording, transcribe, run one turn with
/// pill progress.
pub async fn run_sts_turn(app: tauri::AppHandle) -> Result<String, String> {
    finish_turn(app, None, false).await
}

/// Conversation-page entry: start recording (no float pill).
#[tauri::command]
pub async fn sts_page_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    super::dictation::start_recording(app, state, Some(true)).await
}

/// Conversation-page entry: stop, transcribe, run the turn. `persona`
/// overrides the configured system prompt for this and future turns from the
/// page (page-local; config is untouched).
#[tauri::command]
pub async fn sts_page_stop(
    app: tauri::AppHandle,
    persona: Option<String>,
) -> Result<String, String> {
    finish_turn(app, persona, true).await
}

/// The session transcript so the page can render history on mount.
#[tauri::command]
pub async fn get_sts_history(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, String)>, String> {
    Ok(state.sts_session.lock().await.history.clone())
}

async fn finish_turn(
    app: tauri::AppHandle,
    persona_override: Option<String>,
    from_page: bool,
) -> Result<String, String> {
    if TURN_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return Err("conversation turn already running".to_string());
    }
    let result = run_sts_turn_inner(app, persona_override, from_page).await;
    TURN_IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

async fn run_sts_turn_inner(
    app: tauri::AppHandle,
    persona_override: Option<String>,
    from_page: bool,
) -> Result<String, String> {
    // The bridge exists before anything can fail so every error — including
    // recording/STT failures — reaches the page as an sts:event.
    let sink = crate::adapters::TurnEventBridge::new(app.clone(), !from_page);

    // agent/sts-page overrides: no injection, no float lifecycle from
    // stop_recording — the TurnSink owns rendering from here. "sts-page"
    // additionally suppresses the pill entirely (in-app turns).
    let mode = if from_page { "sts-page" } else { "agent" };
    let state: tauri::State<'_, AppState> = app.state();
    let result =
        match super::dictation::stop_recording(app.clone(), state, Some(mode.to_string())).await {
            Ok(r) => r,
            Err(e) => {
                use fonos_core::sts::TurnSink;
                sink.emit(fonos_core::sts::TurnEvent::Failed(
                    fonos_core::error_class::classify_error(&e),
                ));
                return Err(e);
            }
        };

    let state: tauri::State<'_, AppState> = app.state();

    let (persona, llm_profile, voice_profile, voice, max_turns, global_books, books) = {
        let cfg = state.config.lock().map_err(|e| e.to_string())?;
        (
            cfg.sts_persona.clone(),
            cfg.sts_llm_profile.clone(),
            cfg.sts_voice_profile.clone(),
            cfg.sts_voice.clone(),
            cfg.sts_max_turns,
            cfg.global_vocab_books.clone(),
            cfg.vocab_books.clone(),
        )
    };

    let llm = if !llm_profile.is_empty() {
        super::get_service_config_for_profile(&state, &llm_profile)
    } else {
        super::get_service_config(&state, "llm")
    };
    let tts_svc = if !voice_profile.is_empty() {
        super::get_service_config_for_profile(&state, &voice_profile)
    } else {
        super::get_service_config(&state, "tts")
    };
    if llm.base_url.trim().is_empty() {
        let e = "No LLM profile configured — pick one in Settings > Models.".to_string();
        use fonos_core::sts::TurnSink;
        sink.emit(fonos_core::sts::TurnEvent::Failed(fonos_core::error_class::classify_error(&e)));
        return Err(e);
    }
    if tts_svc.base_url.trim().is_empty() {
        let e = "No TTS profile configured — pick one in Settings > Speech.".to_string();
        use fonos_core::sts::TurnSink;
        sink.emit(fonos_core::sts::TurnEvent::Failed(fonos_core::error_class::classify_error(&e)));
        return Err(e);
    }

    // Stage chain: vocabulary corrections, then the conversation LLM.
    let vocab_books = fonos_core::vocab::effective_books(&books, &global_books, &[])
        .into_iter()
        .cloned()
        .collect();
    let stages: Vec<Box<dyn fonos_core::sts::TextStage>> = vec![
        Box::new(fonos_core::sts::VocabStage { books: vocab_books }),
        Box::new(fonos_core::sts::ChatStage {
            service: llm,
            system: persona_override.filter(|p| !p.trim().is_empty()).unwrap_or(persona),
            use_history: true,
            temperature: 0.4,
            max_tokens: 512,
        }),
    ];

    let tts = fonos_core::tts::HttpTts {
        service: tts_svc,
        voice: super::tts::resolve_voice(&voice),
        speed: 1.0,
    };
    let audio = crate::adapters::PlaybackAudioOut::new(state.audio_playback.clone());

    let mut session = state.sts_session.lock().await;
    session.max_turns = max_turns;
    fonos_core::sts::run_turn(&mut session, result.text, &stages, &tts, &audio, &sink).await
}

/// Clear the conversation memory.
#[tauri::command]
pub async fn reset_sts_session(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut session = state.sts_session.lock().await;
    session.history.clear();
    Ok(())
}

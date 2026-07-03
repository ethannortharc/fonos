//! STS conversation commands (issue #24): hold-to-talk → transcript →
//! core::sts::run_turn (vocab + chat stages) → spoken reply. The pipeline
//! lives in fonos-core; this module resolves config and provides adapters.

use super::AppState;
use tauri::Manager;

/// Key-up entry point: stop recording, transcribe, run one conversation turn.
pub async fn run_sts_turn(app: tauri::AppHandle) -> Result<String, String> {
    // Agent mode override: no injection, no float:stop — the TurnSink owns
    // the pill lifecycle from here (stop_recording already emitted
    // float:processing).
    let state: tauri::State<'_, AppState> = app.state();
    let result =
        super::dictation::stop_recording(app.clone(), state, Some("agent".to_string())).await?;

    let state: tauri::State<'_, AppState> = app.state();
    let sink = crate::adapters::PillTurnSink::new(app.clone());

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
            system: persona,
            use_history: true,
            temperature: 0.4,
            max_tokens: 512,
        }),
    ];

    let tts = fonos_core::tts::HttpTts { service: tts_svc, voice, speed: 1.0 };
    let audio = crate::adapters::PlaybackAudioOut(state.audio_playback.clone());

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

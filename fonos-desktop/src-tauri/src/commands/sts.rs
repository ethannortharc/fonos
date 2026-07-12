//! STS conversation-turn executor (issue #24): transcript →
//! core::sts::run_turn (vocab + chat stages) → spoken reply. The pipeline
//! lives in fonos-core; this module provides the desktop turn executor the
//! hands-free call loop drives.
//!
//! Workbench P2 Task 9 retired the walkie mode (the ⌥S hold-to-talk hotkey
//! arm, the Talk page's `sts_page_start`/`sts_page_stop` commands, and the
//! page-persona override) along with the `get_sts_history`/
//! `reset_sts_session` commands (the call-panel satellite consumes only
//! `sts:event`). What remains — [`execute_turn`]/[`execute_turn_with_audio`]
//! — no longer reads `cfg.sts_*` at all: every formerly-config-sourced value
//! (persona, LLM/TTS services, voice, max_turns) arrives pre-resolved in a
//! [`super::call_widget::ResolvedCallCfg`], whose single constructor is
//! `CallOutput::deliver`. Only the vocab books stay live-read per turn
//! (unchanged behavior: mid-call vocab edits keep applying).
//!
//! **AUDIO RED LINE:** the stage chain, its tuning (temperature 0.4,
//! max_tokens 512), the empty-service error surfacing, and the
//! `run_turn` call are byte-preserved from the pre-Task-9 code — only the
//! sourcing of the values changed.

use tauri::Manager;

use super::call_widget::ResolvedCallCfg;
use super::AppState;

/// Run one conversation turn for an already-transcribed utterance, playing
/// the reply through the default speaker path
/// ([`crate::adapters::PlaybackAudioOut`] on the shared rodio playback).
/// Callers that need the reply routed elsewhere (call mode's
/// voice-processing helper) use [`execute_turn_with_audio`].
///
/// Config/profile errors are surfaced through `sink` before returning.
pub(crate) async fn execute_turn(
    app: &tauri::AppHandle,
    transcript: String,
    call_cfg: &ResolvedCallCfg,
    sink: &dyn fonos_core::sts::TurnSink,
) -> Result<String, String> {
    let state: tauri::State<'_, AppState> = app.state();
    let audio = crate::adapters::PlaybackAudioOut::new(state.audio_playback.clone());
    execute_turn_with_audio(app, transcript, call_cfg, sink, &audio).await
}

/// [`execute_turn`] with a caller-supplied speaker port. Call mode on macOS
/// passes the voice-processing helper's [`AudioOut`] so the TTS plays through
/// the helper's engine — giving Apple's echo canceller its true reference.
///
/// [`AudioOut`]: fonos_core::sts::AudioOut
pub(crate) async fn execute_turn_with_audio(
    app: &tauri::AppHandle,
    transcript: String,
    call_cfg: &ResolvedCallCfg,
    sink: &dyn fonos_core::sts::TurnSink,
    audio: &dyn fonos_core::sts::AudioOut,
) -> Result<String, String> {
    let state: tauri::State<'_, AppState> = app.state();

    // Vocab books stay live-read from config, per turn (unchanged behavior —
    // everything else comes pre-resolved in `call_cfg`).
    let (global_books, books) = {
        let cfg = state.config.lock().map_err(|e| e.to_string())?;
        (cfg.global_vocab_books.clone(), cfg.vocab_books.clone())
    };

    if call_cfg.llm.base_url.trim().is_empty() {
        let e = "No LLM profile configured — pick one in Settings > Models.".to_string();
        sink.emit(fonos_core::sts::TurnEvent::Failed(fonos_core::error_class::classify_error(&e)));
        return Err(e);
    }
    eprintln!(
        "fonos: call TTS base_url={} voice={:?}",
        if call_cfg.tts.base_url.trim().is_empty() { "EMPTY" } else { "set" },
        call_cfg.voice
    );
    if call_cfg.tts.base_url.trim().is_empty() {
        let e = "No TTS profile configured — pick one in Settings > Speech.".to_string();
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
            service: call_cfg.llm.clone(),
            system: call_cfg.persona_system.clone(),
            use_history: true,
            temperature: 0.4,
            max_tokens: 512,
        }),
    ];

    let tts = fonos_core::tts::HttpTts {
        service: call_cfg.tts.clone(),
        voice: super::tts::resolve_voice(&call_cfg.voice),
        speed: 1.0,
    };

    let mut session = state.sts_session.lock().await;
    session.max_turns = call_cfg.max_turns;
    fonos_core::sts::run_turn(&mut session, transcript, &stages, &tts, audio, sink).await
}

//! STS (speech-to-speech) pipeline (issue #24): a composable conversation
//! turn — transcript → [`TextStage`] chain → sentence-chunked synthesis →
//! playback — with typed [`TurnEvent`]s for any renderer (pill, sprite,
//! robot).
//!
//! Everything here is platform-independent. Shells own recording (they hand
//! in the transcript), audio output ([`AudioOut`]), and event rendering
//! ([`TurnSink`]). Scenario differences are composition differences: a chat
//! companion is `[VocabStage, ChatStage(history)]`; a simultaneous
//! interpreter is `[VocabStage, ChatStage(translate persona, no history)]` —
//! no new code, different stage configs.

use crate::error_class::{classify_error, SurfacedError};
use crate::listen::split_for_tts;
use crate::tts::TtsEngine;

/// Typed per-turn notifications for renderers.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    /// The user's words, as transcribed.
    Transcript(String),
    /// The pipeline's final reply text (before/while it is spoken).
    Reply(String),
    /// Speech playback started.
    SpeakingStarted,
    /// Speech playback finished.
    SpeakingDone,
    /// The turn completed successfully.
    TurnDone,
    /// The turn failed; classified for display.
    Failed(SurfacedError),
}

/// Renderer port for turn events.
pub trait TurnSink: Send + Sync {
    /// Deliver one event to the renderer.
    fn emit(&self, event: TurnEvent);
}

/// Speaker port: play one complete WAV and return when playback is done.
#[async_trait::async_trait]
pub trait AudioOut: Send + Sync {
    /// Play `wav` to completion.
    async fn play_wav(&self, wav: Vec<u8>) -> Result<(), String>;
}

/// One composable text-processing step in the pipeline.
#[async_trait::async_trait]
pub trait TextStage: Send + Sync {
    /// Transform `text`, optionally consulting the conversation history
    /// (pairs of user/assistant turns, oldest first).
    async fn process(
        &self,
        text: String,
        history: &[(String, String)],
    ) -> Result<String, String>;
}

/// Deterministic vocabulary corrections (see [`crate::vocab`]).
pub struct VocabStage {
    /// Effective books applied to the transcript.
    pub books: Vec<crate::vocab::VocabBook>,
}

#[async_trait::async_trait]
impl TextStage for VocabStage {
    async fn process(&self, text: String, _history: &[(String, String)]) -> Result<String, String> {
        let refs: Vec<&crate::vocab::VocabBook> = self.books.iter().collect();
        Ok(crate::vocab::apply_rules(&text, &refs))
    }
}

/// LLM chat/transform stage. With `use_history` this is a conversation
/// partner; without it, a stateless transformer (e.g. a translator persona).
pub struct ChatStage {
    /// Resolved LLM connection.
    pub service: crate::llm::ServiceConfig,
    /// System prompt / persona.
    pub system: String,
    /// Include prior turns in the request.
    pub use_history: bool,
    /// Sampling temperature.
    pub temperature: f64,
    /// Response cap — spoken replies should be short.
    pub max_tokens: u32,
}

#[async_trait::async_trait]
impl TextStage for ChatStage {
    async fn process(&self, text: String, history: &[(String, String)]) -> Result<String, String> {
        let mut messages = vec![serde_json::json!({"role": "system", "content": self.system})];
        if self.use_history {
            for (user, assistant) in history {
                messages.push(serde_json::json!({"role": "user", "content": user}));
                messages.push(serde_json::json!({"role": "assistant", "content": assistant}));
            }
        }
        messages.push(serde_json::json!({"role": "user", "content": text}));

        crate::llm::call_openai_compatible(
            &self.service.api_key,
            &self.service.model,
            &self.service.base_url,
            &messages,
            self.temperature,
            self.max_tokens,
            &self.service.provider,
        )
        .await
        .map(|r| r.text)
        .map_err(|e| e.to_string())
    }
}

/// Conversation memory carried across turns.
#[derive(Debug, Default)]
pub struct StsSession {
    /// (user, assistant) pairs, oldest first.
    pub history: Vec<(String, String)>,
    /// Max pairs to retain (older ones are dropped).
    pub max_turns: usize,
}

/// Max characters per synthesized sentence chunk in conversation replies.
pub const STS_CHUNK_CHARS: usize = 300;

/// Run one conversation turn.
///
/// Emits exactly one terminal event (`TurnDone` or `Failed`). Synthesis and
/// playback are pipelined: while sentence *n* plays, sentence *n+1* is
/// already synthesizing.
pub async fn run_turn(
    session: &mut StsSession,
    transcript: String,
    stages: &[Box<dyn TextStage>],
    tts: &dyn TtsEngine,
    audio: &dyn AudioOut,
    sink: &dyn TurnSink,
) -> Result<String, String> {
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        let err = "No speech detected".to_string();
        sink.emit(TurnEvent::Failed(classify_error(&err)));
        return Err(err);
    }
    sink.emit(TurnEvent::Transcript(transcript.clone()));

    // Fold the text through the stage chain.
    let mut text = transcript.clone();
    for stage in stages {
        match stage.process(text, &session.history).await {
            Ok(t) => text = t,
            Err(e) => {
                sink.emit(TurnEvent::Failed(classify_error(&e)));
                return Err(e);
            }
        }
    }
    let reply = text.trim().to_string();
    if reply.is_empty() {
        let err = "The pipeline produced no reply".to_string();
        sink.emit(TurnEvent::Failed(classify_error(&err)));
        return Err(err);
    }
    sink.emit(TurnEvent::Reply(reply.clone()));

    // Pipelined speak: synthesize chunk n+1 while chunk n plays.
    let chunks = split_for_tts(&reply, STS_CHUNK_CHARS);
    sink.emit(TurnEvent::SpeakingStarted);
    let mut pending = match tts.synthesize(&chunks[0]).await {
        Ok(w) => w,
        Err(e) => {
            let e = format!("TTS failed: {e}");
            sink.emit(TurnEvent::Failed(classify_error(&e)));
            return Err(e);
        }
    };
    for next_chunk in chunks.iter().skip(1) {
        let (played, synthesized) =
            tokio::join!(audio.play_wav(std::mem::take(&mut pending)), tts.synthesize(next_chunk));
        if let Err(e) = played {
            let e = format!("Playback failed: {e}");
            sink.emit(TurnEvent::Failed(classify_error(&e)));
            return Err(e);
        }
        match synthesized {
            Ok(w) => pending = w,
            Err(e) => {
                let e = format!("TTS failed: {e}");
                sink.emit(TurnEvent::Failed(classify_error(&e)));
                return Err(e);
            }
        }
    }
    if let Err(e) = audio.play_wav(pending).await {
        let e = format!("Playback failed: {e}");
        sink.emit(TurnEvent::Failed(classify_error(&e)));
        return Err(e);
    }
    sink.emit(TurnEvent::SpeakingDone);

    // Remember the exchange.
    session.history.push((transcript, reply.clone()));
    if session.max_turns > 0 {
        while session.history.len() > session.max_turns {
            session.history.remove(0);
        }
    }

    sink.emit(TurnEvent::TurnDone);
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeSink(Mutex<Vec<TurnEvent>>);
    impl TurnSink for FakeSink {
        fn emit(&self, e: TurnEvent) {
            self.0.lock().unwrap().push(e);
        }
    }

    struct UpperStage;
    #[async_trait::async_trait]
    impl TextStage for UpperStage {
        async fn process(&self, t: String, _h: &[(String, String)]) -> Result<String, String> {
            Ok(t.to_uppercase())
        }
    }
    struct SuffixStage(&'static str);
    #[async_trait::async_trait]
    impl TextStage for SuffixStage {
        async fn process(&self, t: String, _h: &[(String, String)]) -> Result<String, String> {
            Ok(format!("{t}{}", self.0))
        }
    }
    struct FailStage;
    #[async_trait::async_trait]
    impl TextStage for FailStage {
        async fn process(&self, _t: String, _h: &[(String, String)]) -> Result<String, String> {
            Err("LLM API error 401: nope".into())
        }
    }

    struct FakeTts(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl TtsEngine for FakeTts {
        async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
            self.0.lock().unwrap().push(text.to_string());
            Ok(text.as_bytes().to_vec())
        }
    }
    struct FakeAudio(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl AudioOut for FakeAudio {
        async fn play_wav(&self, wav: Vec<u8>) -> Result<(), String> {
            self.0.lock().unwrap().push(String::from_utf8_lossy(&wav).into_owned());
            Ok(())
        }
    }

    fn ports() -> (FakeSink, FakeTts, FakeAudio) {
        (FakeSink(Mutex::new(vec![])), FakeTts(Mutex::new(vec![])), FakeAudio(Mutex::new(vec![])))
    }

    #[tokio::test]
    async fn stages_chain_in_order_and_history_grows() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession { history: vec![], max_turns: 2 };
        let stages: Vec<Box<dyn TextStage>> =
            vec![Box::new(UpperStage), Box::new(SuffixStage("!"))];
        let reply = run_turn(&mut session, "hello".into(), &stages, &tts, &audio, &sink)
            .await
            .unwrap();
        assert_eq!(reply, "HELLO!");
        assert_eq!(session.history, vec![("hello".to_string(), "HELLO!".to_string())]);
        let events = sink.0.lock().unwrap();
        assert_eq!(
            *events,
            vec![
                TurnEvent::Transcript("hello".into()),
                TurnEvent::Reply("HELLO!".into()),
                TurnEvent::SpeakingStarted,
                TurnEvent::SpeakingDone,
                TurnEvent::TurnDone,
            ]
        );
    }

    #[tokio::test]
    async fn history_is_trimmed_to_max_turns() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession { history: vec![], max_turns: 2 };
        let stages: Vec<Box<dyn TextStage>> = vec![Box::new(SuffixStage("."))];
        for word in ["a", "b", "c"] {
            run_turn(&mut session, word.into(), &stages, &tts, &audio, &sink).await.unwrap();
        }
        assert_eq!(session.history.len(), 2);
        assert_eq!(session.history[0].0, "b");
    }

    #[tokio::test]
    async fn long_replies_are_chunked_and_played_in_order() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession::default();
        let long = "First sentence. ".repeat(40); // >> STS_CHUNK_CHARS
        let stages: Vec<Box<dyn TextStage>> = vec![Box::new(SuffixStage(&""))];
        let _ = run_turn(&mut session, long.clone(), &stages, &tts, &audio, &sink).await.unwrap();
        let played = audio.0.lock().unwrap();
        assert!(played.len() >= 2, "expected chunked playback, got {}", played.len());
        assert_eq!(played.join(" ").split_whitespace().count(), long.trim().split_whitespace().count());
    }

    #[tokio::test]
    async fn stage_failure_emits_classified_failed_only() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession::default();
        let stages: Vec<Box<dyn TextStage>> = vec![Box::new(FailStage)];
        let r = run_turn(&mut session, "hi".into(), &stages, &tts, &audio, &sink).await;
        assert!(r.is_err());
        assert!(session.history.is_empty(), "failed turns must not enter history");
        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 2); // Transcript + Failed
        match &events[1] {
            TurnEvent::Failed(s) => assert!(s.message.contains("API key")),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(audio.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_transcript_fails_fast() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession::default();
        let stages: Vec<Box<dyn TextStage>> = vec![];
        assert!(run_turn(&mut session, "  ".into(), &stages, &tts, &audio, &sink).await.is_err());
        assert!(tts.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn vocab_stage_applies_rules() {
        let book = crate::vocab::VocabBook {
            id: "b".into(),
            name: "b".into(),
            enabled: true,
            terms: vec![],
            rules: vec![crate::vocab::VocabRule {
                from: "一休".into(),
                to: "issue".into(),
                ..Default::default()
            }],
        };
        let stage = VocabStage { books: vec![book] };
        let out = stage.process("看看这个一休".into(), &[]).await.unwrap();
        assert_eq!(out, "看看这个issue");
    }
}

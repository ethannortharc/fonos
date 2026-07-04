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
use crate::tts::{PcmSink, TtsEngine};

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

/// Speaker port: receives streamed PCM (via the [`PcmSink`] supertrait) and
/// can be awaited until everything queued has actually been played.
#[async_trait::async_trait]
pub trait AudioOut: PcmSink {
    /// Block until all pushed audio has finished playing.
    async fn finish(&self) -> Result<(), String>;
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

    // Speak a sanitized copy: emoji/markdown are stripped so the TTS never
    // reads symbols aloud; the Reply event above keeps the original text.
    let speech_text = crate::listen::sanitize_for_speech(&reply);
    if speech_text.is_empty() {
        // Nothing speakable (reply was all symbols) — still a completed turn.
        session.history.push((transcript, reply.clone()));
        if session.max_turns > 0 {
            while session.history.len() > session.max_turns {
                session.history.remove(0);
            }
        }
        sink.emit(TurnEvent::TurnDone);
        return Ok(reply);
    }

    // Clause-paced speak. Measured on OMLX Qwen3-TTS: generation is ~2.3x
    // slower than real-time, so feeding raw stream chunks straight to the
    // speaker underruns constantly (word-by-word stutter). Instead each
    // clause is synthesized whole and appended to the playback queue: gaps
    // can only fall BETWEEN clauses (natural pauses), the first clause is
    // small for fast time-to-first-sound, and with a faster-than-real-time
    // engine the queue never drains — seamless by construction.
    sink.emit(TurnEvent::SpeakingStarted);
    let mut began = false;
    for chunk in crate::listen::split_for_speech(&speech_text) {
        let wav = match tts.synthesize(&chunk).await {
            Ok(w) => w,
            Err(e) => {
                let e = format!("TTS failed: {e}");
                sink.emit(TurnEvent::Failed(classify_error(&e)));
                return Err(e);
            }
        };
        let step = (|| -> Result<(), String> {
            let parsed = crate::listen::parse_wav(&wav)?;
            if !began {
                let (rate, channels) = crate::tts::wav_fmt(parsed.fmt)?;
                audio.begin(rate, channels)?;
            }
            audio.push(parsed.data)
        })();
        began = true;
        if let Err(e) = step {
            let e = format!("Playback failed: {e}");
            sink.emit(TurnEvent::Failed(classify_error(&e)));
            return Err(e);
        }
    }
    if let Err(e) = audio.finish().await {
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

    /// Wrap `payload` (padded to frame size) in a minimal valid WAV.
    fn make_wav(payload: &str) -> Vec<u8> {
        let mut pcm = payload.as_bytes().to_vec();
        if pcm.len() % 2 == 1 {
            pcm.push(b' ');
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + pcm.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&16000u32.to_le_bytes());
        out.extend_from_slice(&32000u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        out.extend_from_slice(&pcm);
        out
    }

    struct FakeTts(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl TtsEngine for FakeTts {
        async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
            self.0.lock().unwrap().push(text.to_string());
            Ok(make_wav(text))
        }
    }
    /// Records the streamed lifecycle: begin(fmt) → pcm… → finish.
    struct FakeAudio(Mutex<Vec<String>>);
    impl PcmSink for FakeAudio {
        fn begin(&self, rate: u32, ch: u16) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("begin {rate}/{ch}"));
            Ok(())
        }
        fn push(&self, pcm: &[u8]) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .push(format!("pcm:{}", String::from_utf8_lossy(pcm).trim_end()));
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl AudioOut for FakeAudio {
        async fn finish(&self) -> Result<(), String> {
            self.0.lock().unwrap().push("finish".into());
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
    async fn audio_lifecycle_is_begin_pcm_finish() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession::default();
        let stages: Vec<Box<dyn TextStage>> = vec![Box::new(SuffixStage("."))];
        run_turn(&mut session, "hello".into(), &stages, &tts, &audio, &sink).await.unwrap();
        let calls = audio.0.lock().unwrap();
        assert_eq!(*calls, vec!["begin 16000/1", "pcm:hello.", "finish"]);
    }

    #[tokio::test]
    async fn long_replies_speak_clause_by_clause() {
        let (sink, tts, audio) = ports();
        let mut session = StsSession::default();
        let long = "第一小句，第二小句。然后是第二句话，比较长一些。最后一句结束。".repeat(5);
        let stages: Vec<Box<dyn TextStage>> = vec![Box::new(SuffixStage(""))];
        run_turn(&mut session, long.clone(), &stages, &tts, &audio, &sink).await.unwrap();
        let calls = audio.0.lock().unwrap();
        let begins = calls.iter().filter(|c| c.starts_with("begin")).count();
        let pcms: Vec<&String> = calls.iter().filter(|c| c.starts_with("pcm:")).collect();
        assert_eq!(begins, 1, "begin exactly once");
        assert!(pcms.len() >= 3, "multiple clause chunks, got {}", pcms.len());
        assert_eq!(calls.last().unwrap(), "finish");
        let spoken: String = pcms.iter().map(|c| c.trim_start_matches("pcm:")).collect();
        assert_eq!(spoken, long);
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

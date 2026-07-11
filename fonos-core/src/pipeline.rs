//! Dictation pipeline orchestration — the platform-independent flow between
//! "we have an outcome" and "the user saw the result".
//!
//! Ports ([`EventSink`], [`TextSink`]) are implemented by platform shells
//! (Tauri float pill + CGEvent injection on desktop; other surfaces on other
//! platforms). The flow itself lives here so behavior changes are made once,
//! under unit tests with fake adapters — previously this logic was duplicated
//! across the desktop app's three hotkey paths (hold / toggle / Linux).

use crate::error_class::{classify_error, SurfacedError};

/// Typed pipeline notification; adapters translate these to their UI surface
/// (the desktop pill maps them onto its `float:*` Tauri events).
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineEvent {
    /// A processing stage (LLM) is running; keep the busy indicator up.
    Processing,
    /// The pipeline finished and the final text was delivered (or handed to
    /// the clipboard for modes that don't auto-paste).
    ///
    /// * `raw` — the first text to appear in the pipeline (the STT transcript
    ///   for a mic run, the selection for a text run) — the same value the
    ///   recorder received. Empty (`""`) for emitters that only ever see the
    ///   final text.
    /// * `final_text` — the text delivered to every output and shown on the
    ///   pill.
    /// * `workflow` — the workflow id for engine runs, so a surface can label
    ///   the result by the run's workflow; `None` for emitters outside the
    ///   engine (the STS turn bridge, the listen command, the shared post-LLM
    ///   flow), which carry no workflow identity.
    Delivered {
        /// First text in the pipeline (raw transcript / selection); `""` when
        /// no distinct raw text is at hand.
        raw: String,
        /// Final text delivered to every output and shown on the pill.
        final_text: String,
        /// Workflow id for engine runs; `None` for non-workflow emitters.
        workflow: Option<String>,
    },
    /// The recording produced no usable speech.
    NoSpeech,
    /// The pipeline failed; the error is already classified for display.
    Failed(SurfacedError),
    /// A pipeline step began (test-run tracing; UI-agnostic).
    StepStarted {
        /// The workflow this step belongs to.
        workflow: String,
        /// The widget id of this step's component.
        step_id: String,
        /// Zero-based position in the run's step sequence (source, then
        /// processors in order, then outputs in order).
        index: usize,
        /// `"source"`, `"processor"`, or `"output"`.
        role: String,
    },
    /// A pipeline step finished. `preview` renders the step's output as
    /// text ("[audio]" for audio payloads, truncated to 4000 chars);
    /// `intercepted` marks an output skipped by dry-run.
    StepFinished {
        /// The workflow this step belongs to.
        workflow: String,
        /// The widget id of this step's component.
        step_id: String,
        /// Zero-based position in the run's step sequence.
        index: usize,
        /// `"source"`, `"processor"`, or `"output"`.
        role: String,
        /// A text rendering of the step's output.
        preview: String,
        /// Wall-clock duration of this step, in milliseconds.
        ms: u64,
        /// The step's error, if it failed.
        error: Option<String>,
        /// Whether this was an output skipped by dry-run (no delivery
        /// happened; `ms` is `0`).
        intercepted: bool,
    },
}

/// UI notification port. Implemented by platform shells.
///
/// `Send + Sync` because delivery runs inside spawned async tasks.
pub trait EventSink: Send + Sync {
    /// Deliver one pipeline event to the user-facing surface.
    fn emit(&self, event: PipelineEvent);
}

/// Text delivery port (typing/pasting at the cursor). Implemented by shells.
///
/// `Send + Sync` because delivery runs inside spawned async tasks.
pub trait TextSink: Send + Sync {
    /// Insert `text` at the user's cursor. Errors are raw strings that will be
    /// classified before display.
    fn inject(&self, text: &str) -> Result<(), String>;
    /// Simulate pressing Return after a successful injection.
    fn press_enter(&self) -> Result<(), String>;
}

/// Outcome of the LLM stage as consumed by [`deliver_llm_result`].
#[derive(Debug, Clone)]
pub struct LlmStageOutput {
    /// The processed text.
    pub processed: String,
    /// Whether the mode wants the result auto-inserted at the cursor.
    pub auto_paste: bool,
    /// Whether to press Return after a successful insertion.
    pub auto_press_enter: bool,
}

/// Final disposition of a dictation, used by callers to decide follow-up work
/// (e.g. recording end-to-end latency only for completed dictations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// The result reached the user (injected, or intentionally not pasted).
    Delivered,
    /// The pipeline failed; a [`PipelineEvent::Failed`] was emitted.
    Failed,
}

/// Millisecond pause between injecting text and pressing Return, giving the
/// target app time to commit the inserted text.
pub const PRESS_ENTER_DELAY_MS: u64 = 50;

/// Shared post-LLM flow: deliver the processed text and notify the UI.
///
/// Exactly one terminal event is emitted:
/// * LLM ok, injected (or `auto_paste` off) → [`PipelineEvent::Delivered`]
/// * LLM ok, injection failed → [`PipelineEvent::Failed`] (classified)
/// * LLM error → [`PipelineEvent::Failed`] (classified, `LLM processing failed:` prefix)
///
/// `press_enter` failures are non-fatal (the text already landed): they are
/// logged by the sink implementation and do not change the outcome.
pub async fn deliver_llm_result(
    llm: Result<LlmStageOutput, String>,
    events: &dyn EventSink,
    text_sink: &dyn TextSink,
) -> DeliveryOutcome {
    let out = match llm {
        Ok(out) => out,
        Err(e) => {
            eprintln!("fonos: pipeline error: LLM processing failed: {e}");
            events.emit(PipelineEvent::Failed(classify_error(&format!(
                "LLM processing failed: {e}"
            ))));
            return DeliveryOutcome::Failed;
        }
    };

    if !out.processed.is_empty() && out.auto_paste {
        if let Err(e) = text_sink.inject(&out.processed) {
            eprintln!("fonos: pipeline error: Injection failed: {e}");
            events.emit(PipelineEvent::Failed(classify_error(&format!(
                "Injection failed: {e}"
            ))));
            return DeliveryOutcome::Failed;
        }
        if out.auto_press_enter {
            tokio::time::sleep(std::time::Duration::from_millis(PRESS_ENTER_DELAY_MS)).await;
            if let Err(e) = text_sink.press_enter() {
                eprintln!("fonos: press_enter failed: {e}");
            }
        }
    }

    events.emit(PipelineEvent::Delivered {
        raw: String::new(),
        final_text: out.processed,
        workflow: None,
    });
    DeliveryOutcome::Delivered
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeSink {
        events: Mutex<Vec<PipelineEvent>>,
    }
    impl EventSink for FakeSink {
        fn emit(&self, event: PipelineEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    struct FakeText {
        inject_result: Result<(), String>,
        injected: Mutex<Vec<String>>,
        entered: Mutex<u32>,
    }
    impl FakeText {
        fn ok() -> Self {
            Self { inject_result: Ok(()), injected: Mutex::new(vec![]), entered: Mutex::new(0) }
        }
        fn failing(msg: &str) -> Self {
            Self {
                inject_result: Err(msg.into()),
                injected: Mutex::new(vec![]),
                entered: Mutex::new(0),
            }
        }
    }
    impl TextSink for FakeText {
        fn inject(&self, text: &str) -> Result<(), String> {
            self.injected.lock().unwrap().push(text.to_string());
            self.inject_result.clone()
        }
        fn press_enter(&self) -> Result<(), String> {
            *self.entered.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn out(processed: &str, auto_paste: bool, auto_press_enter: bool) -> LlmStageOutput {
        LlmStageOutput { processed: processed.into(), auto_paste, auto_press_enter }
    }

    #[tokio::test]
    async fn llm_ok_injects_and_delivers() {
        let (sink, text) = (FakeSink::default(), FakeText::ok());
        let r = deliver_llm_result(Ok(out("hello", true, false)), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Delivered);
        assert_eq!(*text.injected.lock().unwrap(), vec!["hello"]);
        assert_eq!(*text.entered.lock().unwrap(), 0);
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![PipelineEvent::Delivered {
                raw: String::new(),
                final_text: "hello".into(),
                workflow: None
            }]
        );
    }

    #[tokio::test]
    async fn auto_press_enter_after_successful_injection() {
        let (sink, text) = (FakeSink::default(), FakeText::ok());
        let r = deliver_llm_result(Ok(out("hi", true, true)), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Delivered);
        assert_eq!(*text.entered.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn auto_paste_off_delivers_without_injecting() {
        let (sink, text) = (FakeSink::default(), FakeText::ok());
        let r = deliver_llm_result(Ok(out("clipboard only", false, true)), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Delivered);
        assert!(text.injected.lock().unwrap().is_empty());
        assert_eq!(*text.entered.lock().unwrap(), 0, "no Enter when nothing was injected");
    }

    #[tokio::test]
    async fn empty_processed_text_still_delivers_without_injecting() {
        let (sink, text) = (FakeSink::default(), FakeText::ok());
        let r = deliver_llm_result(Ok(out("", true, false)), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Delivered);
        assert!(text.injected.lock().unwrap().is_empty());
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![PipelineEvent::Delivered {
                raw: String::new(),
                final_text: String::new(),
                workflow: None
            }]
        );
    }

    #[tokio::test]
    async fn injection_failure_emits_classified_failure_only() {
        let (sink, text) = (
            FakeSink::default(),
            FakeText::failing("Accessibility permission not granted — grant it"),
        );
        let r = deliver_llm_result(Ok(out("hello", true, true)), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Failed);
        assert_eq!(*text.entered.lock().unwrap(), 0, "no Enter after failed injection");
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1, "exactly one terminal event");
        match &events[0] {
            PipelineEvent::Failed(s) => assert_eq!(s.pane, Some("accessibility")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_error_is_classified_with_prefix() {
        let (sink, text) = (FakeSink::default(), FakeText::ok());
        let r =
            deliver_llm_result(Err("LLM API error 401: bad key".into()), &sink, &text).await;
        assert_eq!(r, DeliveryOutcome::Failed);
        assert!(text.injected.lock().unwrap().is_empty());
        let events = sink.events.lock().unwrap();
        match &events[0] {
            PipelineEvent::Failed(s) => assert!(s.message.contains("API key"), "{}", s.message),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}

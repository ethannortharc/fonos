//! Meeting summary prompt builder — constructs (system, user) prompt pairs
//! for LLM-based meeting summarization.

/// A single transcript entry used as input to summary prompt construction.
#[derive(Debug, Clone)]
pub struct SummaryEntry {
    /// Speaker label (e.g. `"Me"`, `"Others"`, `"Speaker 1"`).
    pub speaker: String,
    /// Offset from session start in milliseconds.
    pub timestamp_ms: u64,
    /// Transcript text for this segment.
    pub text: String,
}

/// Default system prompt for meeting summarization.
const DEFAULT_SYSTEM_PROMPT: &str = concat!(
    "You are an expert meeting summarizer. ",
    "Given a meeting transcript with speaker labels and timestamps, produce:\n",
    "1. A concise summary (3–5 sentences) of the main topics discussed.\n",
    "2. Key discussion points organized by topic.\n",
    "3. Action items in the format: '- [ ] <assignee>: <task> (by <deadline if mentioned>)'.\n",
    "4. Decisions made during the meeting.\n\n",
    "Use Markdown formatting. Output only the summary — do not repeat the transcript."
);

/// Build a `(system_prompt, user_prompt)` pair for meeting summarization.
///
/// * `entries` — transcript entries in chronological order
/// * `custom_prompt` — optional replacement for the system prompt
///
/// The user prompt contains the full transcript formatted with speaker labels
/// and human-readable timestamps.
pub fn build_summary_prompt(
    entries: &[SummaryEntry],
    custom_prompt: Option<&str>,
) -> (String, String) {
    let system_prompt = match custom_prompt {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => DEFAULT_SYSTEM_PROMPT.to_string(),
    };

    let mut user_parts = Vec::new();
    user_parts.push("## Meeting Transcript\n".to_string());

    for entry in entries {
        let ts = format_timestamp_ms(entry.timestamp_ms);
        user_parts.push(format!("**[{}] {}**: {}", ts, entry.speaker, entry.text));
    }

    if entries.is_empty() {
        user_parts.push("(No transcript entries — meeting may have had no audio.)".to_string());
    }

    user_parts.push("\n\nPlease summarize this meeting.".to_string());

    let user_prompt = user_parts.join("\n");

    (system_prompt, user_prompt)
}

/// Format milliseconds as `MM:SS` (or `HH:MM:SS` for sessions over an hour).
fn format_timestamp_ms(ms: u64) -> String {
    let total_secs = ms / 1_000;
    let h = total_secs / 3_600;
    let m = (total_secs % 3_600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_mentions_meeting() {
        let (sys, _) = build_summary_prompt(&[], None);
        let lower = sys.to_lowercase();
        assert!(lower.contains("meeting") || lower.contains("summar"));
    }

    #[test]
    fn custom_prompt_is_used() {
        let (sys, _) = build_summary_prompt(&[], Some("Custom instruction"));
        assert!(sys.contains("Custom"));
    }

    #[test]
    fn user_prompt_includes_speaker_and_text() {
        let entries = vec![
            SummaryEntry { speaker: "Me".into(), timestamp_ms: 0, text: "Hello.".into() },
        ];
        let (_, user) = build_summary_prompt(&entries, None);
        assert!(user.contains("Me"));
        assert!(user.contains("Hello."));
    }
}

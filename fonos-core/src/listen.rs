//! Listen workflow (issue #23): shared text-to-speech utilities used by the
//! `wf.listen` engine recipe (and the desktop `speak` output) to turn
//! processed text into one playable item.
//!
//! Platform-independent: the shell supplies a [`TtsEngine`] and decides where
//! the audio file lands and how the item is stored. Long texts are
//! synthesized sentence-chunk by sentence-chunk and concatenated, so TTS
//! backends never see over-long inputs.
//!
//! The full "captured text → title + listenable rewrite → speech" pipeline
//! this module used to run itself (via a legacy `modes::Mode`) was deleted in
//! Workbench P2 Task 12 — Listen has run through the workflow engine's
//! `llm.listen` widget + `wf.listen` recipe since Task 10; only the shared
//! synthesis/sanitization helpers below remain, still used by that recipe's
//! `speak` output and by the agent/meeting/doctor widgets.

use crate::tts::TtsEngine;

/// Max characters per TTS synthesis call; longer texts are chunked at
/// sentence boundaries and the WAVs concatenated.
pub const TTS_CHUNK_CHARS: usize = 480;

/// Sanitize, sentence-chunk, synthesize, and concatenate `text` into one WAV.
///
/// Shared by the `wf.listen` recipe's `speak` output and every other widget
/// that synthesizes long text (agent/meeting/doctor) so they all feed the TTS
/// backend identically: emoji/markup is stripped, the text is split at
/// sentence boundaries so the backend never sees an over-long input, each
/// chunk is synthesized, and the WAVs are concatenated. Errors when the
/// sanitized text is empty (nothing speakable) or synthesis fails.
pub async fn synthesize_long_text(text: &str, tts: &dyn TtsEngine) -> Result<Vec<u8>, String> {
    let speech_text = sanitize_for_speech(text);
    if speech_text.is_empty() {
        return Err("Nothing speakable after removing symbols".to_string());
    }
    let mut wavs = Vec::new();
    for chunk in split_for_tts(&speech_text, TTS_CHUNK_CHARS) {
        wavs.push(tts.synthesize(&chunk).await.map_err(|e| format!("TTS failed: {e}"))?);
    }
    concat_wavs(&wavs)
}

/// First-words fallback title when an LLM title call fails.
pub fn fallback_title(text: &str) -> String {
    clip(text.split_whitespace().collect::<Vec<_>>().join(" ").trim(), 40)
}

fn clip(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

/// Split text into TTS-sized chunks at sentence boundaries (CJK and Latin
/// terminators, then newlines), force-cutting only when a single sentence
/// exceeds the budget.
pub fn split_for_tts(text: &str, max_chars: usize) -> Vec<String> {
    const TERMINATORS: &[char] = &['。', '！', '？', '．', '.', '!', '?', '\n', '；', ';'];
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut sentence = String::new();

    let flush_sentence = |current: &mut String, sentence: &mut String, chunks: &mut Vec<String>| {
        if current.chars().count() + sentence.chars().count() > max_chars && !current.trim().is_empty() {
            chunks.push(std::mem::take(current).trim().to_string());
        }
        current.push_str(sentence);
        sentence.clear();
        // a single over-long sentence: force-cut
        while current.chars().count() > max_chars {
            let cut: String = current.chars().take(max_chars).collect();
            let rest: String = current.chars().skip(max_chars).collect();
            chunks.push(cut.trim().to_string());
            *current = rest;
        }
    };

    for ch in text.chars() {
        sentence.push(ch);
        if TERMINATORS.contains(&ch) {
            flush_sentence(&mut current, &mut sentence, &mut chunks);
        }
    }
    flush_sentence(&mut current, &mut sentence, &mut chunks);
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks.retain(|c| !c.is_empty());
    chunks
}

/// Strip characters that TTS engines misread: emoji and pictographs,
/// markdown markers, decorative bullets/arrows. Regular punctuation (CJK and
/// Latin) is kept. Display text stays untouched — this runs only on the copy
/// sent to synthesis.
pub fn sanitize_for_speech(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        let is_emoji = matches!(code,
            0x1F000..=0x1FAFF   // emoji, symbols & pictographs, supplemental
            | 0x2600..=0x27BF   // misc symbols + dingbats
            | 0x2B00..=0x2BFF   // arrows/stars used as emoji
            | 0x2190..=0x21FF   // arrows
            | 0xFE00..=0xFE0F   // variation selectors
            | 0x200D            // zero-width joiner
            | 0x20E3            // combining keycap
        );
        let is_markup = matches!(ch, '*' | '#' | '`' | '~' | '|' | '•' | '◦' | '▪' | '·');
        if is_emoji || is_markup {
            continue;
        }
        out.push(ch);
    }
    // Collapse runs of spaces left behind by removals (newlines preserved).
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        collapsed.push(ch);
    }
    collapsed.trim().to_string()
}

/// Split a reply into speech clauses for paced synthesis: boundaries at
/// sentence enders AND commas. Chunk targets are graded — a tiny first chunk
/// (speech starts at the first comma, minimizing time-to-first-sound on
/// slower-than-real-time engines), a small second, larger after — so early
/// playback buys synthesis time for the rest. Chunks close at clause
/// boundaries; only pathological boundary-less text is force-cut.
pub fn split_for_speech(text: &str) -> Vec<String> {
    const BOUNDARIES: &[char] = &[
        '。', '！', '？', '．', '.', '!', '?', '\n', '；', ';', '，', ',', '、', '：', ':',
    ];
    /// Close the chunk at the first clause boundary past this many chars.
    fn target(chunk_idx: usize) -> usize {
        match chunk_idx {
            0 => 10,
            1 => 40,
            _ => 90,
        }
    }
    /// Boundary-less safety cap: force-cut runs longer than this.
    const HARD_CAP: usize = 160;

    // Cut into clauses (keeping their trailing delimiter).
    let mut clauses: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if BOUNDARIES.contains(&ch) {
            if !cur.trim().is_empty() {
                clauses.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if !cur.trim().is_empty() {
        clauses.push(cur);
    }

    // Pack clauses until the current chunk reaches its target size.
    let mut chunks: Vec<String> = Vec::new();
    let mut acc = String::new();
    for clause in clauses {
        acc.push_str(&clause);
        if acc.chars().count() >= target(chunks.len()) {
            chunks.push(std::mem::take(&mut acc).trim().to_string());
        }
    }
    if !acc.trim().is_empty() {
        chunks.push(acc.trim().to_string());
    }

    // Safety: force-cut anything still over the hard cap.
    let mut out = Vec::new();
    for chunk in chunks {
        let mut rest = chunk;
        while rest.chars().count() > HARD_CAP {
            let cut: String = rest.chars().take(HARD_CAP).collect();
            rest = rest.chars().skip(HARD_CAP).collect();
            out.push(cut.trim().to_string());
        }
        if !rest.trim().is_empty() {
            out.push(rest.trim().to_string());
        }
    }
    out.retain(|c| !c.is_empty());
    out
}

/// Concatenate several 16-bit PCM WAV files into one.
///
/// All inputs must share the format of the first (validated); output reuses
/// the first file's header with a recomputed length.
pub fn concat_wavs(wavs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let non_empty: Vec<&Vec<u8>> = wavs.iter().filter(|w| !w.is_empty()).collect();
    match non_empty.len() {
        0 => return Err("no audio produced".to_string()),
        1 => return Ok(non_empty[0].clone()),
        _ => {}
    }

    let first = parse_wav(non_empty[0])?;
    let mut data = Vec::new();
    for wav in &non_empty {
        let parsed = parse_wav(wav)?;
        if parsed.fmt != first.fmt {
            return Err("TTS chunks returned mismatched WAV formats".to_string());
        }
        data.extend_from_slice(parsed.data);
    }

    // RIFF header + fmt chunk copied from the first file, fresh data chunk.
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&(first.fmt.len() as u32).to_le_bytes());
    out.extend_from_slice(first.fmt);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    Ok(out)
}

/// Duration in seconds of a PCM WAV, from its fmt byte-rate and data length.
pub fn wav_duration_secs(wav: &[u8]) -> Option<f64> {
    let parsed = parse_wav(wav).ok()?;
    if parsed.fmt.len() < 12 {
        return None;
    }
    let byte_rate = u32::from_le_bytes(parsed.fmt[8..12].try_into().ok()?) as f64;
    if byte_rate <= 0.0 {
        return None;
    }
    Some(parsed.data.len() as f64 / byte_rate)
}

pub(crate) struct ParsedWav<'a> {
    pub(crate) fmt: &'a [u8],
    pub(crate) data: &'a [u8],
}

/// Minimal RIFF walk: locate the `fmt ` and `data` chunks.
pub(crate) fn parse_wav(bytes: &[u8]) -> Result<ParsedWav<'_>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".to_string());
    }
    let mut fmt: Option<&[u8]> = None;
    let mut data: Option<&[u8]> = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_end = (pos + 8 + size).min(bytes.len());
        match id {
            b"fmt " => fmt = Some(&bytes[pos + 8..body_end]),
            b"data" => data = Some(&bytes[pos + 8..body_end]),
            _ => {}
        }
        // chunks are word-aligned
        pos = pos + 8 + size + (size & 1);
    }
    match (fmt, data) {
        (Some(fmt), Some(data)) => Ok(ParsedWav { fmt, data }),
        _ => Err("WAV missing fmt/data chunk".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_wav(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let mut pcm = Vec::new();
        for s in samples {
            pcm.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + pcm.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        out.extend_from_slice(&pcm);
        out
    }

    #[test]
    fn split_respects_sentence_boundaries() {
        let text = "第一句话。第二句话！Third sentence. Fourth?";
        let chunks = split_for_tts(text, 20);
        assert_eq!(chunks, vec!["第一句话。第二句话！", "Third sentence.", "Fourth?"]);
    }

    #[test]
    fn split_short_text_is_one_chunk() {
        assert_eq!(split_for_tts("你好世界。", 480), vec!["你好世界。"]);
    }

    #[test]
    fn split_force_cuts_single_overlong_sentence() {
        let long = "字".repeat(1000);
        let chunks = split_for_tts(&long, 480);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 480));
        assert_eq!(chunks.join(""), long);
    }

    #[test]
    fn concat_merges_pcm_and_rebuilds_header() {
        let a = tiny_wav(16000, &[1, 2, 3]);
        let b = tiny_wav(16000, &[4, 5]);
        let merged = concat_wavs(&[a, b]).unwrap();
        let parsed = parse_wav(&merged).unwrap();
        assert_eq!(parsed.data.len(), 10); // 5 samples * 2 bytes
        assert_eq!(&merged[0..4], b"RIFF");
    }

    #[test]
    fn concat_rejects_mismatched_formats() {
        let a = tiny_wav(16000, &[1]);
        let b = tiny_wav(24000, &[2]);
        assert!(concat_wavs(&[a, b]).is_err());
    }

    #[test]
    fn concat_single_passthrough_and_empty_error() {
        let a = tiny_wav(16000, &[7]);
        assert_eq!(concat_wavs(&[a.clone()]).unwrap(), a);
        assert!(concat_wavs(&[]).is_err());
    }

    #[test]
    fn speech_split_small_first_chunk_then_clauses() {
        let text = "你好呀，今天天气真不错。我们出去散散步怎么样？顺便可以买一杯咖啡。";
        let chunks = split_for_speech(text);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].chars().count() <= 14, "first chunk small: {:?}", chunks[0]);
        assert_eq!(chunks.join(""), text);
    }

    #[test]
    fn speech_split_short_reply_single_chunk() {
        assert_eq!(split_for_speech("好的。"), vec!["好的。"]);
    }

    #[test]
    fn speech_split_force_cuts_no_boundary_text() {
        let long = "x".repeat(400);
        let chunks = split_for_speech(&long);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 160));
        assert_eq!(chunks.join(""), long);
    }

    #[test]
    fn sanitize_strips_emoji_and_markup_keeps_text() {
        let t = "好的！😊 今天的**重点**有三个：`code` 和 → 箭头 ⭐。Sure thing! 👍";
        let clean = sanitize_for_speech(t);
        assert_eq!(clean, "好的！ 今天的重点有三个：code 和 箭头 。Sure thing!");
    }

    #[test]
    fn sanitize_keeps_normal_punctuation() {
        let t = "价格是 3.5 元，占比 50%；明天 10:30 见。";
        assert_eq!(sanitize_for_speech(t), t);
    }

    #[test]
    fn fallback_title_clips() {
        let t = fallback_title(&"word ".repeat(30));
        assert!(t.chars().count() <= 41);
        assert!(t.ends_with('…'));
        assert_eq!(fallback_title("short note"), "short note");
    }

    /// A fake TTS that returns a one-sample WAV per call, so the concat is
    /// observable without a live backend.
    struct CountingTts {
        calls: std::sync::atomic::AtomicUsize,
    }
    #[async_trait::async_trait]
    impl TtsEngine for CountingTts {
        async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(tiny_wav(16000, &[text.chars().count() as i16]))
        }
    }

    #[tokio::test]
    async fn synthesize_long_text_chunks_synthesizes_and_concats() {
        // Two sentences, each well under the chunk budget but split on the
        // boundary once the text as a whole is long enough — force multiple
        // chunks by exceeding TTS_CHUNK_CHARS.
        let sentence = "这是一个用于测试的句子。"; // 12 chars incl. terminator
        let text = sentence.repeat(60); // 720 chars > 480 budget ⇒ >1 chunk
        let tts = CountingTts { calls: std::sync::atomic::AtomicUsize::new(0) };
        let wav = synthesize_long_text(&text, &tts).await.unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert!(
            tts.calls.load(std::sync::atomic::Ordering::SeqCst) >= 2,
            "over-budget text must be synthesized in multiple chunks"
        );
    }

    #[tokio::test]
    async fn synthesize_long_text_errors_when_nothing_speakable() {
        let tts = CountingTts { calls: std::sync::atomic::AtomicUsize::new(0) };
        // Emoji/markup only ⇒ sanitizes to empty ⇒ Err before any synthesis.
        assert!(synthesize_long_text("😀⭐️ **``**", &tts).await.is_err());
        assert_eq!(tts.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

}

//! Shared playback-loudness reference for the call loop's barge detector.
//!
//! Every playback path in the app owns the PCM it emits, so each can record how
//! loud its output *is* on a wall-clock timeline and, given the elapsed time
//! since the queue started, answer "how loud is the assistant right now?" — the
//! reference [`fonos_core::vad::BargeDetector`] gates the mic against. Both
//! [`crate::audio::playback::AudioPlayback`] (rodio, streaming TTS) and the
//! macOS voice-capture helper link ([`crate::audio::voice_capture`]) embed one,
//! so the coupling×reference dynamic bar engages on either path. All values are
//! i16 RMS units, matching [`fonos_core::vad::rms`].

use std::collections::VecDeque;
use std::time::Instant;

/// Target block length for the reference timeline.
const REF_BLOCK_MS: u32 = 100;
/// Half-width (ms) of the window searched around the estimated playback
/// position — wide enough to stay conservative against position error (rodio
/// sink drift; pipe + AVAudioEngine scheduling latency on the helper path).
const REF_WINDOW_MS: u64 = 400;
/// Capacity of the fine-grained raw-PCM ring (16 kHz mono i16), ~4 s = 128 KB —
/// the source the call-mode echo verifier cross-correlates against the mic. The
/// 100 ms block-RMS timeline above is too coarse for the 10 ms envelope that
/// needs.
const RAW_RING_CAPACITY: usize = 4 * 16_000;

/// Per-100ms RMS timeline of the audio pushed into the current playback queue.
///
/// We own every PCM chunk we play, so we can record how loud the output *is* on
/// a timeline and, given the wall-clock elapsed since the queue started, answer
/// the reference the barge detector gates the mic against.
#[derive(Default)]
pub(crate) struct RefEnvelope {
    /// `(block_rms, block_ms)` for each ~100 ms block, in play order.
    blocks: VecDeque<(f32, u32)>,
    /// Total duration (ms) represented by `blocks`.
    total_ms: u64,
    /// Wall-clock instant the current queue began playing (first append after
    /// the queue was empty). `None` when idle.
    queue_start: Option<Instant>,
    /// Rolling raw 16 kHz mono i16 PCM of recent playback (capacity
    /// [`RAW_RING_CAPACITY`]), newest at the back — the fine-grained source the
    /// call-mode echo verifier correlates against the mic.
    raw: VecDeque<i16>,
    /// Total 16 kHz-mono samples ever pushed into `raw` (monotonic within a
    /// timeline; reset by [`reset`](Self::reset)). Lets [`recent_reference`] map
    /// a wall-clock playback position onto ring indices even after eviction.
    ///
    /// [`recent_reference`]: Self::recent_reference
    raw_total: u64,
}

impl RefEnvelope {
    /// Drop the whole timeline (queue drained, stopped, flushed, or replaced).
    pub(crate) fn reset(&mut self) {
        self.blocks.clear();
        self.total_ms = 0;
        self.queue_start = None;
        self.raw.clear();
        self.raw_total = 0;
    }

    /// Start a fresh timeline anchored at *now* — call when appending onto an
    /// idle queue (the first frame after a drain/flush). Equivalent to
    /// [`reset`](Self::reset) plus anchoring the position clock.
    pub(crate) fn begin(&mut self) {
        self.reset();
        self.queue_start = Some(Instant::now());
    }

    /// Append the per-100ms-block RMS of `samples` (interleaved i16 at
    /// `sample_rate` × `channels`) to the timeline.
    pub(crate) fn push_samples(&mut self, samples: &[i16], sample_rate: u32, channels: u16) {
        if samples.is_empty() || sample_rate == 0 || channels == 0 {
            return;
        }
        // Samples per full block (all channels), and the samples-per-ms factor.
        let block_len =
            ((sample_rate as usize * REF_BLOCK_MS as usize / 1000) * channels as usize).max(1);
        let per_ms = (sample_rate as u64 * channels as u64).max(1); // samples/sec·ch → /1000 below
        for block in samples.chunks(block_len) {
            let sum_sq: f64 = block.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / block.len() as f64).sqrt() as f32;
            let block_ms = ((block.len() as u64 * 1000) / per_ms).max(1) as u32;
            self.blocks.push_back((rms, block_ms));
            self.total_ms += block_ms as u64;
        }
        // Also keep the fine-grained raw ring at a fixed 16 kHz mono, downmixing
        // and resampling as needed so callers get one consistent rate.
        for s in to_mono_16k(samples, sample_rate, channels) {
            if self.raw.len() >= RAW_RING_CAPACITY {
                self.raw.pop_front();
            }
            self.raw.push_back(s);
            self.raw_total += 1;
        }
    }

    /// The most recent `duration_ms` of reference PCM (16 kHz mono), ending at
    /// the current estimated playback position — the window aligned with a mic
    /// snippet ending "now", for the call-mode echo cross-correlation.
    ///
    /// Position is the wall-clock elapsed since the queue started (clamped to the
    /// total queued duration), the same clock [`reference_rms`](Self::reference_rms)
    /// uses. Because both mic and reference end at that same wall-clock instant,
    /// the acoustic + pipeline delay falls out as a small positive lag the
    /// caller's cross-correlation recovers. Returns `Vec::new()` when idle, or
    /// when the requested window predates what the ring still holds (the caller
    /// then defers to the ASR check).
    pub(crate) fn recent_reference(&self, duration_ms: u64) -> Vec<i16> {
        if self.raw.is_empty() {
            return Vec::new();
        }
        let Some(start) = self.queue_start else {
            return Vec::new();
        };
        let pos_ms = (start.elapsed().as_millis() as u64).min(self.total_ms);
        let ring_len = self.raw.len() as i64;
        // Timeline sample index of the oldest sample still in the ring.
        let oldest = self.raw_total as i64 - ring_len;
        let pos_samp = (pos_ms * 16) as i64; // 16 samples/ms @ 16 kHz
        let dur_samp = (duration_ms * 16) as i64;
        let end = (pos_samp - oldest).clamp(0, ring_len);
        let begin = (pos_samp - dur_samp - oldest).clamp(0, ring_len);
        if end <= begin {
            return Vec::new();
        }
        self.raw
            .iter()
            .skip(begin as usize)
            .take((end - begin) as usize)
            .copied()
            .collect()
    }

    /// Max block RMS within ±[`REF_WINDOW_MS`] of `pos_ms` along the timeline.
    fn max_rms_around(&self, pos_ms: u64) -> f32 {
        let lo = pos_ms.saturating_sub(REF_WINDOW_MS);
        let hi = pos_ms + REF_WINDOW_MS;
        let mut cursor = 0u64;
        let mut peak = 0.0f32;
        for &(rms, block_ms) in &self.blocks {
            let start = cursor;
            let end = cursor + block_ms as u64;
            // Overlap test between [start, end) and [lo, hi].
            if end > lo && start <= hi && rms > peak {
                peak = rms;
            }
            cursor = end;
        }
        peak
    }

    /// Current playback loudness (i16 RMS units): estimate the playback position
    /// as the wall-clock time elapsed since the queue started (clamped to the
    /// total queued duration) and return the MAX block RMS within a ±400 ms
    /// window around it — conservative against position drift, so a momentary
    /// estimation error can never make the reference read *quieter* than the
    /// audio actually reaching the speaker. Returns `0.0` when the timeline is
    /// empty or was never anchored; callers gate this on their own "queue empty"
    /// signal (rodio `sink.empty()`, helper `DRAIN`).
    pub(crate) fn reference_rms(&self) -> f32 {
        if self.blocks.is_empty() {
            return 0.0;
        }
        let Some(start) = self.queue_start else {
            return 0.0;
        };
        let pos_ms = (start.elapsed().as_millis() as u64).min(self.total_ms);
        self.max_rms_around(pos_ms)
    }
}

/// Downmix interleaved multi-channel audio to mono and resample to 16 kHz — the
/// fixed rate of the raw reference ring, so [`RefEnvelope::recent_reference`]
/// always returns 16 kHz mono regardless of the TTS server's native format.
fn to_mono_16k(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<i16> {
    if samples.is_empty() || sample_rate == 0 || channels == 0 {
        return Vec::new();
    }
    let mono: Vec<i16> = if channels > 1 {
        samples
            .chunks(channels as usize)
            .map(|f| (f.iter().map(|&s| s as i32).sum::<i32>() / f.len() as i32) as i16)
            .collect()
    } else {
        samples.to_vec()
    };
    if sample_rate == 16_000 {
        mono
    } else {
        fonos_core::audio::resample_i16(&mono, sample_rate, 16_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ms` of a square wave at `amp` (RMS == `amp`) at `sr` Hz mono.
    fn tone(ms: u32, sr: u32, amp: i16) -> Vec<i16> {
        let n = ms as usize * sr as usize / 1000;
        (0..n).map(|i| if i % 2 == 0 { amp } else { -amp }).collect()
    }

    /// `push_samples` accounts one ~100 ms block per 100 ms of audio and sums
    /// the total duration; a fresh envelope is idle and reads zero.
    #[test]
    fn push_accounts_blocks_and_duration() {
        let mut env = RefEnvelope::default();
        assert!(env.blocks.is_empty());
        assert_eq!(env.reference_rms(), 0.0, "no blocks → 0 (never anchored)");
        // 1 s of 16 kHz mono → ten 100 ms blocks, 1000 ms total.
        env.push_samples(&tone(1000, 16_000, 1234), 16_000, 1);
        assert_eq!(env.total_ms, 1000);
        assert_eq!(env.blocks.len(), 10);
    }

    /// The reference reads the loudest block within ±400 ms of the current
    /// position: just after `begin` (position ≈ 0) a quiet lead-in followed by a
    /// loud swell inside the window reports the swell, not the lead-in — the
    /// dynamic bar the barge detector needs.
    #[test]
    fn reference_reads_window_peak_at_current_position() {
        let mut env = RefEnvelope::default();
        let mut sig = tone(300, 16_000, 200); // quiet clause lead-in
        sig.extend(tone(300, 16_000, 4000)); // loud swell, within ±400 ms of t=0
        env.begin();
        env.push_samples(&sig, 16_000, 1);
        let r = env.reference_rms();
        assert!((3500.0..4100.0).contains(&r), "window peak ~4000, got {r}");
    }

    /// `recent_reference` returns the raw window ending at the current playback
    /// position, spanning back the requested duration.
    #[test]
    fn recent_reference_extracts_window_at_playback_position() {
        let mut env = RefEnvelope::default();
        env.begin();
        // 2 s of 16 kHz mono audio.
        env.push_samples(&tone(2_000, 16_000, 500), 16_000, 1);
        // Idle (no timeline) → empty regardless of duration asked.
        assert!(RefEnvelope::default().recent_reference(200).is_empty(), "idle → empty");
        // Let the playback clock advance, then read a 200 ms window.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let w = env.recent_reference(200);
        assert!(!w.is_empty(), "mid-playback window should be populated");
        // ~200 ms at 16 kHz ≈ 3200 samples; bound generously against jitter.
        assert!(w.len() <= 200 * 16 + 64, "bounded to requested span, got {}", w.len());
        assert!(w.len() >= 100 * 16, "roughly the requested span, got {}", w.len());
    }

    /// `reset` returns the envelope to idle/zero; `begin` re-anchors it.
    #[test]
    fn reset_returns_to_idle_zero() {
        let mut env = RefEnvelope::default();
        env.begin();
        env.push_samples(&tone(200, 16_000, 1000), 16_000, 1);
        assert!(env.reference_rms() > 0.0);
        env.reset();
        assert!(env.blocks.is_empty());
        assert_eq!(env.total_ms, 0);
        assert_eq!(env.reference_rms(), 0.0, "reset → idle → 0");
    }
}

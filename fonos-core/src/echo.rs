//! Model-free acoustic echo detection for call-mode barge-in, plus a text-level
//! echo test used as the fallback stage.
//!
//! Call mode re-opens the mic while the assistant speaks, so the mic captures
//! the assistant's own voice bleeding back (echo) alongside any real
//! interruption. Because the shell owns the reference PCM it pushes to the
//! speaker, a suspected barge can be settled *without a model*: cross-correlate
//! the mic's loudness envelope against the reference's. Echo tracks the
//! reference (high correlation, little unexplained energy); a genuine
//! interruption does not (low correlation) or adds energy the echo can't explain
//! (high residual).
//!
//! Everything here is pure and platform-independent; the shell owns capturing
//! the mic snippet and the aligned reference samples. All signal inputs are
//! 16 kHz mono: [`envelope`] assumes 16 samples/ms.

use std::collections::HashSet;

/// Per-hop RMS envelope of 16 kHz mono i16 samples. `hop_ms` at 16 kHz maps to
/// `hop_ms · 16` samples per hop; the trailing partial hop (if any) is kept and
/// measured over its own (shorter) length. A 10 ms hop is the call verifier's
/// default — fine enough to resolve syllable-scale loudness, coarse enough to
/// stay cheap.
pub fn envelope(samples: &[i16], hop_ms: u32) -> Vec<f32> {
    let hop = (hop_ms.max(1) as usize) * 16; // 16 samples/ms @ 16 kHz
    samples
        .chunks(hop)
        .map(|c| {
            let sum_sq: f64 = c.iter().map(|&s| (s as f64) * (s as f64)).sum();
            (sum_sq / c.len() as f64).sqrt() as f32
        })
        .collect()
}

/// Minimum overlapping hops for a correlation to be meaningful.
const MIN_OVERLAP_HOPS: usize = 4;

/// Peak normalized cross-correlation of `mic_env` against `ref_env`, searching
/// mic delays of `0 ..= max_lag_hops` hops, and the lag at which it peaks.
///
/// At lag `L` the mic's later samples are paired with the reference's earlier
/// ones (`mic_env[L + j]` vs `ref_env[j]`) — the echo in the mic *follows* the
/// reference by the acoustic + pipeline delay, so the true alignment shows up as
/// a small positive lag. Each overlap is Pearson-correlated (zero-mean,
/// unit-variance per window), so a constant scale difference between the
/// unit-gain reference and the makeup-gained mic doesn't matter; only the
/// *shape* over time does. Returns `(0.0, 0)` when no lag has enough overlap or
/// every window is flat (degenerate — the caller then defers to the ASR check).
pub fn xcorr_peak(mic_env: &[f32], ref_env: &[f32], max_lag_hops: usize) -> (f32, usize) {
    let mut best = 0.0f32;
    let mut best_lag = 0usize;
    let mut found = false;
    for lag in 0..=max_lag_hops {
        if lag >= mic_env.len() {
            break;
        }
        let n = (mic_env.len() - lag).min(ref_env.len());
        if n < MIN_OVERLAP_HOPS {
            continue;
        }
        if let Some(c) = pearson(&mic_env[lag..lag + n], &ref_env[..n]) {
            if !found || c > best {
                best = c;
                best_lag = lag;
                found = true;
            }
        }
    }
    (best, best_lag)
}

/// Fraction of the mic envelope's energy that the reference echo can NOT explain
/// at the given `lag` (0.0 … 1.0). A least-squares gain `a = ⟨mic,ref⟩ / ⟨ref,ref⟩`
/// (floored at 0 — a negative echo gain is unphysical) scales the reference to
/// best match the mic; the residual is the mic energy left *above* that scaled
/// echo (floored at 0 per hop — we only count excess, not shortfall). `0.0`
/// means the mic is essentially just scaled echo; values toward `1.0` mean most
/// of the mic is some other sound (a real voice overlapping the echo).
pub fn residual_ratio(mic_env: &[f32], ref_env: &[f32], lag: usize) -> f32 {
    if lag >= mic_env.len() {
        return 1.0;
    }
    let n = (mic_env.len() - lag).min(ref_env.len());
    if n == 0 {
        return 1.0;
    }
    let m = &mic_env[lag..lag + n];
    let r = &ref_env[..n];
    let rms_m = (m.iter().map(|&x| x * x).sum::<f32>() / n as f32).sqrt();
    if rms_m <= 1e-6 {
        return 0.0; // silent mic — nothing unexplained
    }
    let dot_mr: f32 = m.iter().zip(r).map(|(&x, &y)| x * y).sum();
    let dot_rr: f32 = r.iter().map(|&y| y * y).sum();
    let a = if dot_rr > 1e-6 { (dot_mr / dot_rr).max(0.0) } else { 0.0 };
    let resid_sq: f32 = m
        .iter()
        .zip(r)
        .map(|(&x, &y)| {
            let e = (x - a * y).max(0.0); // excess energy beyond the echo model
            e * e
        })
        .sum();
    let rms_resid = (resid_sq / n as f32).sqrt();
    (rms_resid / rms_m).clamp(0.0, 1.0)
}

/// Pearson correlation of two equal-length windows, or `None` if either is
/// (near-)flat (undefined correlation).
fn pearson(a: &[f32], b: &[f32]) -> Option<f32> {
    let n = a.len();
    if n == 0 || b.len() != n {
        return None;
    }
    let inv = 1.0 / n as f32;
    let mean_a = a.iter().sum::<f32>() * inv;
    let mean_b = b.iter().sum::<f32>() * inv;
    let (mut cov, mut va, mut vb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    if va <= 1e-9 || vb <= 1e-9 {
        return None;
    }
    Some(cov / (va.sqrt() * vb.sqrt()))
}

/// Similarity of a transcribed barge snippet to the reply being spoken right
/// now — the text-level echo test that gates a call-mode barge when the DSP
/// stage is inconclusive.
///
/// Returns the fraction of `transcript`'s character bigrams that also occur in
/// `reply` (0.0 … 1.0). Both strings are normalized first — lowercased, with
/// whitespace and punctuation removed — so formatting differences between the
/// STT output and the reply text don't matter. Character bigrams (rather than
/// word tokens) keep it working for CJK, which has no word spaces. A high value
/// means the snippet is largely contained in the reply — i.e. the mic captured
/// the assistant's own voice (echo), not new words.
///
/// Short-string guard: a normalized transcript shorter than 2 characters can't
/// form a bigram and is treated as noise/echo — it returns `1.0` so the caller
/// refutes the (probably spurious) trigger.
pub fn echo_similarity(transcript: &str, reply: &str) -> f32 {
    let t = normalize(transcript);
    if t.len() < 2 {
        return 1.0;
    }
    let r = normalize(reply);
    let reply_bigrams: HashSet<(char, char)> = r.windows(2).map(|w| (w[0], w[1])).collect();
    let mut total = 0usize;
    let mut present = 0usize;
    for w in t.windows(2) {
        total += 1;
        if reply_bigrams.contains(&(w[0], w[1])) {
            present += 1;
        }
    }
    if total == 0 {
        return 1.0;
    }
    present as f32 / total as f32
}

/// Lowercase and keep only alphanumeric characters (drops whitespace and
/// punctuation; keeps CJK ideographs, which are alphabetic).
fn normalize(s: &str) -> Vec<char> {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_per_hop_rms() {
        // 20 ms of constant amplitude 1000 at 16 kHz → two 10 ms hops, rms 1000.
        let s = vec![1000i16; 16 * 20];
        let e = envelope(&s, 10);
        assert_eq!(e.len(), 2);
        for v in e {
            assert!((v - 1000.0).abs() < 1.0, "hop rms ≈ 1000, got {v}");
        }
        assert!(envelope(&[], 10).is_empty(), "empty → empty");
    }

    /// A distinctive, aperiodic reference envelope for the correlation tests.
    fn ref_shape(n: usize) -> Vec<f32> {
        (0..n).map(|i| ((i * i * 7 + i * 3) % 97) as f32 + 5.0).collect()
    }

    #[test]
    fn xcorr_detects_scaled_delayed_echo() {
        let refe = ref_shape(60);
        // mic = quiet bleed for `lag` hops, then a 0.5× delayed copy of the ref.
        let lag = 5;
        let mut mic = vec![5.0f32; lag];
        mic.extend(refe.iter().map(|&x| x * 0.5));
        let (corr, best_lag) = xcorr_peak(&mic, &refe, 50);
        assert!(corr > 0.9, "scaled delayed copy → high corr, got {corr}");
        assert_eq!(best_lag, lag, "recovers the acoustic delay");
        let resid = residual_ratio(&mic, &refe, best_lag);
        assert!(resid < 0.15, "echo fully explains the mic → low residual, got {resid}");
    }

    #[test]
    fn residual_flags_overlapping_speech() {
        let refe = ref_shape(60);
        let lag = 4;
        // Pure echo (0.5× delayed copy) — the baseline residual.
        let mut echo_only = vec![5.0f32; lag];
        echo_only.extend(refe.iter().map(|&x| x * 0.5));
        let resid_echo_only = residual_ratio(&echo_only, &refe, lag);
        // Same echo plus an independent additive burst in the middle: still
        // correlated (echo present), but with a lot of unexplained energy.
        let mut mic = vec![5.0f32; lag];
        mic.extend(refe.iter().enumerate().map(|(i, &x)| {
            let burst = if (20..40).contains(&i) { 90.0 } else { 0.0 };
            x * 0.5 + burst
        }));
        let (corr, best_lag) = xcorr_peak(&mic, &refe, 50);
        let resid = residual_ratio(&mic, &refe, best_lag);
        assert!(corr > 0.4, "echo still present → non-trivial corr, got {corr}");
        assert!(
            resid > resid_echo_only + 0.2,
            "independent burst lifts the residual above pure echo ({resid} vs {resid_echo_only})"
        );
    }

    #[test]
    fn xcorr_unrelated_is_low() {
        // Two independent pseudo-random envelopes; long enough that spurious
        // correlations stay small even taking the max over many lags.
        let a: Vec<f32> = (0..200).map(|i| ((i * 13 + 7) % 29) as f32).collect();
        let b: Vec<f32> = (0..200).map(|i| ((i * 17 + 3) % 31) as f32).collect();
        let (corr, _) = xcorr_peak(&a, &b, 20);
        assert!(corr < 0.5, "unrelated envelopes → low corr, got {corr}");
    }

    #[test]
    fn dsp_guards_degenerate_inputs() {
        // Too short for the minimum overlap → no valid lag.
        assert_eq!(xcorr_peak(&[1.0, 2.0], &[1.0, 2.0], 10), (0.0, 0));
        // Flat windows: zero variance → correlation undefined → 0.
        let flat = vec![7.0f32; 40];
        assert_eq!(xcorr_peak(&flat, &flat, 10).0, 0.0);
        // Silent mic: nothing unexplained → residual 0.
        assert_eq!(residual_ratio(&vec![0.0; 40], &vec![5.0; 40], 0), 0.0);
        // Lag past the mic length → fully unexplained (fail toward "other sound").
        assert_eq!(residual_ratio(&[1.0, 2.0], &[1.0, 2.0], 10), 1.0);
    }

    // ── echo_similarity (text-level fallback) ────────────────────────────────

    #[test]
    fn echo_similarity_exact_substring_is_high() {
        let reply = "今天天气很好，我们去公园散步吧";
        let transcript = "今天天气很好"; // a clean substring of the reply
        assert!(echo_similarity(transcript, reply) > 0.9, "substring → echo");
    }

    #[test]
    fn echo_similarity_paraphrase_is_mid() {
        let reply = "我喜欢在早上喝咖啡";
        let transcript = "我早上喝茶"; // partial overlap
        let s = echo_similarity(transcript, reply);
        assert!((0.2..0.8).contains(&s), "paraphrase-ish overlap is mid, got {s}");
    }

    #[test]
    fn echo_similarity_unrelated_is_low() {
        let reply = "the weather is nice today";
        let transcript = "我想要一杯咖啡"; // unrelated, different script
        assert!(echo_similarity(transcript, reply) < 0.2, "unrelated → low");
    }

    #[test]
    fn echo_similarity_empty_or_tiny_is_treated_as_echo() {
        // Empty / whitespace / single char cannot be a real interruption.
        assert_eq!(echo_similarity("", "anything"), 1.0);
        assert_eq!(echo_similarity("   ", "anything"), 1.0);
        assert_eq!(echo_similarity("啊", "anything"), 1.0);
    }

    #[test]
    fn echo_similarity_boundary_is_exact_fraction() {
        // transcript bigrams: ab, bc, cd, de ; reply bigrams: ab, bc, cx, xy
        // present: ab, bc → 2 of 4 = 0.5 exactly (punctuation/case normalized).
        let s = echo_similarity("Ab-c, d e", "a b c x y");
        assert!((s - 0.5).abs() < 1e-6, "expected 0.5, got {s}");
    }
}

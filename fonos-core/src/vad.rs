//! Energy-based voice activity detection for hands-free "call mode" (issue #24).
//!
//! Pure and platform-independent: the shell feeds fixed-size 16 kHz i16 PCM
//! chunks via [`VadSession::push`] and reacts to the returned [`VadEvent`].
//! The detector maintains an adaptive noise floor (a slow EMA of the RMS while
//! no speech is present), so a quiet background hum is learned and ignored
//! while real speech — several times louder than the floor — trips the
//! threshold. A short "maybe" window filters clicks (`min_speech_ms`); a
//! hangover counter ends the utterance after `silence_hang_ms` of trailing
//! quiet; and hard caps (`max_utterance_ms`, `timeout_ms`) guarantee the loop
//! always makes progress.

/// Tuning knobs for [`VadSession`]. `Default` matches the call-mode defaults.
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// 0.0 (only loud, clearly-separated speech) … 1.0 (very eager). Lower
    /// values demand more energy over the noise floor before speech is
    /// detected. Default `0.5`.
    pub sensitivity: f32,
    /// Minimum continuous speech before an utterance is confirmed and
    /// [`VadEvent::SpeechStart`] fires — filters clicks and lip smacks.
    /// Default `250`.
    pub min_speech_ms: u32,
    /// Trailing silence after confirmed speech that ends the utterance
    /// ([`VadEvent::UtteranceEnd`]). Default `800`.
    pub silence_hang_ms: u32,
    /// Hard cap on a single utterance; forces [`VadEvent::UtteranceEnd`] even
    /// without a pause. Default `30_000`.
    pub max_utterance_ms: u32,
    /// If no speech is ever confirmed within this window, give up with
    /// [`VadEvent::Timeout`] (the call loop hangs up). Default `60_000`.
    pub timeout_ms: u32,
    /// When `Some`, REPLACES the sensitivity-derived absolute-minimum clamp on
    /// the speech threshold (see [`VadSession::threshold`]) with this fixed RMS
    /// value; the relative `noise_floor × factor` term is unchanged (it is
    /// scale-free). `None` keeps the raw-cpal ramp (48…120 RMS) — the default,
    /// so existing behavior is untouched.
    ///
    /// Set for processed-audio paths (Apple VPIO / Linux `module-echo-cancel`):
    /// their silence floor is near-zero and speech is AGC-levelled far below raw
    /// cpal, so the raw-cpal absolute minimum would leave the VAD deaf. A small
    /// fixed floor lets the (near-zero) relative bar dominate while still
    /// rejecting flat-line silence.
    pub abs_min_threshold: Option<f32>,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            min_speech_ms: 250,
            silence_hang_ms: 800,
            max_utterance_ms: 30_000,
            timeout_ms: 60_000,
            abs_min_threshold: None,
        }
    }
}

/// What a single [`VadSession::push`] concluded. At most one non-`None` event
/// is produced per call, and each session emits at most one terminal event
/// ([`VadEvent::UtteranceEnd`] or [`VadEvent::Timeout`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// Nothing notable — keep feeding chunks.
    None,
    /// Speech has been confirmed (crossed the threshold for `min_speech_ms`).
    SpeechStart,
    /// The utterance ended (hangover satisfied, or the length cap hit).
    UtteranceEnd,
    /// No speech at all within `timeout_ms` — give up.
    Timeout,
}

/// Internal phase of the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting for the first threshold crossing.
    Idle,
    /// Above threshold, but not yet held long enough to confirm.
    Maybe,
    /// Confirmed speech; accumulating and watching for the hangover.
    Speech,
    /// A terminal event has been emitted; further pushes are inert.
    Done,
}

/// Streaming energy VAD. Create with [`VadSession::new`], feed 16 kHz mono i16
/// chunks to [`push`](VadSession::push), and act on the returned event.
pub struct VadSession {
    config: VadConfig,
    phase: Phase,
    /// Adaptive noise floor (RMS units), a slow EMA over non-speech chunks.
    noise_floor: f32,
    /// Continuous above-threshold time in the current `Maybe` run.
    run_ms: u32,
    /// Confirmed-speech duration since [`VadEvent::SpeechStart`].
    utterance_ms: u32,
    /// Trailing sub-threshold time since the last confirmed-speech chunk.
    silence_ms: u32,
    /// Total time fed while no speech has yet been confirmed (drives timeout).
    idle_ms: u32,
}

/// Sample count per millisecond at 16 kHz.
const SAMPLES_PER_MS: u32 = 16;

/// Seed for the noise floor before any audio is observed (RMS units).
const INITIAL_NOISE_FLOOR: f32 = 50.0;

/// EMA weight applied to each non-speech chunk when adapting the floor.
const FLOOR_ADAPT_ALPHA: f32 = 0.1;

impl VadSession {
    /// Start a fresh session with the given tuning.
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            phase: Phase::Idle,
            noise_floor: INITIAL_NOISE_FLOOR,
            run_ms: 0,
            utterance_ms: 0,
            silence_ms: 0,
            idle_ms: 0,
        }
    }

    /// The adaptive noise floor (RMS units) as it stands right now — a slow EMA
    /// of the room's non-speech energy. Read by the call loop when an utterance
    /// ends: this is the user's real ambient level, measured while nothing was
    /// playing, and it seeds the barge detector's absolute floor for the reply
    /// that follows (so a quiet-phase noise level can't masquerade as a barge).
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// Speech-detection threshold in RMS units: the noise floor scaled by a
    /// sensitivity-derived factor, but never below an absolute minimum so a
    /// near-silent room still demands real speech energy. Public so the call
    /// loop can log the effective bar alongside the live noise floor.
    pub fn threshold(&self) -> f32 {
        let s = self.config.sensitivity.clamp(0.0, 1.0);
        // Absolute floor: a caller-supplied fixed override (processed-audio
        // paths), else the raw-cpal sensitivity ramp (120 … 48 RMS).
        let abs_min = self
            .config
            .abs_min_threshold
            .unwrap_or_else(|| 120.0 * (1.0 - 0.6 * s));
        (self.noise_floor * speech_threshold_factor(s)).max(abs_min)
    }

    /// Feed one chunk of 16 kHz mono i16 samples and get the resulting event.
    ///
    /// Chunks may be any length; timing is derived from the sample count. An
    /// empty chunk is a no-op.
    pub fn push(&mut self, samples: &[i16]) -> VadEvent {
        if self.phase == Phase::Done || samples.is_empty() {
            return VadEvent::None;
        }
        let chunk_ms = (samples.len() as u32) / SAMPLES_PER_MS;
        if chunk_ms == 0 {
            return VadEvent::None;
        }
        let rms = rms(samples);

        let above = rms > self.threshold();

        match self.phase {
            Phase::Idle => {
                if above {
                    self.phase = Phase::Maybe;
                    self.run_ms = chunk_ms;
                    // A run that already clears `min_speech_ms` in one chunk is
                    // confirmed immediately.
                    if self.run_ms >= self.config.min_speech_ms {
                        return self.confirm();
                    }
                } else {
                    self.adapt_floor(rms);
                    self.idle_ms = self.idle_ms.saturating_add(chunk_ms);
                    if self.idle_ms >= self.config.timeout_ms {
                        self.phase = Phase::Done;
                        return VadEvent::Timeout;
                    }
                }
                VadEvent::None
            }
            Phase::Maybe => {
                if above {
                    self.run_ms = self.run_ms.saturating_add(chunk_ms);
                    if self.run_ms >= self.config.min_speech_ms {
                        return self.confirm();
                    }
                    VadEvent::None
                } else {
                    // The run was too short — a click. Back to listening; the
                    // aborted run still counts toward the no-speech timeout.
                    self.phase = Phase::Idle;
                    self.run_ms = 0;
                    self.adapt_floor(rms);
                    self.idle_ms = self.idle_ms.saturating_add(chunk_ms);
                    if self.idle_ms >= self.config.timeout_ms {
                        self.phase = Phase::Done;
                        return VadEvent::Timeout;
                    }
                    VadEvent::None
                }
            }
            Phase::Speech => {
                self.utterance_ms = self.utterance_ms.saturating_add(chunk_ms);
                if above {
                    // Deadlock escape: energy only barely above the threshold,
                    // sustained inside an utterance, is far more likely an
                    // underestimated noise floor (e.g. post-AEC ambient after
                    // makeup gain) than speech — speech runs tens of times the
                    // threshold. Let the floor recover slowly so the threshold
                    // climbs past the ambient and trailing silence can finally
                    // count; otherwise the floor is frozen (it only adapts on
                    // sub-threshold chunks) and the utterance never ends.
                    if rms < self.threshold() * 2.0 {
                        self.adapt_floor(rms);
                    }
                    self.silence_ms = 0;
                } else {
                    self.adapt_floor(rms);
                    self.silence_ms = self.silence_ms.saturating_add(chunk_ms);
                }
                if self.silence_ms >= self.config.silence_hang_ms
                    || self.utterance_ms >= self.config.max_utterance_ms
                {
                    self.phase = Phase::Done;
                    return VadEvent::UtteranceEnd;
                }
                VadEvent::None
            }
            Phase::Done => VadEvent::None,
        }
    }

    /// Transition Maybe → Speech and emit [`VadEvent::SpeechStart`].
    fn confirm(&mut self) -> VadEvent {
        self.phase = Phase::Speech;
        self.utterance_ms = self.run_ms;
        self.silence_ms = 0;
        VadEvent::SpeechStart
    }

    /// Slow EMA of the noise floor over non-speech chunks only, so speech never
    /// pulls the floor up.
    fn adapt_floor(&mut self, rms: f32) {
        self.noise_floor =
            self.noise_floor * (1.0 - FLOOR_ADAPT_ALPHA) + rms * FLOOR_ADAPT_ALPHA;
    }
}

/// How many times over the noise floor a chunk must sit before it counts as
/// speech in the listen VAD, as a function of `sensitivity` (0.0 … 1.0). Higher
/// sensitivity → smaller factor (easier to trip): 4.0 … 1.5. Exposed so the
/// call loop can seed the barge detector's ambient floor with the *same* notion
/// of "clearly louder than the quiet room" the listen phase used.
pub fn speech_threshold_factor(sensitivity: f32) -> f32 {
    4.0 - 2.5 * sensitivity.clamp(0.0, 1.0)
}

/// Root-mean-square amplitude of a chunk, in i16 units. Shared by the shell's
/// call loop so its barge detector measures mic energy on the same scale the
/// [`VadSession`] uses internally.
pub fn rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    ((sum_sq / samples.len() as f64).sqrt()) as f32
}

// ── Barge-in detection ─────────────────────────────────────────────────────

/// Multiplier on the learned coupling term: the mic must exceed the expected
/// bleed (`coupling * ref_rms`) by this factor to count as excess energy.
const BARGE_MARGIN: f32 = 2.0;
/// EMA weight when a non-excess chunk nudges an underestimated coupling upward.
const BARGE_COUPLING_ALPHA: f32 = 0.05;
/// Multiplier applied to the peak warmup bleed to derive the absolute floor.
const BARGE_PEAK_BLEED_MULT: f32 = 1.3;

/// Reference-gated barge-in detector for call mode — the AEC-free replacement
/// for the static-noise-floor barge VAD.
///
/// Because the shell owns the TTS playback PCM, it can hand us a *live*
/// loudness reference (`ref_rms`) for what the speaker is emitting right now,
/// alongside the mic's `mic_rms`. TTS speech is highly dynamic, so a static
/// floor learned from the reply's first syllables is inevitably exceeded by
/// louder syllables later — that is what made the old detector interrupt
/// itself. Here the bar tracks the reference: the mic only counts as *excess*
/// when it exceeds the expected speaker→mic bleed (`coupling * ref_rms`) by a
/// margin, so the assistant's own voice — however loud it swells — never trips
/// it, while genuine user speech (energy with no matching reference rise) does.
///
/// Feed it per-chunk `(mic_rms, ref_rms, chunk_ms)` triples; it returns `true`
/// the moment a barge is confirmed.
pub struct BargeDetector {
    /// Remaining warmup budget (ms); while >0, pushes only learn coupling/bleed.
    warmup_remaining_ms: u32,
    /// Consecutive excess time required to confirm a barge.
    sustained_ms: u32,
    /// Learned speaker→mic coupling ratio `max(mic_rms / ref_rms)` — the peak
    /// transfer observed during warmup, then EMA-corrected upward afterward.
    coupling: f32,
    /// Peak mic bleed observed during warmup, in RMS units.
    peak_bleed: f32,
    /// The listen phase's learned ambient noise floor (RMS units) — the user's
    /// real room noise, measured while nothing played. Seeds the absolute floor
    /// so post-AEC residual / room noise at ambient level can never barge.
    ambient_floor: f32,
    /// Multiplier on `ambient_floor`: the listen VAD's speech-threshold factor
    /// times an extra margin, so a barge must be *clearly* louder than what
    /// counted as speech in the quiet phase.
    ambient_k: f32,
    /// Hard lower bound on the absolute floor (RMS units): even a near-silent
    /// reference still demands real energy before a barge is confirmed. A
    /// constructor param so processed-audio paths (system AEC, near-zero
    /// residual) can use a low floor and lean on the ambient-seeded term, while
    /// the raw-cpal fallback keeps the conservative 80.
    abs_min: f32,
    /// Consecutive excess time accumulated in the current run.
    run_ms: u32,
    /// The detection threshold used on the most recent detect-phase push — kept
    /// only so the diagnostic log can record the `(mic, ref, threshold)` triples
    /// leading up to a barge. `0.0` until the first post-warmup push.
    last_threshold: f32,
}

impl BargeDetector {
    /// Create a detector that spends `warmup_ms` learning the playback→mic
    /// coupling and bleed floor, then confirms a barge after `sustained_ms` of
    /// consecutive excess mic energy.
    ///
    /// `ambient_floor` is the listen phase's learned room-noise level and
    /// `ambient_k` the multiplier applied to it (see [`Self::abs_floor`]); pass
    /// `0.0` for either to disable ambient seeding (the pre-seeding behavior).
    /// `abs_min` is the hard lower bound on the absolute floor (80 for raw cpal;
    /// a low value for processed-audio paths whose warmup sees near-silence).
    pub fn new(
        warmup_ms: u32,
        sustained_ms: u32,
        ambient_floor: f32,
        ambient_k: f32,
        abs_min: f32,
    ) -> Self {
        Self {
            warmup_remaining_ms: warmup_ms,
            sustained_ms,
            coupling: 0.0,
            peak_bleed: 0.0,
            ambient_floor,
            ambient_k,
            abs_min,
            run_ms: 0,
            last_threshold: 0.0,
        }
    }

    /// The absolute floor (RMS units): the loudest of the warmup peak bleed
    /// scaled up, the ambient room noise scaled by `ambient_k`, and the hard
    /// minimum. Governs detection whenever there is no live reference
    /// (`ref_rms == 0`, e.g. a gap between clauses). With working AEC the warmup
    /// sees near-silence, so the peak-bleed term collapses to the hard minimum;
    /// the ambient term is what keeps the bar above the user's real room noise.
    pub fn abs_floor(&self) -> f32 {
        (self.peak_bleed * BARGE_PEAK_BLEED_MULT)
            .max(self.ambient_floor * self.ambient_k)
            .max(self.abs_min)
    }

    /// The detection threshold for a given live reference: the larger of the
    /// absolute floor (optionally scaled by `abs_mult`) and the reference-gated
    /// coupling bar (`coupling · ref_rms · margin`). `abs_mult == 1.0`
    /// reproduces the exact bar [`push`](Self::push) applies.
    ///
    /// The soft-barge verify path raises the absolute-floor term (`abs_mult`
    /// > 1) to demand energy *clearly* louder than the residual that tripped
    /// the trigger, while leaving the coupling term — which already tracks the
    /// live playback loudness — untouched.
    pub fn threshold_for(&self, ref_rms: f32, abs_mult: f32) -> f32 {
        (self.abs_floor() * abs_mult).max(self.coupling * ref_rms * BARGE_MARGIN)
    }

    /// Learned speaker→mic coupling ratio (peak `mic_rms / ref_rms`). For the
    /// diagnostic log's warmup summary.
    pub fn coupling(&self) -> f32 {
        self.coupling
    }

    /// Peak mic bleed observed during warmup (RMS units). For the log.
    pub fn peak_bleed(&self) -> f32 {
        self.peak_bleed
    }

    /// The threshold applied on the most recent detect-phase push. For the log's
    /// pre-barge `(mic, ref, threshold)` triples.
    pub fn last_threshold(&self) -> f32 {
        self.last_threshold
    }

    /// Length (ms) of the current consecutive-excess run — the sustained-run
    /// length at the moment a barge fires. For the log.
    pub fn run_ms(&self) -> u32 {
        self.run_ms
    }

    /// Clear the consecutive-excess accumulator (the sustained run) while
    /// keeping every learned quantity — coupling, peak bleed, ambient/abs
    /// floor. Used by the soft-barge verify path: when a fired barge is
    /// *refuted* (the sustained energy was echo residual, not the user — it
    /// failed to hold above the raised verify bar), the detector must require a
    /// fresh, full `sustained_ms` run before it can fire again, rather than
    /// re-confirming immediately off the tail of the run it just refuted.
    pub fn reset_run(&mut self) {
        self.run_ms = 0;
    }

    /// Whether the detector is still in its warmup window (learning, not
    /// detecting). Lets the monitor log the one-shot warmup summary exactly when
    /// warmup ends.
    pub fn is_warming_up(&self) -> bool {
        self.warmup_remaining_ms > 0
    }

    /// Feed one chunk. Returns `true` once excess mic energy has been sustained
    /// for `sustained_ms` — a confirmed barge.
    pub fn push(&mut self, mic_rms: f32, ref_rms: f32, chunk_ms: u32) -> bool {
        // ── Warmup: learn the coupling ratio and peak bleed, never detect. ──
        if self.warmup_remaining_ms > 0 {
            if ref_rms > 0.0 {
                let ratio = mic_rms / ref_rms;
                if ratio > self.coupling {
                    self.coupling = ratio;
                }
            }
            if mic_rms > self.peak_bleed {
                self.peak_bleed = mic_rms;
            }
            self.warmup_remaining_ms = self.warmup_remaining_ms.saturating_sub(chunk_ms);
            return false;
        }

        // ── Detect: the bar tracks the live reference; `ref_rms == 0` falls
        // back to the absolute floor alone (both are covered by the `max`). ──
        let threshold = self.threshold_for(ref_rms, 1.0);
        self.last_threshold = threshold;
        let excess = mic_rms > threshold;

        if excess {
            self.run_ms = self.run_ms.saturating_add(chunk_ms);
            if self.run_ms >= self.sustained_ms {
                return true;
            }
        } else {
            // Any non-excess chunk breaks the run …
            self.run_ms = 0;
            // … and, if the reference is live, lets an *underestimated*
            // coupling creep up toward the observed ratio (never down), so a
            // slightly low warmup estimate self-corrects instead of leaking a
            // steady stream of false-positive excess chunks.
            if ref_rms > 0.0 {
                let observed = mic_rms / ref_rms;
                if observed > self.coupling {
                    self.coupling += BARGE_COUPLING_ALPHA * (observed - self.coupling);
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ms` of pure silence at 16 kHz.
    fn silence(ms: u32) -> Vec<i16> {
        vec![0i16; (ms * SAMPLES_PER_MS) as usize]
    }

    /// `ms` of a square wave at `amp` (RMS == `amp`).
    fn tone(ms: u32, amp: i16) -> Vec<i16> {
        (0..(ms * SAMPLES_PER_MS))
            .map(|i| if i % 2 == 0 { amp } else { -amp })
            .collect()
    }

    /// Feed a signal in 100 ms chunks, collecting every emitted event.
    fn feed(vad: &mut VadSession, signal: &[i16]) -> Vec<VadEvent> {
        let chunk = 100 * SAMPLES_PER_MS as usize;
        signal
            .chunks(chunk)
            .map(|c| vad.push(c))
            .filter(|e| *e != VadEvent::None)
            .collect()
    }

    #[test]
    fn silence_times_out() {
        let mut vad = VadSession::new(VadConfig { timeout_ms: 2_000, ..Default::default() });
        // 1.9 s of silence: no event yet.
        assert!(feed(&mut vad, &silence(1_900)).is_empty());
        // Crossing 2 s emits exactly one Timeout.
        assert_eq!(feed(&mut vad, &silence(200)), vec![VadEvent::Timeout]);
        // Session is terminal afterwards.
        assert_eq!(vad.push(&tone(500, 8_000)), VadEvent::None);
    }

    #[test]
    fn tone_burst_starts_then_ends_after_hangover() {
        let mut vad = VadSession::new(VadConfig::default());
        let mut signal = silence(300);
        signal.extend(tone(500, 6_000)); // clearly-voiced burst
        signal.extend(silence(1_000)); // > silence_hang_ms (800)
        let events = feed(&mut vad, &signal);
        assert_eq!(events, vec![VadEvent::SpeechStart, VadEvent::UtteranceEnd]);
    }

    #[test]
    fn quiet_hum_does_not_trigger_but_speech_does() {
        let mut vad = VadSession::new(VadConfig::default());
        // 3 s of a quiet hum: learned as background, never confirmed as speech.
        let hum = feed(&mut vad, &tone(3_000, 60));
        assert!(hum.is_empty(), "quiet hum must not trigger: {hum:?}");
        // Loud speech, well above the adapted floor, does trigger.
        let mut loud = tone(500, 4_000);
        loud.extend(silence(1_000));
        let events = feed(&mut vad, &loud);
        assert_eq!(events, vec![VadEvent::SpeechStart, VadEvent::UtteranceEnd]);
    }

    #[test]
    fn max_utterance_caps_endless_speech() {
        let mut vad =
            VadSession::new(VadConfig { max_utterance_ms: 1_000, ..Default::default() });
        // Continuous loud tone, no pause: the cap forces an end.
        let events = feed(&mut vad, &tone(3_000, 6_000));
        assert_eq!(events, vec![VadEvent::SpeechStart, VadEvent::UtteranceEnd]);
    }

    #[test]
    fn short_click_is_filtered() {
        let mut vad = VadSession::new(VadConfig::default());
        // A 100 ms spike (< min_speech_ms 250) followed by quiet: no speech.
        let mut signal = tone(100, 8_000);
        signal.extend(silence(500));
        let events = feed(&mut vad, &signal);
        assert!(events.is_empty(), "click should be filtered, got {events:?}");
    }

    #[test]
    fn intra_utterance_pause_does_not_end_it() {
        let mut vad = VadSession::new(VadConfig::default());
        let mut signal = tone(500, 6_000);
        signal.extend(silence(400)); // brief gap, under the 800 ms hangover
        signal.extend(tone(500, 6_000)); // speech resumes
        signal.extend(silence(1_000)); // final pause ends it
        let events = feed(&mut vad, &signal);
        // Exactly one start and one end — the mid pause did not split it.
        assert_eq!(events, vec![VadEvent::SpeechStart, VadEvent::UtteranceEnd]);
    }

    /// `abs_min_threshold: None` (the default) preserves the raw-cpal absolute
    /// minimum clamp: after the floor adapts down to processed-style silence,
    /// conservative ~70 RMS speech still sits under the sensitivity-0.5 floor
    /// (120·(1−0.3) = 84) and never confirms — the pre-change behavior.
    #[test]
    fn abs_min_threshold_none_preserves_default_clamp() {
        let mut vad = VadSession::new(VadConfig { sensitivity: 0.5, ..Default::default() });
        // Let the noise floor decay toward zero (a processed-audio silence).
        let _ = feed(&mut vad, &silence(3_000));
        // ~70 RMS speech clears the tiny relative bar but not the 84 abs-min.
        let mut signal = tone(500, 70);
        signal.extend(silence(1_000));
        let events = feed(&mut vad, &signal);
        assert!(events.is_empty(), "70 RMS is under the default abs-min (84): {events:?}");
    }

    /// A low `abs_min_threshold` REPLACES that clamp (the AEC/processed-audio
    /// fix): with the same near-zero floor, the identical ~70 RMS speech now
    /// clears the 12 RMS override and the relative bar, and confirms — while the
    /// relative `noise_floor × factor` term is untouched.
    #[test]
    fn abs_min_threshold_override_lowers_the_bar() {
        let mut vad = VadSession::new(VadConfig {
            sensitivity: 0.5,
            abs_min_threshold: Some(12.0),
            ..Default::default()
        });
        let _ = feed(&mut vad, &silence(3_000));
        let mut signal = tone(500, 70);
        signal.extend(silence(1_000));
        let events = feed(&mut vad, &signal);
        assert_eq!(events, vec![VadEvent::SpeechStart, VadEvent::UtteranceEnd]);
    }

    // ── BargeDetector ──────────────────────────────────────────────────────
    //
    // Reference-gated barge-in. The detector is fed `(mic_rms, ref_rms)` pairs;
    // 100 ms chunks throughout. Numbers are in i16 RMS units, sized to a
    // plausible acoustic scenario: a moderately-loud TTS reference (~2000) and
    // a modest, AEC-free speaker→mic coupling (~0.15, so bleed ~300).

    /// Push `n` identical chunks and return whether a barge fired on any of them.
    fn feed_barge(d: &mut BargeDetector, mic_rms: f32, ref_rms: f32, n: u32) -> bool {
        (0..n).any(|_| d.push(mic_rms, ref_rms, 100))
    }

    /// Nothing is ever confirmed during the warmup window, however loud the mic.
    #[test]
    fn barge_no_detection_during_warmup() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        // Three 100 ms warmup chunks with a very loud mic: no barge.
        assert!(!d.push(9_000.0, 2_000.0, 100));
        assert!(!d.push(9_000.0, 2_000.0, 100));
        assert!(!d.push(9_000.0, 2_000.0, 100));
    }

    /// Warmup learns coupling as the peak `mic_rms / ref_rms` and bleed as the
    /// peak mic energy seen.
    #[test]
    fn barge_warmup_learns_peak_coupling_and_bleed() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        d.push(200.0, 2_000.0, 100); // ratio 0.10, bleed 200
        d.push(360.0, 2_000.0, 100); // ratio 0.18, bleed 360  ← the peaks
        d.push(150.0, 2_000.0, 100); // ratio 0.075
        assert!((d.coupling - 0.18).abs() < 1e-6, "coupling = peak ratio");
        assert!((d.peak_bleed - 360.0).abs() < 1e-6, "bleed = peak mic RMS");
    }

    /// The core fix: dynamic assistant speech — quiet during warmup, then
    /// swelling to 3× loudness — never trips the detector, because the mic
    /// bleed scales *with* the reference it is gated against.
    #[test]
    fn barge_dynamic_assistant_speech_never_triggers() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        // Warmup on moderate playback: coupling ≈ 0.15, bleed 300.
        feed_barge(&mut d, 300.0, 2_000.0, 3);
        // Now the assistant swells to 3× (ref 6000) with proportional bleed
        // (900 = 0.15 × 6000), and dips to valleys — sustained for seconds.
        assert!(!feed_barge(&mut d, 900.0, 6_000.0, 40), "loud proportional bleed");
        assert!(!feed_barge(&mut d, 300.0, 2_000.0, 40), "quiet proportional bleed");
        assert!(!feed_barge(&mut d, 600.0, 4_000.0, 40), "mid proportional bleed");
    }

    /// Genuine user speech — mic energy with NO matching rise in the reference —
    /// is confirmed once it sustains past `sustained_ms`.
    #[test]
    fn barge_genuine_user_speech_triggers() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3); // coupling ≈ 0.15
        // The assistant keeps talking (ref 2000, expected bleed ~300) but the
        // mic jumps to 5000 — the user talking over it — and holds it.
        // Threshold = max(390, 0.15·2000·2 = 600) = 600; 5000 ≫ 600.
        assert!(feed_barge(&mut d, 5_000.0, 2_000.0, 6), "500 ms of user speech barges");
    }

    /// A short shout (200 ms, under `sustained_ms`) is not a barge.
    #[test]
    fn barge_short_shout_does_not_trigger() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3);
        // 200 ms of loud excess, then the mic falls back to bleed: no barge.
        assert!(!feed_barge(&mut d, 5_000.0, 2_000.0, 2), "200 ms < 450 ms sustained");
        assert!(!feed_barge(&mut d, 300.0, 2_000.0, 5));
    }

    /// Consecutive excess is required: a single non-excess chunk resets the run,
    /// so two 400 ms bursts split by a quiet chunk never confirm — but a full
    /// unbroken run does.
    #[test]
    fn barge_run_resets_on_non_excess_chunk() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3);
        assert!(!feed_barge(&mut d, 5_000.0, 2_000.0, 4), "400 ms run");
        assert!(!d.push(300.0, 2_000.0, 100), "quiet chunk resets the run");
        assert!(!feed_barge(&mut d, 5_000.0, 2_000.0, 4), "another 400 ms run");
        // A 5th consecutive excess chunk (now 500 ms unbroken) finally confirms.
        assert!(d.push(5_000.0, 2_000.0, 100), "5th consecutive excess barges");
    }

    /// When the reference drops to zero (a gap between clauses / drained queue)
    /// detection falls back to the absolute floor: residual bleed at that level
    /// stays quiet, but a loud voice in the gap still barges.
    #[test]
    fn barge_ref_gap_falls_back_to_floor() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3); // bleed 300 → abs_floor = 390
        // Gap: ref 0, mic still at bleed level 300 < 390 floor → no barge.
        assert!(!feed_barge(&mut d, 300.0, 0.0, 30), "residual bleed in a gap");
        // Gap: ref 0, mic loud (user speaks into the silence) → barges.
        assert!(feed_barge(&mut d, 2_000.0, 0.0, 6), "loud voice in a gap barges");
    }

    /// The absolute floor never drops below the hard minimum, even when warmup
    /// saw almost no bleed — so faint reference noise cannot barge.
    #[test]
    fn barge_abs_floor_has_a_hard_minimum() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        // Near-silent warmup: peak bleed 10 → 10·1.3 = 13, clamped up to 80.
        feed_barge(&mut d, 10.0, 20.0, 3);
        // ref 0, mic 60 sits under the 80 floor: no barge.
        assert!(!feed_barge(&mut d, 60.0, 0.0, 30), "60 < 80 hard-min floor");
        // ref 0, mic 200 clears it and sustains: barge.
        assert!(feed_barge(&mut d, 200.0, 0.0, 6), "200 > 80 floor barges");
    }

    /// Continuous re-learning: a coupling underestimated at warmup creeps up on
    /// non-excess chunks toward the observed ratio (never down), so it
    /// self-corrects instead of leaking false positives — and never triggers.
    #[test]
    fn barge_coupling_ema_self_corrects_upward() {
        let mut d = BargeDetector::new(100, 450, 0.0, 0.0, 80.0);
        d.push(200.0, 2_000.0, 100); // warmup underestimates: coupling = 0.10
        assert!((d.coupling - 0.10).abs() < 1e-6);
        // Steady bleed at ratio 0.175 (mic 350). abs_floor = max(260, 80) = 260;
        // threshold = max(260, coupling·2000·2) starts at 400 > 350, so every
        // chunk is non-excess and nudges coupling up toward 0.175.
        assert!(!feed_barge(&mut d, 350.0, 2_000.0, 60), "underestimate must not barge");
        assert!(d.coupling > 0.10, "coupling rose toward the observed ratio");
        assert!(d.coupling < 0.175, "but only converges toward it, never overshoots");
        // A low observed ratio must NOT drag coupling back down.
        let before = d.coupling;
        d.push(20.0, 2_000.0, 100); // observed 0.01 ≪ coupling
        assert!((d.coupling - before).abs() < 1e-6, "coupling only ratchets upward");
    }

    /// The self-trigger fix: with working AEC the warmup sees near-silence, so
    /// the peak-bleed term collapses to the 80 hard-min and *any* post-AEC
    /// residual or room noise sustained past 450 ms used to barge. Seeding the
    /// floor with the listen phase's real ambient level (× K) raises the bar
    /// above that residual — so it never triggers however long it sustains —
    /// while a genuine voice, clearly louder than the quiet room, still barges.
    #[test]
    fn utterance_ends_despite_underestimated_floor() {
        // Regression: post-AEC ambient (~120 RMS after makeup gain) sat ABOVE
        // a threshold built from a stale low floor (50 × 2.0 = 100). Silence
        // never counted, the floor only adapted on sub-threshold chunks —
        // deadlock: the utterance never ended. The in-speech recovery path
        // must lift the floor and end the utterance on trailing "silence".
        let cfg = VadConfig {
            sensitivity: 0.8,
            min_speech_ms: 250,
            silence_hang_ms: 1000,
            max_utterance_ms: 30_000,
            timeout_ms: 60_000,
            abs_min_threshold: Some(12.0),
        };
        let mut vad = VadSession::new(cfg);
        let chunk = |r: i16| vec![r; 1600]; // 100ms of constant amplitude ≈ rms r
        // real speech: 600ms at rms ~3000
        let mut started = false;
        for _ in 0..6 {
            if vad.push(&chunk(3000)) == VadEvent::SpeechStart {
                started = true;
            }
        }
        assert!(started, "speech should be detected");
        // then post-speech ambient at rms ~120 — above the stale threshold
        let mut ended_after_ms = None;
        for i in 0..80 {
            if vad.push(&chunk(120)) == VadEvent::UtteranceEnd {
                ended_after_ms = Some((i + 1) * 100);
                break;
            }
        }
        let ms = ended_after_ms.expect("utterance must end despite ambient above stale threshold");
        assert!(ms <= 6000, "should end within ~6s of trailing ambient, took {ms}ms");
    }

    /// The soft-barge refute path: `reset_run` clears a partial (or complete)
    /// sustained run without touching learned state, so after a refuted trigger
    /// the detector demands a *fresh* full `sustained_ms` of excess before it
    /// can fire again — it must not re-confirm off the tail of the run it just
    /// refuted.
    #[test]
    fn barge_reset_run_requires_a_fresh_sustained_window() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3); // warmup: coupling ≈ 0.15
        // Build a 400 ms excess run — one 100 ms chunk short of the 450 ms
        // needed to fire.
        assert!(!feed_barge(&mut d, 5_000.0, 2_000.0, 4), "400 ms < 450 ms");
        // Without a reset the very next excess chunk (500 ms) would confirm.
        // Refute instead: reset the run.
        d.reset_run();
        assert!(!d.push(5_000.0, 2_000.0, 100), "post-reset: only 100 ms of a new run");
        assert!(!feed_barge(&mut d, 5_000.0, 2_000.0, 3), "up to 400 ms of the fresh run");
        assert!(d.push(5_000.0, 2_000.0, 100), "a full fresh 450+ ms run confirms");
    }

    /// `threshold_for` raises only the absolute-floor term; the coupling term
    /// (already tracking the live reference) is untouched, and `abs_mult == 1.0`
    /// reproduces the bar `push` applies. The verify path's raised bar therefore
    /// sits clearly above the trigger threshold.
    #[test]
    fn barge_threshold_for_raises_only_the_absolute_floor() {
        let mut d = BargeDetector::new(300, 450, 0.0, 0.0, 80.0);
        feed_barge(&mut d, 300.0, 2_000.0, 3); // coupling ≈ 0.15, abs_floor 390
        // No live reference: the bar is the absolute floor, and the 1.5× verify
        // multiplier raises it proportionally.
        let base = d.threshold_for(0.0, 1.0);
        assert!((base - d.abs_floor()).abs() < 1e-6, "abs_mult 1.0 == abs_floor");
        assert!((d.threshold_for(0.0, 1.5) - d.abs_floor() * 1.5).abs() < 1e-6);
        // With a loud reference the coupling bar dominates and the multiplier on
        // the (smaller) absolute-floor term leaves it unchanged.
        let coupling_bar = d.coupling() * 8_000.0 * BARGE_MARGIN; // ≫ abs_floor
        assert!((d.threshold_for(8_000.0, 1.5) - coupling_bar).abs() < 1e-3);
    }

    #[test]
    fn barge_ambient_seed_raises_the_bar() {
        // The user's quiet room measured ~220 RMS during the listen phase, at
        // sensitivity 0.5 → K = speech_threshold_factor(0.5) · 1.5 = 2.75 · 1.5
        // = 4.125, so the ambient-seeded floor is 220 · 4.125 ≈ 907.
        let ambient = 220.0;
        let k = speech_threshold_factor(0.5) * 1.5;
        let mut d = BargeDetector::new(300, 450, ambient, k, 80.0);
        // Near-silent AEC warmup: peak bleed ~30 (would give abs_floor 80).
        feed_barge(&mut d, 30.0, 0.0, 3);
        assert!(d.abs_floor() > 900.0, "ambient seed lifts the floor well above 80");

        // Post-AEC residual / room noise at ~600 RMS — comfortably above the old
        // 80 floor, and sustained for seconds — must NOT barge now.
        assert!(
            !feed_barge(&mut d, 600.0, 0.0, 50),
            "residual below ambient·K never triggers, even sustained"
        );
        // A genuine voice, clearly louder than the quiet room, still barges.
        assert!(feed_barge(&mut d, 2_000.0, 0.0, 6), "real speech clears the seeded floor");
    }
}

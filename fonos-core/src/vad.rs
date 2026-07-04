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
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            min_speech_ms: 250,
            silence_hang_ms: 800,
            max_utterance_ms: 30_000,
            timeout_ms: 60_000,
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

    /// Speech-detection threshold in RMS units: the noise floor scaled by a
    /// sensitivity-derived factor, but never below an absolute minimum so a
    /// near-silent room still demands real speech energy.
    fn threshold(&self) -> f32 {
        let s = self.config.sensitivity.clamp(0.0, 1.0);
        // Higher sensitivity → smaller factor (easier to trip): 4.0 … 1.5.
        let factor = 4.0 - 2.5 * s;
        // Absolute floor: 120 … 48 RMS across the sensitivity range.
        let abs_min = 120.0 * (1.0 - 0.6 * s);
        (self.noise_floor * factor).max(abs_min)
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
                    self.silence_ms = 0;
                } else {
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

/// Root-mean-square amplitude of a chunk, in i16 units.
fn rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    ((sum_sq / samples.len() as f64).sqrt()) as f32
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
}

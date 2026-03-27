//! Tests for audio capture correctness.
//! Covers: INV-05 (Audio capture — 16kHz, 16bit, mono, non-zero PCM)
//!
//! Hardware-dependent tests (those that open the real microphone) are gated
//! with #[cfg(not(feature = "ci"))] and should be skipped in headless CI.

// ---------------------------------------------------------------------------
// INV-05 Level 1: Static / format validation (no hardware required)
// ---------------------------------------------------------------------------

/// INV-05: A PCM buffer described as 16kHz 16-bit mono has the expected byte
/// layout.  One sample = 2 bytes (i16 little-endian).  Two seconds of audio
/// at 16 kHz = 32 000 samples = 64 000 bytes.
#[test]
fn test_pcm_buffer_format() {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BIT_DEPTH: u16 = 16;
    const DURATION_SECS: u32 = 2;

    let expected_samples = SAMPLE_RATE * DURATION_SECS;
    let bytes_per_sample = (BIT_DEPTH / 8) as u32;
    let expected_bytes = expected_samples * bytes_per_sample * (CHANNELS as u32);

    // Synthesise a mock buffer of the correct length (silent, all zeros is fine
    // for the format check — non-zero check happens in the integration test).
    let mock_buffer: Vec<u8> = vec![0u8; expected_bytes as usize];

    assert_eq!(
        mock_buffer.len(),
        64_000,
        "INV-05: 2s at 16kHz 16bit mono must be exactly 64 000 bytes"
    );

    // Verify the buffer can be interpreted as i16 samples without remainder.
    assert_eq!(
        mock_buffer.len() % 2,
        0,
        "INV-05: PCM buffer length must be even (16-bit samples)"
    );

    let samples: Vec<i16> = mock_buffer
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();

    assert_eq!(
        samples.len() as u32,
        expected_samples,
        "INV-05: sample count must equal SAMPLE_RATE * DURATION_SECS"
    );
}

/// INV-05: Validate that a 200ms chunk (the streaming chunk size) has the
/// correct byte length: 16000 * 0.2 * 2 bytes = 6 400 bytes.
#[test]
fn test_pcm_chunk_size_200ms() {
    const SAMPLE_RATE: u32 = 16_000;
    const CHUNK_DURATION_MS: u32 = 200;
    const BYTES_PER_SAMPLE: u32 = 2; // 16-bit

    let expected_chunk_bytes = SAMPLE_RATE * CHUNK_DURATION_MS / 1000 * BYTES_PER_SAMPLE;
    assert_eq!(
        expected_chunk_bytes, 6_400,
        "INV-05: 200ms chunk at 16kHz 16bit must be 6400 bytes"
    );
}

// ---------------------------------------------------------------------------
// INV-05 Level 3: Real microphone capture (CI-skippable)
// ---------------------------------------------------------------------------

/// INV-05: Open the default input device, capture 2 seconds of audio, and
/// verify that at least some samples are non-zero (i.e., the mic is live).
///
/// Skipped when the `ci` feature is enabled or no audio device is present.
/// Run manually: `cargo test test_audio_capture_produces_samples`
#[test]
#[cfg(not(feature = "ci"))]
fn test_audio_capture_produces_samples() {
    use fonos_app::audio::capture::AudioCapture;
    use std::time::Duration;

    let mut cap = AudioCapture::new().expect("INV-05: failed to open audio device");
    cap.start().expect("INV-05: failed to start capture");

    std::thread::sleep(Duration::from_secs(2));

    let level = cap.get_level();
    cap.stop();

    // Any non-zero RMS level means the mic delivered real samples.
    assert!(
        level > 0.0,
        "INV-05: capture produced zero-level audio after 2 seconds — mic may be muted or silent"
    );
}

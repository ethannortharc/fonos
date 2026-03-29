//! Audio utilities for meeting mode — stereo helpers and WAV encoding.

use crate::Result;
use crate::Error;

// ── Speaker hint ──────────────────────────────────────────────────────────

/// Identifies the likely speaker for a transcript chunk.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeakerHint {
    /// The local microphone channel — the user themselves.
    Me,
    /// The system audio channel — remote participants.
    Others,
    /// A numbered speaker from diarization.
    Speaker(u32),
}

impl SpeakerHint {
    /// Map a channel name (`"left"`, `"right"`, `"mono"`) to a [`SpeakerHint`].
    ///
    /// - `"left"` → [`SpeakerHint::Me`] (mic channel)
    /// - `"right"` → [`SpeakerHint::Others`] (system audio channel)
    /// - anything else → [`SpeakerHint::Me`] (safe default)
    pub fn from_channel(ch: &str) -> Self {
        match ch.to_lowercase().as_str() {
            "right" => SpeakerHint::Others,
            _ => SpeakerHint::Me,
        }
    }

    /// Human-readable label for this speaker hint.
    pub fn label(&self) -> &str {
        match self {
            SpeakerHint::Me => "Me",
            SpeakerHint::Others => "Others",
            SpeakerHint::Speaker(_) => "Speaker",
        }
    }
}

// ── Stereo interleave / split ─────────────────────────────────────────────

/// Interleave two mono i16 slices into a stereo (L, R) buffer.
///
/// `left` = mic channel, `right` = system audio channel.
/// Both slices must have the same length.
pub fn interleave_stereo(left: &[i16], right: &[i16]) -> Vec<i16> {
    assert_eq!(left.len(), right.len(), "stereo channels must be the same length");
    let mut out = Vec::with_capacity(left.len() * 2);
    for (l, r) in left.iter().zip(right.iter()) {
        out.push(*l);
        out.push(*r);
    }
    out
}

/// Split an interleaved stereo buffer back into `(left, right)` mono channels.
pub fn split_stereo(stereo: &[i16]) -> (Vec<i16>, Vec<i16>) {
    assert_eq!(stereo.len() % 2, 0, "interleaved buffer must have even length");
    let mut left = Vec::with_capacity(stereo.len() / 2);
    let mut right = Vec::with_capacity(stereo.len() / 2);
    for pair in stereo.chunks_exact(2) {
        left.push(pair[0]);
        right.push(pair[1]);
    }
    (left, right)
}

// ── WAV encoding ─────────────────────────────────────────────────────────

/// Encode a mono i16 PCM slice as a WAV file (44-byte header + PCM data).
pub fn build_mono_wav(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>> {
    build_wav(samples, sample_rate, 1)
}

/// Encode a stereo interleaved i16 PCM slice as a WAV file (44-byte header + PCM data).
pub fn build_stereo_wav(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>> {
    build_wav(samples, sample_rate, 2)
}

/// Internal WAV encoder.
///
/// Produces a minimal PCM WAV (format tag 1, 16-bit samples).
fn build_wav(samples: &[i16], sample_rate: u32, num_channels: u16) -> Result<Vec<u8>> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_len = (samples.len() * 2) as u32; // 2 bytes per i16
    let chunk_size = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());          // sub-chunk size
    buf.extend_from_slice(&1u16.to_le_bytes());           // PCM format
    buf.extend_from_slice(&num_channels.to_le_bytes());   // NumChannels   (bytes 22-23)
    buf.extend_from_slice(&sample_rate.to_le_bytes());    // SampleRate    (bytes 24-27)
    buf.extend_from_slice(&byte_rate.to_le_bytes());      // ByteRate
    buf.extend_from_slice(&block_align.to_le_bytes());    // BlockAlign
    buf.extend_from_slice(&bits_per_sample.to_le_bytes()); // BitsPerSample

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());

    // PCM samples (little-endian)
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }

    if buf.len() < 44 {
        return Err(Error::Config("WAV encoding produced an undersized buffer".into()));
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_wav_header_fields() {
        let samples = vec![0i16; 16_000];
        let wav = build_mono_wav(&samples, 16_000).unwrap();
        assert!(wav.len() >= 44);
        let channels = u16::from_le_bytes([wav[22], wav[23]]);
        assert_eq!(channels, 1);
        let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(rate, 16_000);
    }

    #[test]
    fn stereo_roundtrip() {
        let left: Vec<i16> = (0..100).collect();
        let right: Vec<i16> = (100..200).collect();
        let stereo = interleave_stereo(&left, &right);
        let (l2, r2) = split_stereo(&stereo);
        assert_eq!(l2, left);
        assert_eq!(r2, right);
    }
}

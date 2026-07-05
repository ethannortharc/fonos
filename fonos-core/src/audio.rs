//! Audio utilities — WAV encoding shared across STT, TTS, and voice cloning.
//!
//! This module contains pure, platform-independent audio helpers. It has no
//! dependency on `cpal`, `rodio`, or any OS audio API; those belong in the
//! platform layer where hardware access is handled.

use std::io::Write;
use std::path::Path;

use crate::{Error, Result};

/// Write 16-bit signed little-endian PCM samples to a standard WAV file.
///
/// The output is a mono, 16-bit PCM WAV file with the given `sample_rate`.
/// `pcm` must contain raw little-endian `i16` bytes (2 bytes per sample).
///
/// # Errors
///
/// Returns [`Error::Audio`] if the file cannot be created or any write fails.
pub fn write_wav(path: &Path, pcm: &[u8], sample_rate: u32) -> Result<()> {
    let data_size = pcm.len() as u32;
    let mut f = std::fs::File::create(path)
        .map_err(|e| Error::Audio(format!("create WAV '{}': {e}", path.display())))?;

    // RIFF header
    f.write_all(b"RIFF")
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&(36 + data_size).to_le_bytes())
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(b"WAVEfmt ")
        .map_err(|e| Error::Audio(e.to_string()))?;

    // fmt chunk: 16 bytes, PCM format
    f.write_all(&16u32.to_le_bytes())
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&1u16.to_le_bytes()) // PCM = 1
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&1u16.to_le_bytes()) // mono = 1 channel
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&sample_rate.to_le_bytes())
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&(sample_rate * 2).to_le_bytes()) // byte rate = sampleRate * numChannels * bitsPerSample/8
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&2u16.to_le_bytes()) // block align = numChannels * bitsPerSample/8
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&16u16.to_le_bytes()) // bits per sample
        .map_err(|e| Error::Audio(e.to_string()))?;

    // data chunk
    f.write_all(b"data")
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(&data_size.to_le_bytes())
        .map_err(|e| Error::Audio(e.to_string()))?;
    f.write_all(pcm)
        .map_err(|e| Error::Audio(e.to_string()))?;

    Ok(())
}

/// Resample mono 16-bit PCM from `from_rate` to `to_rate` by linear
/// interpolation — accurate enough for speech (TTS output → the 16 kHz the
/// platform voice helpers expect). Same-rate (or degenerate) input is returned
/// unchanged.
pub fn resample_i16(samples: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if from_rate == to_rate || from_rate == 0 || to_rate == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let out_len = ((samples.len() as u64 * to_rate as u64) / from_rate as u64) as usize;
    let step = from_rate as f64 / to_rate as f64;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * step;
        let idx = pos as usize;
        if idx >= samples.len() {
            break;
        }
        let s0 = samples[idx] as f64;
        let s1 = if idx + 1 < samples.len() { samples[idx + 1] as f64 } else { s0 };
        let frac = pos - idx as f64;
        out.push((s0 + (s1 - s0) * frac).round() as i16);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity_rate_is_passthrough() {
        let samples: Vec<i16> = vec![0, 100, -200, 3000, i16::MAX, i16::MIN];
        assert_eq!(resample_i16(&samples, 16_000, 16_000), samples);
    }

    #[test]
    fn resample_two_to_one_downsample_length_and_values() {
        // 32 kHz → 16 kHz: every output position lands exactly on an even
        // input index (frac = 0), so values are the even samples verbatim.
        let samples: Vec<i16> = vec![0, 10, 20, 30, 40, 50, 60, 70];
        let out = resample_i16(&samples, 32_000, 16_000);
        assert_eq!(out.len(), 4);
        assert_eq!(out, vec![0, 20, 40, 60]);
    }

    #[test]
    fn test_write_wav_produces_valid_header() {
        let tmp = std::env::temp_dir().join("fonos_test_audio.wav");
        // 4 samples × 2 bytes each
        let pcm: Vec<u8> = vec![0u8; 8];
        write_wav(&tmp, &pcm, 16000).expect("write_wav should succeed");

        let bytes = std::fs::read(&tmp).expect("should be able to read file");
        // Verify RIFF magic
        assert_eq!(&bytes[0..4], b"RIFF");
        // Verify WAVE type
        assert_eq!(&bytes[8..12], b"WAVE");
        // Verify fmt chunk
        assert_eq!(&bytes[12..16], b"fmt ");
        // Verify data chunk marker
        assert_eq!(&bytes[36..40], b"data");

        let _ = std::fs::remove_file(&tmp);
    }
}

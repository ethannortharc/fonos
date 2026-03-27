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

#[cfg(test)]
mod tests {
    use super::*;

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

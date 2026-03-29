//! Audio chunker — splits a continuous PCM stream into fixed-length segments.
//!
//! Each chunk covers a configurable time window (default 10–15 seconds).
//! The chunker is timer-based; VAD integration can be layered on top.

/// Configuration for the audio chunker.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// PCM sample rate in Hz (e.g. 16_000).
    pub sample_rate: u32,
    /// Preferred chunk length in seconds (target).
    pub target_chunk_secs: usize,
    /// Minimum chunk length in seconds (non-final chunks must be at least this long).
    pub min_chunk_secs: usize,
    /// Maximum chunk length in seconds (chunks are cut at this length at the latest).
    pub max_chunk_secs: usize,
}

/// A single audio chunk produced by the chunker.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// PCM samples (i16, mono) for this chunk.
    pub samples: Vec<i16>,
    /// Index of the first sample within the original input buffer.
    pub sample_offset: usize,
}

/// Split `samples` into [`AudioChunk`]s according to `config`.
///
/// All non-final chunks will be exactly `target_chunk_secs` seconds long.
/// The final chunk may be shorter than `min_chunk_secs` — it is always included
/// as long as it is non-empty.
///
/// Returns an empty `Vec` when `samples` is empty.
pub fn chunk_audio(samples: &[i16], config: &ChunkConfig) -> Vec<AudioChunk> {
    if samples.is_empty() {
        return Vec::new();
    }

    let chunk_size = (config.sample_rate as usize) * config.target_chunk_secs;
    let mut chunks = Vec::new();
    let mut offset = 0;

    while offset < samples.len() {
        let end = (offset + chunk_size).min(samples.len());
        chunks.push(AudioChunk {
            samples: samples[offset..end].to_vec(),
            sample_offset: offset,
        });
        offset = end;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> ChunkConfig {
        ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        }
    }

    #[test]
    fn empty_produces_no_chunks() {
        let chunks = chunk_audio(&[], &default_config());
        assert!(chunks.is_empty());
    }

    #[test]
    fn sample_offsets_are_correct() {
        let samples: Vec<i16> = vec![0i16; 16_000 * 30];
        let config = default_config();
        let chunks = chunk_audio(&samples, &config);

        let mut expected = 0;
        for chunk in &chunks {
            assert_eq!(chunk.sample_offset, expected);
            expected += chunk.samples.len();
        }
        assert_eq!(expected, samples.len());
    }
}

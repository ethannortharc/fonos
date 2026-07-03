//! TTS client — OpenAI-compatible `/v1/audio/speech` synthesis, plus the
//! [`TtsEngine`] port used by the listen workflow and the STS pipeline.
//!
//! Platform-independent: callers supply a resolved [`ServiceConfig`]; shells
//! own playback and file placement.

use crate::llm::ServiceConfig;

/// Receives synthesized audio as it is produced. Implemented by platform
/// shells (speaker queues) and test fakes.
pub trait PcmSink: Send + Sync {
    /// Called once, before any PCM, with the stream's format.
    fn begin(&self, sample_rate: u32, channels: u16) -> Result<(), String>;
    /// A chunk of 16-bit little-endian PCM frames.
    fn push(&self, pcm: &[u8]) -> Result<(), String>;
}

/// Synthesis port. [`HttpTts`] is the standard OpenAI-compatible client;
/// tests use fakes, and future engines (system voices) implement the same
/// trait.
#[async_trait::async_trait]
pub trait TtsEngine: Send + Sync {
    /// Synthesize `text` and return complete WAV bytes.
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String>;

    /// Stream synthesis into `sink` as audio is produced, so playback can
    /// start before generation finishes. The default falls back to whole-WAV
    /// synthesis for engines without native streaming.
    async fn synthesize_stream(&self, text: &str, sink: &dyn PcmSink) -> Result<(), String> {
        let wav = self.synthesize(text).await?;
        let parsed = crate::listen::parse_wav(&wav)?;
        let (rate, channels) = wav_fmt(parsed.fmt)?;
        sink.begin(rate, channels)?;
        sink.push(parsed.data)
    }
}

/// Decode sample-rate and channel count from a WAV `fmt ` chunk body.
fn wav_fmt(fmt: &[u8]) -> Result<(u32, u16), String> {
    if fmt.len() < 8 {
        return Err("WAV fmt chunk too short".to_string());
    }
    let channels = u16::from_le_bytes(fmt[2..4].try_into().unwrap());
    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
    Ok((rate, channels))
}

/// OpenAI-compatible `/v1/audio/speech` synthesizer (OMLX, OpenAI, …).
pub struct HttpTts {
    /// Resolved TTS connection info.
    pub service: ServiceConfig,
    /// Voice identifier as understood by the provider.
    pub voice: String,
    /// Playback speed multiplier (1.0 = normal).
    pub speed: f64,
}

#[async_trait::async_trait]
impl TtsEngine for HttpTts {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
        synthesize_wav(&self.service, text, &self.voice, self.speed).await
    }

    /// Native streaming: `stream: true` yields one WAV header followed by
    /// PCM chunks; audio reaches the sink as the first sentence finishes
    /// synthesizing, not when the whole reply does.
    async fn synthesize_stream(&self, text: &str, sink: &dyn PcmSink) -> Result<(), String> {
        use futures_util::StreamExt;

        let url = format!("{}/v1/audio/speech", self.service.base_url.trim_end_matches('/'));
        let model = if self.service.model.is_empty() {
            "f5-tts".to_string()
        } else {
            self.service.model.clone()
        };
        let body = serde_json::json!({
            "input": text,
            "voice": self.voice,
            "model": model,
            "speed": self.speed,
            "response_format": "wav",
            "stream": true,
        });
        let client = reqwest::Client::new();
        let mut req = client.post(&url).json(&body);
        if !self.service.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.service.api_key));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("speech synthesis request failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Servers without streaming support reject the flag — fall back.
            if status.is_client_error() {
                eprintln!("fonos: TTS stream=true rejected ({status}) — falling back to batch");
                let wav = self.synthesize(text).await?;
                let parsed = crate::listen::parse_wav(&wav)?;
                let (rate, channels) = wav_fmt(parsed.fmt)?;
                sink.begin(rate, channels)?;
                return sink.push(parsed.data);
            }
            return Err(format!("speech API error {status}: {body}"));
        }

        let mut stream = resp.bytes_stream();
        let mut parser = WavStreamParser::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("speech stream read failed: {e}"))?;
            for event in parser.feed(&chunk)? {
                match event {
                    WavStreamEvent::Format { sample_rate, channels } => {
                        sink.begin(sample_rate, channels)?
                    }
                    WavStreamEvent::Pcm(data) => sink.push(&data)?,
                }
            }
        }
        if !parser.started() {
            return Err("TTS stream produced no audio".to_string());
        }
        Ok(())
    }
}

/// Incremental parser for a streamed WAV: consumes the header (which may be
/// split across network chunks), then forwards frame-aligned PCM.
pub struct WavStreamParser {
    buf: Vec<u8>,
    header_done: bool,
    frame_bytes: usize,
    started: bool,
}

/// Events produced by [`WavStreamParser::feed`].
pub enum WavStreamEvent {
    /// The stream's format, parsed from the WAV header (emitted once).
    Format {
        /// Samples per second.
        sample_rate: u32,
        /// Channel count.
        channels: u16,
    },
    /// Frame-aligned 16-bit LE PCM bytes.
    Pcm(Vec<u8>),
}

impl Default for WavStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl WavStreamParser {
    /// New parser awaiting the WAV header.
    pub fn new() -> Self {
        Self { buf: Vec::new(), header_done: false, frame_bytes: 2, started: false }
    }

    /// Whether the header has been seen (any audio produced).
    pub fn started(&self) -> bool {
        self.started
    }

    /// Feed raw network bytes; returns zero or more events.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<WavStreamEvent>, String> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();

        if !self.header_done {
            // Walk chunks until the `data` chunk starts; a streaming header is
            // written in one piece, but network chunking can still split it.
            if self.buf.len() < 12 {
                return Ok(events);
            }
            if &self.buf[0..4] != b"RIFF" || &self.buf[8..12] != b"WAVE" {
                return Err("TTS stream is not a WAV".to_string());
            }
            let mut pos = 12;
            let mut fmt: Option<(u32, u16)> = None;
            let mut data_start: Option<usize> = None;
            while pos + 8 <= self.buf.len() {
                let id = &self.buf[pos..pos + 4];
                let size =
                    u32::from_le_bytes(self.buf[pos + 4..pos + 8].try_into().unwrap()) as usize;
                if id == b"data" {
                    data_start = Some(pos + 8);
                    break;
                }
                if pos + 8 + size > self.buf.len() {
                    return Ok(events); // incomplete chunk — wait for more bytes
                }
                if id == b"fmt " {
                    let body = &self.buf[pos + 8..pos + 8 + size];
                    fmt = Some(wav_fmt(body)?);
                    let bits = if size >= 16 {
                        u16::from_le_bytes(body[14..16].try_into().unwrap())
                    } else {
                        16
                    };
                    let (_, ch) = fmt.unwrap();
                    self.frame_bytes = (ch as usize) * (bits as usize / 8).max(1);
                }
                pos += 8 + size + (size & 1);
            }
            let (Some((rate, channels)), Some(start)) = (fmt, data_start) else {
                return Ok(events); // header incomplete
            };
            self.buf.drain(..start);
            self.header_done = true;
            self.started = true;
            events.push(WavStreamEvent::Format { sample_rate: rate, channels });
        }

        // Forward whole frames; keep any tail byte(s) for the next feed.
        let usable = self.buf.len() - (self.buf.len() % self.frame_bytes);
        if usable > 0 {
            let pcm: Vec<u8> = self.buf.drain(..usable).collect();
            events.push(WavStreamEvent::Pcm(pcm));
        }
        Ok(events)
    }
}

/// POST to `/v1/audio/speech` and return raw WAV bytes.
pub async fn synthesize_wav(
    tts: &ServiceConfig,
    text: &str,
    voice: &str,
    speed: f64,
) -> Result<Vec<u8>, String> {
    let url = format!("{}/v1/audio/speech", tts.base_url.trim_end_matches('/'));
    let model = if tts.model.is_empty() { "f5-tts".to_string() } else { tts.model.clone() };

    let body = serde_json::json!({
        "input": text,
        "voice": voice,
        "model": model,
        "speed": speed,
        "response_format": "wav",
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("http client error: {e}"))?;

    let mut req = client.post(&url).json(&body);
    if !tts.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", tts.api_key));
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("speech synthesis request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!("speech API error {status}: {err_body}"));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("failed to read speech response bytes: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(rate: u32, channels: u16) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&rate.to_le_bytes());
        h.extend_from_slice(&(rate * 2 * channels as u32).to_le_bytes());
        h.extend_from_slice(&(2 * channels).to_le_bytes());
        h.extend_from_slice(&16u16.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        h
    }

    #[test]
    fn parses_header_split_across_feeds_and_aligns_frames() {
        let mut p = WavStreamParser::new();
        let h = header(24000, 1);
        // header split mid-way
        assert!(p.feed(&h[..20]).unwrap().is_empty());
        let mut events = p.feed(&h[20..]).unwrap();
        // then pcm arriving with an odd split (3 bytes, then 1)
        events.extend(p.feed(&[1, 2, 3]).unwrap());
        events.extend(p.feed(&[4]).unwrap());
        let mut fmt_seen = false;
        let mut pcm: Vec<u8> = Vec::new();
        for e in events {
            match e {
                WavStreamEvent::Format { sample_rate, channels } => {
                    fmt_seen = true;
                    assert_eq!((sample_rate, channels), (24000, 1));
                }
                WavStreamEvent::Pcm(d) => pcm.extend(d),
            }
        }
        assert!(fmt_seen);
        assert_eq!(pcm, vec![1, 2, 3, 4]);
        assert!(p.started());
    }

    #[test]
    fn rejects_non_wav_streams() {
        let mut p = WavStreamParser::new();
        assert!(p.feed(b"<!doctype html>xxxxxxxx").is_err());
    }

    #[test]
    fn stereo_frame_alignment_holds_back_partial_frames() {
        let mut p = WavStreamParser::new();
        p.feed(&header(16000, 2)).unwrap();
        // 5 bytes = one full 4-byte frame + 1 leftover
        let events = p.feed(&[1, 2, 3, 4, 5]).unwrap();
        let pcm: Vec<u8> = events
            .into_iter()
            .filter_map(|e| match e {
                WavStreamEvent::Pcm(d) => Some(d),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(pcm, vec![1, 2, 3, 4]);
    }
}

//! TTS client — OpenAI-compatible `/v1/audio/speech` synthesis, plus the
//! [`TtsEngine`] port used by the listen workflow and the STS pipeline.
//!
//! Platform-independent: callers supply a resolved [`ServiceConfig`]; shells
//! own playback and file placement.

use crate::llm::ServiceConfig;

/// Synthesis port. [`HttpTts`] is the standard OpenAI-compatible client;
/// tests use fakes, and future engines (system voices, streaming) implement
/// the same trait.
#[async_trait::async_trait]
pub trait TtsEngine: Send + Sync {
    /// Synthesize `text` and return complete WAV bytes.
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, String>;
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

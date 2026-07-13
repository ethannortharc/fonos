//! STT clients — Whisper-compatible HTTP upload and chat-completions (base64
//! audio) transcription, with vocabulary biasing merged into the prompts.
//!
//! Platform-independent: callers supply the resolved [`ServiceConfig`] and the
//! audio bytes. Platform-specific engines (e.g. the Apple on-device helper)
//! live in the shells as adapters.

use crate::llm::ServiceConfig;

/// Transcribe via HTTP POST to an OpenAI-compatible /v1/audio/transcriptions endpoint.
///
/// `stt_prompt`/`stt_temperature` are the widget-level STT hints (formerly
/// carried on a legacy `modes::Mode`, deleted in Workbench P2 Task 12) — pass
/// `""`/`0.0` when the caller has none.
pub async fn transcribe_http(
    stt: &ServiceConfig,
    file_bytes: &[u8],
    model_name: &str,
    lang_code: &str,
    stt_prompt: &str,
    stt_temperature: f64,
    vocab_terms: &[String],
) -> Result<String, String> {
    let url = format!("{}/v1/audio/transcriptions", stt.base_url);
    let part = match reqwest::multipart::Part::bytes(file_bytes.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav") {
        Ok(p) => p,
        Err(e) => return Err(format!("could not build request: {e}")),
    };

    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model_name.to_string());

    // Apply the widget's own stt_prompt merged with the vocabulary glossary
    // into one Whisper prompt (budget-capped).
    let prompt = crate::vocab::build_stt_prompt(
        stt_prompt,
        vocab_terms,
        crate::vocab::STT_PROMPT_BUDGET_CHARS,
    );
    if !prompt.is_empty() {
        form = form.text("prompt", prompt);
    }
    if stt_temperature > 0.0 {
        form = form.text("temperature", stt_temperature.to_string());
    }

    if !lang_code.is_empty() {
        form = form.text("language", lang_code.to_string());
    }

    // `connect_timeout` bounds only the TCP/TLS handshake — a stale process
    // whose port isn't listening (or a firewalled drop) fails fast instead of
    // hanging the pill in "Processing" indefinitely. `timeout` bounds the
    // whole request (connect + upload + inference + response) and stays
    // generous since a real transcription of a long recording can legitimately
    // take a while — this used to be a flat 15s, which was actually the wrong
    // knob for the reported bug (connect refusal is near-instant either way)
    // and risked cutting off long legitimate transcriptions.
    let client = match reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(120))
        .build() {
        Ok(c) => c,
        Err(e) => return Err(format!("could not build HTTP client: {e}")),
    };

    let mut req = client.post(&url).multipart(form);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            Ok(json["text"].as_str().unwrap_or("").to_string())
        }
        Ok(resp) => {
            // Non-2xx (e.g. 404 when the endpoint isn't implemented, 401 bad key).
            // Surface it instead of returning an empty transcript silently.
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let hint = if status.as_u16() == 404 {
                format!(
                    " — {url} not found; this server may not implement audio transcription"
                )
            } else {
                String::new()
            };
            Err(format!(
                "{status}{hint}: {}",
                body.chars().take(200).collect::<String>()
            ))
        }
        Err(e) => Err(format!("request to {url} failed: {e}")),
    }
}

/// Transcribe audio by sending it as base64 in a chat completions request.
/// This path works with multimodal models that accept `input_audio` content
/// blocks (OpenRouter, Gemini, Voxtral, GPT-Audio, etc.).
pub async fn transcribe_chat(
    stt: &ServiceConfig,
    file_bytes: &[u8],
    lang_code: &str,
    vocab_terms: &[String],
) -> Result<String, String> {
    use base64::Engine;

    let url = {
        let base = stt.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        }
    };

    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(file_bytes);

    let lang_hint = if lang_code.is_empty() {
        String::new()
    } else {
        format!(" The audio is in language code '{}'.", lang_code)
    };
    let vocab_hint = if vocab_terms.is_empty() {
        String::new()
    } else {
        format!(" Domain vocabulary (prefer these exact spellings): {}.", vocab_terms.join(", "))
    };

    let body = serde_json::json!({
        "model": stt.model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": format!(
                        "Transcribe this audio exactly as spoken. Output only the transcript text, nothing else.{}{}",
                        lang_hint, vocab_hint
                    )
                },
                {
                    "type": "input_audio",
                    "input_audio": {
                        "data": audio_b64,
                        "format": "wav"
                    }
                }
            ]
        }],
        "temperature": 0.0,
        "max_tokens": 4096
    });

    // Same connect/total split as `transcribe_http` above.
    let client = match reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Err(format!("could not build HTTP client: {e}")),
    };

    let mut req = client.post(&url).json(&body);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let json: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string())
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!(
                "{status}: {}",
                body.chars().take(200).collect::<String>()
            ))
        }
        Err(e) => Err(format!("request to {url} failed: {e}")),
    }
}

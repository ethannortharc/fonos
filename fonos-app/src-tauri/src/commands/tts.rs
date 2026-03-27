//! Tauri commands for text-to-speech synthesis and audio playback.

use crate::audio::playback::AudioPlayback;
use fonos_core::voice_store;
use serde::Serialize;

use super::AppState;

/// Resolve voice: if it's a local cloned voice, return the absolute file path
/// to the reference audio. The OMLX server accepts file paths in the "voice" field.
fn resolve_voice(voice: &str) -> String {
    if voice == "default" || voice.is_empty() {
        return voice.to_string();
    }
    if let Some(v) = voice_store::get_voice(voice) {
        eprintln!("fonos: TTS voice '{}' → reference path: {}", v.name, v.audio_path);
        return v.audio_path;
    }
    voice.to_string()
}

#[derive(Serialize)]
pub struct TtsResult {
    pub duration_secs: f64,
    pub latency_ms: u64,
    pub size_bytes: usize,
    pub audio_path: String,
}

/// Play a WAV file from disk by path (for history replay).
#[tauri::command]
pub async fn play_audio_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let wav_data = tokio::fs::read(&path).await
        .map_err(|e| format!("failed to read {path}: {e}"))?;

    let mut guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        let playback = AudioPlayback::new()
            .map_err(|e| format!("audio init failed: {e}"))?;
        *guard = Some(playback);
    }
    guard.as_ref().unwrap().play_wav(wav_data)
        .map_err(|e| format!("playback error: {e}"))?;
    Ok(())
}

/// Generate speech AND play it in one call — avoids round-tripping binary audio through IPC.
#[tauri::command]
pub async fn generate_and_play(
    state: tauri::State<'_, AppState>,
    text: String,
    voice: String,
    speed: f64,
) -> Result<TtsResult, String> {
    let tts = super::get_service_config(&state, "tts");
    let url = format!("{}/v1/audio/speech", tts.base_url);
    let model = if tts.model.is_empty() { "f5-tts".to_string() } else { tts.model.clone() };
    let resolved = resolve_voice(&voice);
    let body = serde_json::json!({
        "input": text,
        "voice": resolved,
        "model": model,
        "speed": speed,
        "response_format": "wav",
    });

    eprintln!("fonos: TTS POST {} voice={} model={}", url, resolved, model);
    let t0 = std::time::Instant::now();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http client error: {e}"))?;

    let mut req = client.post(&url).json(&body);
    if !tts.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", tts.api_key));
    }

    let response = req.send().await
        .map_err(|e| format!("TTS request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let err = response.text().await.unwrap_or_default();

        // If voice cloning failed, retry without cloned voice
        if resolved != voice && (err.contains("Voice cache not found") || err.contains("voice")) {
            eprintln!("fonos: TTS voice clone failed, retrying with default voice: {err}");
            let fallback_body = serde_json::json!({
                "input": text, "voice": "default", "model": model,
                "speed": speed, "response_format": "wav",
            });
            let client2 = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build().map_err(|e| format!("http error: {e}"))?;
            let mut req2 = client2.post(&url).json(&fallback_body);
            if !tts.api_key.is_empty() {
                req2 = req2.header("Authorization", format!("Bearer {}", tts.api_key));
            }
            let resp2 = req2.send().await.map_err(|e| format!("TTS fallback failed: {e}"))?;
            if !resp2.status().is_success() {
                return Err(format!("TTS error (voice clone unsupported by model '{}'. Use Qwen3-TTS for cloning): {}", model, err));
            }
            let bytes = resp2.bytes().await.map_err(|e| format!("failed to read audio: {e}"))?;
            let wav_data = bytes.to_vec();
            // Continue with fallback audio below
            eprintln!("fonos: TTS fallback to default voice succeeded");

            let latency_ms = t0.elapsed().as_millis() as u64;
            let size_bytes = wav_data.len();
            let duration_secs = if wav_data.len() > 44 && &wav_data[0..4] == b"RIFF" {
                let sample_rate = u32::from_le_bytes([wav_data[24], wav_data[25], wav_data[26], wav_data[27]]) as f64;
                let bits = u16::from_le_bytes([wav_data[34], wav_data[35]]) as f64;
                let channels = u16::from_le_bytes([wav_data[22], wav_data[23]]) as f64;
                (wav_data.len() - 44) as f64 / (sample_rate * channels * bits / 8.0)
            } else { 0.0 };

            let audio_dir = std::env::temp_dir().join("fonos_audio");
            let _ = std::fs::create_dir_all(&audio_dir);
            let audio_path = audio_dir.join(format!("tts_{}.wav", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
            let audio_path_str = audio_path.to_string_lossy().to_string();
            let _ = std::fs::write(&audio_path, &wav_data);

            {
                let mut guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
                if guard.is_none() { *guard = Some(AudioPlayback::new().map_err(|e| format!("audio init: {e}"))?); }
                guard.as_ref().unwrap().play_wav(wav_data).map_err(|e| format!("playback: {e}"))?;
            }
            return Ok(TtsResult { duration_secs, latency_ms, size_bytes, audio_path: audio_path_str });
        }

        return Err(format!("TTS error {status}: {err}"));
    }

    let bytes = response.bytes().await
        .map_err(|e| format!("failed to read audio: {e}"))?;
    let wav_data = bytes.to_vec();
    let latency_ms = t0.elapsed().as_millis() as u64;
    let size_bytes = wav_data.len();

    // Calculate audio duration from WAV header
    let duration_secs = if wav_data.len() > 44 && &wav_data[0..4] == b"RIFF" {
        let sample_rate = u32::from_le_bytes([wav_data[24], wav_data[25], wav_data[26], wav_data[27]]) as f64;
        let bits = u16::from_le_bytes([wav_data[34], wav_data[35]]) as f64;
        let channels = u16::from_le_bytes([wav_data[22], wav_data[23]]) as f64;
        let data_size = (wav_data.len() - 44) as f64;
        data_size / (sample_rate * channels * bits / 8.0)
    } else {
        0.0
    };

    // Save audio to temp file for history replay
    let audio_dir = std::env::temp_dir().join("fonos_audio");
    let _ = std::fs::create_dir_all(&audio_dir);
    let audio_path = audio_dir.join(format!("tts_{}.wav", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    let audio_path_str = audio_path.to_string_lossy().to_string();
    let _ = std::fs::write(&audio_path, &wav_data);

    // Play directly — no IPC round-trip
    {
        let mut guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let playback = AudioPlayback::new()
                .map_err(|e| format!("audio init failed: {e}"))?;
            *guard = Some(playback);
        }
        guard.as_ref().unwrap().play_wav(wav_data)
            .map_err(|e| format!("playback error: {e}"))?;
    }

    Ok(TtsResult { duration_secs, latency_ms, size_bytes, audio_path: audio_path_str })
}

/// POST to `POST /v1/audio/speech` and return the raw WAV bytes.
#[tauri::command]
pub async fn synthesize_speech(
    state: tauri::State<'_, AppState>,
    text: String,
    voice: String,
    speed: f64,
) -> Result<Vec<u8>, String> {
    let tts = super::get_service_config(&state, "tts");
    let url = format!("{}/v1/audio/speech", tts.base_url);
    let model = if tts.model.is_empty() { "f5-tts".to_string() } else { tts.model.clone() };

    let resolved = resolve_voice(&voice);
    let body = serde_json::json!({
        "input": text,
        "voice": resolved,
        "model": model,
        "speed": speed,
        "response_format": "wav",
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http client error: {e}"))?;

    let mut req = client.post(&url).json(&body);
    if !tts.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", tts.api_key));
    }

    let response = req.send().await
        .map_err(|e| format!("speech synthesis request failed: {e}"))?;


    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        return Err(format!("speech API error {status}: {err_body}"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("failed to read speech response bytes: {e}"))?;

    Ok(bytes.to_vec())
}

/// Decode `audio_data` (WAV bytes) and play it through the default output device.
#[tauri::command]
pub async fn play_speech(
    state: tauri::State<'_, AppState>,
    audio_data: Vec<u8>,
) -> Result<(), String> {
    let mut guard = state.audio_playback.lock().map_err(|e| e.to_string())?;

    if guard.is_none() {
        let playback = AudioPlayback::new()
            .map_err(|e| format!("failed to init audio playback: {e}"))?;
        *guard = Some(playback);
    }

    guard
        .as_ref()
        .unwrap()
        .play_wav(audio_data)
        .map_err(|e| format!("playback error: {e}"))?;

    Ok(())
}

/// Stop playback and clear the queue immediately.
#[tauri::command]
pub fn stop_playback(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
    if let Some(playback) = guard.as_ref() {
        playback.stop();
    }
    Ok(())
}

/// Pause playback at the current position.
#[tauri::command]
pub fn pause_playback(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
    if let Some(playback) = guard.as_ref() {
        playback.pause();
    }
    Ok(())
}

/// Resume a paused playback.
#[tauri::command]
pub fn resume_playback(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
    if let Some(playback) = guard.as_ref() {
        playback.resume();
    }
    Ok(())
}

//! Voice management — local storage + TTS preview via configured provider.

use crate::audio::capture::AudioCapture;
use fonos_core::voice_store;
use super::AppState;

/// Open native file picker for audio files.
#[tauri::command]
pub async fn pick_audio_file() -> Result<Option<String>, String> {
    let result = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "flac", "m4a", "ogg", "aiff"])
            .set_title("Select Reference Audio")
            .pick_file()
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("dialog error: {e}"))?;
    Ok(result)
}

/// Record audio from mic for voice cloning.
#[tauri::command]
pub async fn record_voice_sample(
    state: tauri::State<'_, AppState>,
    duration_secs: f64,
) -> Result<String, String> {
    let dur = duration_secs.clamp(3.0, 10.0);
    let dur_ms = (dur * 1000.0) as u64;

    {
        let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let capture = AudioCapture::new()
                .map_err(|e| format!("mic init failed: {e}"))?;
            *guard = Some(capture);
        }
        guard.as_mut().unwrap().start()
            .map_err(|e| format!("mic start failed: {e}"))?;
    }

    tokio::time::sleep(std::time::Duration::from_millis(dur_ms)).await;

    let samples: Vec<i16> = {
        let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        let capture = guard.as_mut().ok_or("capture not init")?;
        capture.stop();
        let mut all = Vec::new();
        while let Some(chunk) = capture.take_chunk(200) {
            all.extend_from_slice(&chunk);
        }
        all
    };

    if samples.is_empty() {
        return Err("no audio captured".into());
    }

    // Write WAV
    let tmp = std::env::temp_dir().join(format!("fonos_voice_{}.wav", std::process::id()));
    let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fonos_core::audio::write_wav(&tmp, &pcm, 16000).map_err(|e| e.to_string())?;

    Ok(tmp.to_string_lossy().to_string())
}

/// List all saved voices (local storage).
#[tauri::command]
pub fn list_voices() -> Result<serde_json::Value, String> {
    let voices = voice_store::list_voices();
    let items: Vec<serde_json::Value> = std::iter::once(serde_json::json!({
        "voice_id": "default",
        "name": "Default",
        "status": "ready",
    }))
    .chain(voices.iter().map(|v| serde_json::json!({
        "voice_id": v.id,
        "name": v.name,
        "status": "ready",
        "audio_path": v.audio_path,
    })))
    .collect();

    Ok(serde_json::json!({ "voices": items }))
}

/// Clone a voice — save audio file locally.
#[tauri::command]
pub async fn clone_voice(
    name: String,
    audio_path: String,
) -> Result<serde_json::Value, String> {
    let audio_bytes = tokio::fs::read(&audio_path).await
        .map_err(|e| format!("failed to read '{audio_path}': {e}"))?;

    if audio_bytes.is_empty() {
        return Err("audio file is empty".into());
    }

    let voice = voice_store::save_voice(&name, &audio_bytes).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "voice_id": voice.id,
        "name": voice.name,
        "status": "ready",
    }))
}

/// Delete a saved voice.
#[tauri::command]
pub fn delete_voice(voice_id: String) -> Result<(), String> {
    voice_store::delete_voice(&voice_id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Preview a voice — play back the saved recording.
#[tauri::command]
pub async fn preview_voice(
    state: tauri::State<'_, AppState>,
    voice_id: String,
    _text: String,
) -> Result<(), String> {
    if voice_id == "default" {
        return Err("Default voice has no recording to preview".into());
    }

    let voice = voice_store::get_voice(&voice_id)
        .ok_or_else(|| format!("Voice '{}' not found", voice_id))?;

    let wav_data = std::fs::read(&voice.audio_path)
        .map_err(|e| format!("Failed to read voice file: {e}"))?;

    // Play through audio output
    let mut guard = state.audio_playback.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        let playback = crate::audio::playback::AudioPlayback::new()
            .map_err(|e| format!("audio init failed: {e}"))?;
        *guard = Some(playback);
    }
    guard.as_ref().unwrap().play_wav(wav_data)
        .map_err(|e| format!("playback error: {e}"))?;

    Ok(())
}


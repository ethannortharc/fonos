//! Voice store — local voice library management.
//!
//! Saves cloned voice recordings to the app's data directory.
//! No server dependency — voices are managed entirely on-device.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Error, Result};

/// A locally stored voice entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    /// Unique identifier for the voice (e.g. "voice_abc123").
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Absolute path to the stored WAV file.
    pub audio_path: String,
    /// ISO 8601 timestamp when the voice was saved.
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VoiceDb {
    voices: Vec<Voice>,
}

/// Returns the directory used to store voice audio files.
fn voices_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.fonos.app")
        .join("voices");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Returns the path to the voice metadata JSON file.
fn db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.fonos.app")
        .join("voices.json")
}

fn load_db() -> VoiceDb {
    match std::fs::read_to_string(db_path()) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => VoiceDb::default(),
    }
}

fn save_db(db: &VoiceDb) -> Result<()> {
    let json = serde_json::to_string_pretty(db)
        .map_err(|e| Error::Voice(format!("failed to serialise voice db: {e}")))?;
    std::fs::write(db_path(), json)
        .map_err(|e| Error::Voice(format!("failed to save voice db: {e}")))
}

/// List all saved voices from local storage.
pub fn list_voices() -> Vec<Voice> {
    load_db().voices
}

/// Save a new voice recording to disk and register it in the voice database.
/// Returns the newly created [`Voice`] entry.
pub fn save_voice(name: &str, audio_bytes: &[u8]) -> Result<Voice> {
    let id = format!("voice_{}", uuid_short());
    let filename = format!("{}.wav", id);
    let path = voices_dir().join(&filename);

    std::fs::write(&path, audio_bytes)
        .map_err(|e| Error::Voice(format!("failed to write voice file: {e}")))?;

    let voice = Voice {
        id,
        name: name.to_string(),
        audio_path: path.to_string_lossy().to_string(),
        created_at: chrono_now(),
    };

    let mut db = load_db();
    db.voices.push(voice.clone());
    save_db(&db)?;

    Ok(voice)
}

/// Retrieve a single voice by its ID. Returns `None` if not found.
pub fn get_voice(id: &str) -> Option<Voice> {
    load_db().voices.into_iter().find(|v| v.id == id)
}

/// Delete a voice entry and its audio file. Returns `true` if an entry was removed.
pub fn delete_voice(id: &str) -> Result<bool> {
    let mut db = load_db();
    let before = db.voices.len();

    // Remove the audio file if it exists
    if let Some(voice) = db.voices.iter().find(|v| v.id == id) {
        let _ = std::fs::remove_file(&voice.audio_path);
    }

    db.voices.retain(|v| v.id != id);
    let removed = db.voices.len() < before;
    save_db(&db)?;
    Ok(removed)
}

// ─── Helpers ─────────────────────────────────────────────

/// Generate a short unique identifier based on the current time.
fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}{:04x}", t.as_secs(), t.subsec_millis())
}

/// Returns the current UTC time as an ISO 8601 string (YYYY-MM-DDTHH:MM:SS).
fn chrono_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, m, s)
}

fn days_to_ymd(mut days: i64) -> (i64, i64, i64) {
    // Algorithm from Howard Hinnant
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

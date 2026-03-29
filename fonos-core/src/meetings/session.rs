//! Meeting session management — container creation, entry insertion, and duration tracking.

use rusqlite::Connection;
use serde_json::json;

use crate::{Error, Result};
use crate::meetings::audio::SpeakerHint;
use crate::storage::{Container, ContainerType, Entry, EntryRole, SourceType, insert_container, insert_entry};

/// Create a new meeting session container and return its row ID.
///
/// # Arguments
/// * `conn` — open SQLite connection with storage tables initialised
/// * `started_at` — ISO 8601 timestamp when the meeting started (used in the title)
/// * `audio_source` — e.g. `"mic_only"`, `"dual_channel"`, `"screencapturekit"`
pub fn create_meeting_session(
    conn: &Connection,
    started_at: &str,
    audio_source: &str,
) -> Result<i64> {
    // Derive a human-friendly title from the timestamp.
    let title = format!("Meeting {}", started_at.replace('T', " "));

    // Derive channel_mode from audio_source.
    let channel_mode = if audio_source.contains("dual") {
        "dual"
    } else {
        "mono"
    };

    let now = now_iso8601();
    let container = Container {
        id: None,
        container_type: ContainerType::MeetingSession,
        title,
        parent_id: None,
        created_at: now.clone(),
        updated_at: now,
        metadata: json!({
            "audio_source": audio_source,
            "channel_mode": channel_mode,
            "duration_total_ms": 0,
            "summary_generated": false,
        }),
    };

    insert_container(conn, &container)
}

/// Update the `duration_total_ms` metadata field and `updated_at` timestamp of a meeting
/// session container.
pub fn update_meeting_duration(
    conn: &Connection,
    container_id: i64,
    duration_ms: u64,
) -> Result<()> {
    // Read existing metadata so we can merge rather than overwrite.
    let metadata_json: String = conn
        .query_row(
            "SELECT metadata FROM containers WHERE id = ?1",
            rusqlite::params![container_id],
            |row| row.get(0),
        )
        .map_err(|e| Error::Database(format!("update_meeting_duration fetch: {e}")))?;

    let mut metadata: serde_json::Value =
        serde_json::from_str(&metadata_json)
            .map_err(|e| Error::Database(format!("update_meeting_duration parse: {e}")))?;

    metadata["duration_total_ms"] = json!(duration_ms);

    let new_metadata = serde_json::to_string(&metadata)
        .map_err(|e| Error::Database(format!("update_meeting_duration serialise: {e}")))?;

    conn.execute(
        "UPDATE containers SET metadata = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![container_id, new_metadata, now_iso8601()],
    )
    .map_err(|e| Error::Database(format!("update_meeting_duration: {e}")))?;

    Ok(())
}

/// Insert a transcript chunk as an entry in a meeting session.
///
/// # Arguments
/// * `conn` — open SQLite connection
/// * `session_id` — container row ID of the meeting session
/// * `text` — raw STT transcript for this chunk
/// * `chunk_index` — zero-based chunk sequence number within this session
/// * `timestamp_in_session_ms` — offset from session start in milliseconds
/// * `duration_ms` — duration of this chunk in milliseconds
/// * `speaker_hint` — optional speaker hint (mic/system channel)
pub fn insert_meeting_entry(
    conn: &Connection,
    session_id: i64,
    text: &str,
    chunk_index: u32,
    timestamp_in_session_ms: u64,
    duration_ms: u64,
    speaker_hint: Option<SpeakerHint>,
) -> Result<i64> {
    let speaker_label = speaker_hint.as_ref().map(|h| h.label().to_string());

    let mut meta = json!({
        "chunk_index": chunk_index,
        "timestamp_in_session_ms": timestamp_in_session_ms,
        "duration_ms": duration_ms,
    });

    if let Some(label) = &speaker_label {
        meta["speaker_hint"] = json!(label);
    }

    let entry = Entry {
        id: None,
        created_at: now_iso8601(),
        source_type: SourceType::Meeting,
        role: EntryRole::User,
        mode: "meeting".into(),
        raw_text: text.into(),
        processed_text: Some(text.into()),
        container_id: Some(session_id),
        audio_ref: None,
        metadata: meta,
    };

    insert_entry(conn, &entry)
}

/// Insert an AI-generated summary as a `system`-role entry in the meeting session container.
///
/// Returns the row ID of the inserted entry.
pub fn insert_summary_entry(
    conn: &Connection,
    session_id: i64,
    summary: &str,
) -> Result<i64> {
    let entry = Entry {
        id: None,
        created_at: now_iso8601(),
        source_type: SourceType::Meeting,
        role: EntryRole::System,
        mode: "meeting_summary".into(),
        raw_text: summary.into(),
        processed_text: Some(summary.into()),
        container_id: Some(session_id),
        audio_ref: None,
        metadata: json!({
            "type": "summary",
        }),
    };

    insert_entry(conn, &entry)
}

// ── Internal helpers ──────────────────────────────────────────────────────

/// Return the current UTC time as an ISO 8601-like string with millisecond precision.
///
/// Format: `<unix_ms>` — sufficient for storage ordering and change detection.
/// For human-readable dates, use chrono in a higher-level layer.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // Include milliseconds so that timestamps are unique even within the same second.
    let ms = dur.as_millis();
    // Store as a sortable numeric string that looks like a timestamp.
    format!("{ms}")
}

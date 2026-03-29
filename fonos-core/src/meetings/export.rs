//! Meeting export — Markdown and JSON serialization for meeting sessions.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::{Error, Result};
use crate::storage::get_container_entries;

// ── Markdown export ───────────────────────────────────────────────────────

/// Export a meeting session as a Markdown string.
///
/// The output contains:
/// 1. Session title and metadata header.
/// 2. AI summary section (if a `system`-role entry exists).
/// 3. Full transcript with speaker labels and timestamps.
///
/// Returns `Err` if `session_id` does not exist or cannot be read.
pub fn export_meeting_markdown(conn: &Connection, session_id: i64) -> Result<String> {
    // Fetch the container.
    let container = conn
        .query_row(
            "SELECT title, created_at, metadata FROM containers WHERE id = ?1",
            rusqlite::params![session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|_| Error::Database(format!("meeting session {session_id} not found")))?;

    let (title, created_at, metadata_json) = container;
    let metadata: Value = serde_json::from_str(&metadata_json).unwrap_or_default();
    let duration_ms = metadata["duration_total_ms"].as_u64().unwrap_or(0);

    let entries = get_container_entries(conn, session_id)?;
    if entries.is_empty() {
        return Err(Error::Database(format!(
            "meeting session {session_id} has no entries"
        )));
    }

    let mut md = String::new();

    // Header
    md.push_str(&format!("# {title}\n\n"));
    md.push_str(&format!("**Date:** {created_at}  \n"));
    if duration_ms > 0 {
        md.push_str(&format!(
            "**Duration:** {}  \n",
            format_duration_ms(duration_ms)
        ));
    }
    md.push('\n');

    // Separate summary entries from transcript entries.
    let mut summary_text: Option<String> = None;
    let mut transcript_entries: Vec<_> = Vec::new();

    for entry in &entries {
        if entry.role == crate::storage::EntryRole::System {
            summary_text = Some(entry.raw_text.clone());
        } else {
            transcript_entries.push(entry);
        }
    }

    // AI Summary section
    if let Some(summary) = summary_text {
        md.push_str("---\n\n");
        md.push_str(&summary);
        md.push_str("\n\n");
    }

    // Transcript section
    md.push_str("---\n\n## Transcript\n\n");

    for entry in &transcript_entries {
        let meta: Value = serde_json::from_str(
            &serde_json::to_string(&entry.metadata).unwrap_or_else(|_| "{}".into()),
        )
        .unwrap_or_default();

        let ts_ms = meta["timestamp_in_session_ms"].as_u64().unwrap_or(0);
        let ts_str = format_timestamp_ms(ts_ms);
        let speaker = meta["speaker_hint"].as_str().unwrap_or("Speaker");

        md.push_str(&format!(
            "**[{ts_str}] {speaker}**: {}\n\n",
            entry.raw_text
        ));
    }

    Ok(md)
}

// ── JSON export ───────────────────────────────────────────────────────────

/// Export a meeting session as a JSON string.
///
/// The root object contains:
/// - `"session_id"`: integer
/// - `"title"`: string
/// - `"created_at"`: string
/// - `"metadata"`: object
/// - `"transcript"`: array of transcript entry objects
/// - `"summary"`: string or null
///
/// Returns `Err` if `session_id` does not exist or cannot be read.
pub fn export_meeting_json(conn: &Connection, session_id: i64) -> Result<String> {
    let container = conn
        .query_row(
            "SELECT title, created_at, metadata FROM containers WHERE id = ?1",
            rusqlite::params![session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|_| Error::Database(format!("meeting session {session_id} not found")))?;

    let (title, created_at, metadata_json) = container;
    let metadata: Value = serde_json::from_str(&metadata_json).unwrap_or_default();

    let entries = get_container_entries(conn, session_id)?;
    if entries.is_empty() {
        return Err(Error::Database(format!(
            "meeting session {session_id} has no entries"
        )));
    }

    let mut transcript_items: Vec<Value> = Vec::new();
    let mut summary_text: Option<String> = None;

    for entry in &entries {
        if entry.role == crate::storage::EntryRole::System {
            summary_text = Some(entry.raw_text.clone());
            continue;
        }

        let meta: Value = serde_json::from_str(
            &serde_json::to_string(&entry.metadata).unwrap_or_else(|_| "{}".into()),
        )
        .unwrap_or_default();

        let ts_ms = meta["timestamp_in_session_ms"].as_u64().unwrap_or(0);
        let speaker = meta["speaker_hint"]
            .as_str()
            .unwrap_or("Speaker")
            .to_string();

        transcript_items.push(json!({
            "speaker": speaker,
            "timestamp_ms": ts_ms,
            "text": entry.raw_text,
            "chunk_index": meta["chunk_index"].as_u64().unwrap_or(0),
            "duration_ms": meta["duration_ms"].as_u64().unwrap_or(0),
        }));
    }

    let output = json!({
        "session_id": session_id,
        "title": title,
        "created_at": created_at,
        "metadata": metadata,
        "transcript": transcript_items,
        "summary": summary_text,
    });

    serde_json::to_string_pretty(&output)
        .map_err(|e| Error::Database(format!("export_meeting_json serialise: {e}")))
}

// ── Formatting helpers ────────────────────────────────────────────────────

fn format_timestamp_ms(ms: u64) -> String {
    let total_secs = ms / 1_000;
    let h = total_secs / 3_600;
    let m = (total_secs % 3_600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1_000;
    let h = total_secs / 3_600;
    let m = (total_secs % 3_600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else {
        format!("{m}m {s}s")
    }
}

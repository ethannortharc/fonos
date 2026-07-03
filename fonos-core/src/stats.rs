//! Stats & History — SQLite-backed event tracking for Fonos.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Error, Result};

// ─── Public Types ────────────────────────────────────────

/// A single recorded event (STT, TTS, or LLM interaction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Row ID in the database.
    pub id: i64,
    /// Event type: "stt", "tts", or "llm".
    #[serde(rename = "type")]
    pub event_type: String,
    /// ISO 8601 timestamp when the event was recorded.
    pub created_at: String,
    /// Date portion only (YYYY-MM-DD) for daily aggregation.
    pub date: String,
    /// Input text (transcript for STT, prompt for LLM).
    pub input_text: String,
    /// Output text (result from TTS/LLM).
    pub output_text: String,
    /// Word count of input_text.
    pub words_in: i64,
    /// Word count of output_text.
    pub words_out: i64,
    /// Duration of audio in seconds (STT/TTS).
    pub duration_secs: f64,
    /// Round-trip latency in milliseconds (LLM).
    pub latency_ms: i64,
    /// Dictation mode ("raw", "fix", etc.).
    pub mode: String,
    /// Model identifier used.
    pub model: String,
    /// Voice identifier used.
    pub voice: String,
    /// Path to generated audio file.
    pub audio_path: String,
    /// Tokens consumed from the input.
    pub tokens_in: i64,
    /// Tokens produced in the output.
    pub tokens_out: i64,
    /// Session ID linking related events from a single hotkey press.
    pub session_id: String,
}

/// Daily aggregated statistics for a single calendar date.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStat {
    /// Calendar date (YYYY-MM-DD).
    pub date: String,
    /// Number of STT events.
    pub stt_count: i64,
    /// Total audio seconds processed by STT.
    pub stt_seconds: f64,
    /// Total words transcribed.
    pub stt_words: i64,
    /// Number of TTS events.
    pub tts_count: i64,
    /// Total words synthesised.
    pub tts_words: i64,
    /// Number of LLM events.
    pub llm_count: i64,
    /// Sum of all LLM latencies in milliseconds.
    pub llm_latency_total: i64,
    /// Total tokens (in + out) consumed.
    pub tokens_total: i64,
    /// Estimated time saved in seconds.
    pub time_saved_secs: f64,
}

/// Summary of activity for today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodaySummary {
    /// Estimated time saved in seconds.
    pub time_saved_secs: f64,
    /// Combined word count across all events.
    pub total_words: i64,
    /// Total event count (proxy for sessions).
    pub total_sessions: i64,
    /// Number of STT events today.
    pub stt_count: i64,
    /// Words transcribed today.
    pub stt_words: i64,
    /// Audio seconds processed today.
    pub stt_seconds: f64,
    /// Number of TTS events today.
    pub tts_count: i64,
    /// Words synthesised today.
    pub tts_words: i64,
    /// Number of LLM events today.
    pub llm_count: i64,
    /// Average LLM latency in milliseconds.
    pub llm_latency_avg: i64,
    /// Total tokens (in + out) consumed today.
    pub tokens_total: i64,
}

// ─── Database Init ───────────────────────────────────────

/// Returns the path to the SQLite database file.
pub fn db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Library/Application Support"))
        .join("com.fonos.app")
        .join("fonos.db")
}

/// Initialize the database: create table and indexes if not present.
/// Idempotent — safe to call on every app start.
pub fn init_db(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            type         TEXT    NOT NULL,
            created_at   TEXT    NOT NULL,
            date         TEXT    NOT NULL,
            input_text   TEXT,
            output_text  TEXT,
            words_in     INTEGER NOT NULL DEFAULT 0,
            words_out    INTEGER NOT NULL DEFAULT 0,
            duration_secs REAL   NOT NULL DEFAULT 0,
            latency_ms   INTEGER NOT NULL DEFAULT 0,
            mode         TEXT,
            model        TEXT,
            voice        TEXT,
            audio_path   TEXT,
            tokens_in    INTEGER NOT NULL DEFAULT 0,
            tokens_out   INTEGER NOT NULL DEFAULT 0,
            session_id   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_events_date ON events(date);
        CREATE INDEX IF NOT EXISTS idx_events_type ON events(type);",
    )
    .expect("stats: init_db failed");

    // Migration: add columns if upgrading from older schema
    for stmt in &[
        "ALTER TABLE events ADD COLUMN tokens_in INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE events ADD COLUMN tokens_out INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE events ADD COLUMN session_id TEXT",
    ] {
        let _ = conn.execute_batch(stmt); // ignore "duplicate column" errors
    }
    let _ = conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id)");
}

// ─── Word Counting ───────────────────────────────────────

/// Count words in text. CJK text counts characters; Latin text splits on whitespace.
pub fn count_words(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let has_cjk = text.chars().any(|c| {
        matches!(c as u32,
            0x4E00..=0x9FFF   // CJK Unified Ideographs
            | 0x3400..=0x4DBF // Extension A
            | 0x20000..=0x2A6DF // Extension B
            | 0xF900..=0xFAFF // Compatibility
            | 0x3040..=0x309F // Hiragana
            | 0x30A0..=0x30FF // Katakana
        )
    });
    if has_cjk {
        text.chars().filter(|c| !c.is_whitespace()).count()
    } else {
        text.split_whitespace().count()
    }
}

// ─── CRUD Operations ─────────────────────────────────────

/// Per-model dictation latency summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyModelStat {
    /// STT backend/model identifier as recorded on the event.
    pub model: String,
    /// Number of dictations measured for this model.
    pub count: i64,
    /// Median end-to-end latency (nearest-rank).
    pub p50_ms: i64,
    /// 95th-percentile end-to-end latency (nearest-rank).
    pub p95_ms: i64,
}

/// End-to-end dictation latency percentiles over a date window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Number of dictations measured in the window.
    pub count: i64,
    /// Median end-to-end latency (nearest-rank).
    pub p50_ms: i64,
    /// 95th-percentile end-to-end latency (nearest-rank).
    pub p95_ms: i64,
    /// Mean end-to-end latency.
    pub avg_ms: i64,
    /// Fastest dictation in the window.
    pub min_ms: i64,
    /// Slowest dictation in the window.
    pub max_ms: i64,
    /// Per-STT-backend breakdown, most-used first (capped at 6).
    pub by_model: Vec<LatencyModelStat>,
}

/// Nearest-rank percentile over an ascending-sorted slice.
fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Record one end-to-end dictation (key release → text delivered) latency.
/// Stored as an `events` row with type `dictation`; excluded from session
/// counts so it never double-counts the underlying stt/llm events.
pub fn record_dictation_latency(
    conn: &Connection,
    latency_ms: i64,
    mode: &str,
    stt_model: &str,
) -> Result<i64> {
    record_event(conn, "dictation", "", "", 0.0, latency_ms, mode, stt_model, "", "", 0, 0, "")
}

/// P50/P95 (nearest-rank) of end-to-end dictation latency in [date_from, date_to].
pub fn get_dictation_latency(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<LatencyStats> {
    let mut stmt = conn
        .prepare(
            "SELECT latency_ms, COALESCE(model, '')
             FROM events
             WHERE type = 'dictation' AND latency_ms > 0
               AND date >= ?1 AND date <= ?2",
        )
        .map_err(|e| Error::Database(format!("get_dictation_latency: {e}")))?;

    let rows = stmt
        .query_map(params![date_from, date_to], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| Error::Database(format!("get_dictation_latency query: {e}")))?;

    let mut all: Vec<i64> = Vec::new();
    let mut per_model: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
    for row in rows {
        let (ms, model) = row.map_err(|e| Error::Database(format!("get_dictation_latency row: {e}")))?;
        all.push(ms);
        per_model.entry(model).or_default().push(ms);
    }
    all.sort_unstable();

    let mut by_model: Vec<LatencyModelStat> = per_model
        .into_iter()
        .map(|(model, mut v)| {
            v.sort_unstable();
            LatencyModelStat {
                model,
                count: v.len() as i64,
                p50_ms: percentile(&v, 50.0),
                p95_ms: percentile(&v, 95.0),
            }
        })
        .collect();
    by_model.sort_by(|a, b| b.count.cmp(&a.count).then(a.model.cmp(&b.model)));
    by_model.truncate(6);

    let count = all.len() as i64;
    Ok(LatencyStats {
        count,
        p50_ms: percentile(&all, 50.0),
        p95_ms: percentile(&all, 95.0),
        avg_ms: if count > 0 { all.iter().sum::<i64>() / count } else { 0 },
        min_ms: all.first().copied().unwrap_or(0),
        max_ms: all.last().copied().unwrap_or(0),
        by_model,
    })
}

/// Insert one usage event row (stt / tts / llm / dictation).
pub fn record_event(
    conn: &Connection,
    event_type: &str,
    input_text: &str,
    output_text: &str,
    duration_secs: f64,
    latency_ms: i64,
    mode: &str,
    model: &str,
    voice: &str,
    audio_path: &str,
    tokens_in: i64,
    tokens_out: i64,
    session_id: &str,
) -> Result<i64> {
    let now = chrono_now();
    let date = &now[..10]; // YYYY-MM-DD
    let words_in = count_words(input_text) as i64;
    let words_out = count_words(output_text) as i64;
    let sid: Option<&str> = if session_id.is_empty() { None } else { Some(session_id) };

    conn.execute(
        "INSERT INTO events
           (type, created_at, date, input_text, output_text,
            words_in, words_out, duration_secs, latency_ms,
            mode, model, voice, audio_path, tokens_in, tokens_out, session_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            event_type, now, date, input_text, output_text,
            words_in, words_out, duration_secs, latency_ms,
            mode, model, voice, audio_path, tokens_in, tokens_out, sid,
        ],
    )
    .map_err(|e| Error::Database(format!("record_event: {e}")))?;

    Ok(conn.last_insert_rowid())
}

/// Tag recent events (last N rows) with a session ID.
/// Used by the hotkey handler to link STT + LLM events from a single press.
pub fn tag_session(conn: &Connection, session_id: &str, seconds_back: i64) -> Result<usize> {
    let affected = conn.execute(
        "UPDATE events SET session_id = ?1
         WHERE session_id IS NULL
         AND id IN (SELECT id FROM events ORDER BY id DESC LIMIT ?2)",
        params![session_id, seconds_back],
    )
    .map_err(|e| Error::Database(format!("tag_session: {e}")))?;
    Ok(affected)
}

/// Generate a simple session ID based on timestamp and random suffix.
pub fn new_session_id() -> String {
    use std::time::SystemTime;
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("s-{:x}", ts)
}

/// Delete a single event by ID. No-op if ID does not exist.
pub fn delete_event(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM events WHERE id = ?1", params![id])
        .map_err(|e| Error::Database(format!("delete_event: {e}")))?;
    Ok(())
}

/// Get daily statistics for a date range (inclusive).
pub fn get_daily_stats(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<DailyStat>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                date,
                SUM(CASE WHEN type='stt' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='stt' THEN duration_secs ELSE 0 END),
                SUM(CASE WHEN type='stt' THEN words_in ELSE 0 END),
                SUM(CASE WHEN type='tts' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='tts' THEN words_out ELSE 0 END),
                SUM(CASE WHEN type='llm' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='llm' THEN latency_ms ELSE 0 END),
                SUM(tokens_in + tokens_out)
             FROM events
             WHERE date >= ?1 AND date <= ?2
             GROUP BY date
             ORDER BY date",
        )
        .map_err(|e| Error::Database(format!("get_daily_stats: {e}")))?;

    let rows = stmt
        .query_map(params![date_from, date_to], |row| {
            let stt_words: i64 = row.get(3)?;
            Ok(DailyStat {
                date: row.get(0)?,
                stt_count: row.get(1)?,
                stt_seconds: row.get(2)?,
                stt_words,
                tts_count: row.get(4)?,
                tts_words: row.get(5)?,
                llm_count: row.get(6)?,
                llm_latency_total: row.get(7)?,
                tokens_total: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
                time_saved_secs: (stt_words as f64) * 1.1,
            })
        })
        .map_err(|e| Error::Database(format!("get_daily_stats query: {e}")))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Database(format!("get_daily_stats row: {e}")))?);
    }
    Ok(result)
}

/// Get paginated events, optionally filtered by type. Newest first.
pub fn get_history(
    conn: &Connection,
    limit: i64,
    offset: i64,
    type_filter: &str,
) -> Result<Vec<Event>> {
    let cols = "id, type, created_at, date, input_text, output_text,
                    words_in, words_out, duration_secs, latency_ms,
                    mode, model, voice, audio_path, tokens_in, tokens_out, session_id";
    let (sql, has_filter) = if type_filter.is_empty() || type_filter == "all" {
        (format!("SELECT {} FROM events ORDER BY created_at DESC LIMIT ?1 OFFSET ?2", cols), false)
    } else {
        (format!("SELECT {} FROM events WHERE type = ?3 ORDER BY created_at DESC LIMIT ?1 OFFSET ?2", cols), true)
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Database(format!("get_history: {e}")))?;

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<Event> {
        Ok(Event {
            id: row.get(0)?,
            event_type: row.get(1)?,
            created_at: row.get(2)?,
            date: row.get(3)?,
            input_text: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            output_text: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            words_in: row.get(6)?,
            words_out: row.get(7)?,
            duration_secs: row.get(8)?,
            latency_ms: row.get(9)?,
            mode: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
            model: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
            voice: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
            audio_path: row.get::<_, Option<String>>(13)?.unwrap_or_default(),
            tokens_in: row.get::<_, Option<i64>>(14)?.unwrap_or(0),
            tokens_out: row.get::<_, Option<i64>>(15)?.unwrap_or(0),
            session_id: row.get::<_, Option<String>>(16)?.unwrap_or_default(),
        })
    };

    let rows = if has_filter {
        stmt.query_map(params![limit, offset, type_filter], map_row)
    } else {
        stmt.query_map(params![limit, offset], map_row)
    }
    .map_err(|e| Error::Database(format!("get_history query: {e}")))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Database(format!("get_history row: {e}")))?);
    }
    Ok(result)
}

/// Get today's aggregated summary.
pub fn get_today(conn: &Connection) -> Result<TodaySummary> {
    let today = &chrono_now()[..10];

    let mut stmt = conn
        .prepare(
            "SELECT
                SUM(CASE WHEN type='stt' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='stt' THEN words_in ELSE 0 END),
                SUM(CASE WHEN type='stt' THEN duration_secs ELSE 0 END),
                SUM(CASE WHEN type='tts' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='tts' THEN words_out ELSE 0 END),
                SUM(CASE WHEN type='llm' THEN 1 ELSE 0 END),
                SUM(CASE WHEN type='llm' THEN latency_ms ELSE 0 END),
                SUM(CASE WHEN type IN ('stt','tts','llm') THEN 1 ELSE 0 END),
                SUM(tokens_in + tokens_out)
             FROM events WHERE date = ?1",
        )
        .map_err(|e| Error::Database(format!("get_today: {e}")))?;

    let result = stmt
        .query_row(params![today], |row| {
            let stt_count: i64 = row.get::<_, Option<i64>>(0)?.unwrap_or(0);
            let stt_words: i64 = row.get::<_, Option<i64>>(1)?.unwrap_or(0);
            let stt_seconds: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
            let tts_count: i64 = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
            let tts_words: i64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0);
            let llm_count: i64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0);
            let llm_latency_total: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
            let total_sessions: i64 = row.get::<_, Option<i64>>(7)?.unwrap_or(0);
            let tokens_total: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(0);

            Ok(TodaySummary {
                time_saved_secs: (stt_words as f64) * 1.1,
                total_words: stt_words + tts_words,
                total_sessions,
                stt_count,
                stt_words,
                stt_seconds,
                tts_count,
                tts_words,
                llm_count,
                llm_latency_avg: if llm_count > 0 { llm_latency_total / llm_count } else { 0 },
                tokens_total,
            })
        })
        .map_err(|e| Error::Database(format!("get_today query: {e}")))?;

    Ok(result)
}

// ─── Helpers ─────────────────────────────────────────────

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

#[cfg(test)]
mod latency_tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn);
        conn
    }

    fn insert(conn: &Connection, date: &str, ms: i64, model: &str) {
        conn.execute(
            "INSERT INTO events (type, created_at, date, latency_ms, mode, model)
             VALUES ('dictation', ?1, ?1, ?2, 'raw', ?3)",
            params![format!("{date}T12:00:00"), ms, model],
        )
        .unwrap();
    }

    #[test]
    fn percentile_nearest_rank() {
        let v: Vec<i64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50.0), 50);
        assert_eq!(percentile(&v, 95.0), 95);
        assert_eq!(percentile(&[7], 95.0), 7);
        assert_eq!(percentile(&[], 50.0), 0);
    }

    #[test]
    fn latency_stats_over_window() {
        let conn = db();
        for (i, ms) in [100, 200, 300, 400, 1000].iter().enumerate() {
            insert(&conn, "2026-07-01", *ms, if i < 3 { "qwen" } else { "whisper" });
        }
        insert(&conn, "2026-06-01", 9999, "qwen"); // outside window

        let s = get_dictation_latency(&conn, "2026-07-01", "2026-07-31").unwrap();
        assert_eq!(s.count, 5);
        assert_eq!(s.p50_ms, 300);
        assert_eq!(s.p95_ms, 1000);
        assert_eq!(s.min_ms, 100);
        assert_eq!(s.max_ms, 1000);
        assert_eq!(s.avg_ms, 400);
        assert_eq!(s.by_model.len(), 2);
        assert_eq!(s.by_model[0].model, "qwen"); // most-used first
        assert_eq!(s.by_model[0].count, 3);
        assert_eq!(s.by_model[0].p50_ms, 200);
    }

    #[test]
    fn empty_window_is_zeroes() {
        let conn = db();
        let s = get_dictation_latency(&conn, "2026-07-01", "2026-07-31").unwrap();
        assert_eq!(s.count, 0);
        assert_eq!(s.p50_ms, 0);
        assert!(s.by_model.is_empty());
    }

    #[test]
    fn dictation_rows_do_not_inflate_session_count() {
        let conn = db();
        record_event(&conn, "stt", "hi", "", 1.0, 500, "raw", "qwen", "", "", 0, 0, "").unwrap();
        record_dictation_latency(&conn, 800, "raw", "qwen").unwrap();
        let today = get_today(&conn).unwrap();
        assert_eq!(today.total_sessions, 1, "dictation latency rows must not count as sessions");
    }
}

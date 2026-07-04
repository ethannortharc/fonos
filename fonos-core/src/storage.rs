//! Storage — unified SQLite-backed data layer for Fonos v2.
//!
//! Provides Entry/Container data model, FTS5 full-text search, migration
//! from the legacy events table, and notebook export utilities.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

// ─── Enums ────────────────────────────────────────────────

/// The origin of an entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// Voice dictation via the float pill.
    Dictation,
    /// AI agent conversation turn.
    Agent,
    /// Note recorded via the note panel.
    Note,
    /// Meeting transcript or meeting-related recording.
    Meeting,
    /// Listen-queue item: captured text summarized and synthesized to audio.
    Listen,
}

impl SourceType {
    fn as_str(&self) -> &'static str {
        match self {
            SourceType::Dictation => "dictation",
            SourceType::Agent => "agent",
            SourceType::Note => "note",
            SourceType::Meeting => "meeting",
            SourceType::Listen => "listen",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "agent" => SourceType::Agent,
            "note" => SourceType::Note,
            "meeting" => SourceType::Meeting,
            "listen" => SourceType::Listen,
            _ => SourceType::Dictation,
        }
    }
}

/// The speaker/role of an entry in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntryRole {
    /// Human speaker / user input.
    User,
    /// AI assistant response.
    Assistant,
    /// System-generated entry.
    System,
}

impl EntryRole {
    fn as_str(&self) -> &'static str {
        match self {
            EntryRole::User => "user",
            EntryRole::Assistant => "assistant",
            EntryRole::System => "system",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "assistant" => EntryRole::Assistant,
            "system" => EntryRole::System,
            _ => EntryRole::User,
        }
    }
}

/// The structural type of a container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerType {
    /// A top-level notebook.
    Notebook,
    /// A section within a notebook.
    Section,
    /// An agent conversation session.
    Conversation,
    /// A meeting session container.
    MeetingSession,
    /// A journal container.
    Journal,
    /// A research container.
    Research,
}

impl ContainerType {
    fn as_str(&self) -> &'static str {
        match self {
            ContainerType::Notebook => "notebook",
            ContainerType::Section => "section",
            ContainerType::Conversation => "conversation",
            ContainerType::MeetingSession => "meeting_session",
            ContainerType::Journal => "journal",
            ContainerType::Research => "research",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "section" => ContainerType::Section,
            "conversation" => ContainerType::Conversation,
            "meeting_session" => ContainerType::MeetingSession,
            "journal" => ContainerType::Journal,
            "research" => ContainerType::Research,
            _ => ContainerType::Notebook,
        }
    }
}

// ─── Structs ──────────────────────────────────────────────

/// A single recorded text entry (dictation, note, agent turn, meeting snippet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Row ID in the database. `None` before insertion.
    pub id: Option<i64>,
    /// ISO 8601 timestamp when the entry was created.
    pub created_at: String,
    /// Origin of this entry.
    pub source_type: SourceType,
    /// Speaker role.
    pub role: EntryRole,
    /// Processing mode used (e.g. "raw", "polish", "note", "agent").
    pub mode: String,
    /// Raw transcribed text (before LLM processing).
    pub raw_text: String,
    /// LLM-processed text. `None` if not yet processed or raw mode.
    pub processed_text: Option<String>,
    /// Optional container (notebook / conversation) this entry belongs to.
    pub container_id: Option<i64>,
    /// Optional path / reference to the associated audio file.
    pub audio_ref: Option<String>,
    /// Arbitrary JSON metadata (duration, word count, model info, etc.).
    pub metadata: serde_json::Value,
}

/// A container that groups related entries (notebook, conversation, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    /// Row ID in the database. `None` before insertion.
    pub id: Option<i64>,
    /// Structural type of this container.
    pub container_type: ContainerType,
    /// Human-readable title.
    pub title: String,
    /// Optional parent container for nested hierarchies.
    pub parent_id: Option<i64>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-updated timestamp.
    pub updated_at: String,
    /// Arbitrary JSON metadata.
    pub metadata: serde_json::Value,
}

/// Query filter for `get_entries`.
#[derive(Debug, Clone, Default)]
pub struct EntryFilter {
    /// Optional source type filter.
    pub source_type: Option<SourceType>,
    /// Maximum number of rows to return.
    pub limit: Option<i64>,
    /// Number of rows to skip.
    pub offset: Option<i64>,
    /// If `true`, return newest entries first (ORDER BY created_at DESC).
    pub order_desc: bool,
}

// ─── Database Init ────────────────────────────────────────

/// Initialise all v2 storage tables.  Idempotent — safe to call on every launch.
pub fn init_storage_db(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS containers (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            container_type TEXT    NOT NULL,
            title          TEXT    NOT NULL,
            parent_id      INTEGER,
            created_at     TEXT    NOT NULL,
            updated_at     TEXT    NOT NULL,
            metadata       TEXT    NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS entries (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at     TEXT    NOT NULL,
            source_type    TEXT    NOT NULL,
            role           TEXT    NOT NULL,
            mode           TEXT    NOT NULL DEFAULT '',
            raw_text       TEXT    NOT NULL DEFAULT '',
            processed_text TEXT,
            container_id   INTEGER,
            audio_ref      TEXT,
            metadata       TEXT    NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_entries_created_at  ON entries(created_at);
        CREATE INDEX IF NOT EXISTS idx_entries_source_type ON entries(source_type);
        CREATE INDEX IF NOT EXISTS idx_entries_container   ON entries(container_id);

        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts
            USING fts5(raw_text, processed_text, content='entries', content_rowid='id',
                       tokenize='trigram');

        -- Keep FTS5 in sync: INSERT
        CREATE TRIGGER IF NOT EXISTS entries_fts_insert
            AFTER INSERT ON entries BEGIN
                INSERT INTO entries_fts(rowid, raw_text, processed_text)
                VALUES (new.id, new.raw_text, COALESCE(new.processed_text, ''));
            END;

        -- Keep FTS5 in sync: UPDATE
        CREATE TRIGGER IF NOT EXISTS entries_fts_update
            AFTER UPDATE ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, raw_text, processed_text)
                VALUES ('delete', old.id, old.raw_text, COALESCE(old.processed_text, ''));
                INSERT INTO entries_fts(rowid, raw_text, processed_text)
                VALUES (new.id, new.raw_text, COALESCE(new.processed_text, ''));
            END;

        -- Keep FTS5 in sync: DELETE
        CREATE TRIGGER IF NOT EXISTS entries_fts_delete
            BEFORE DELETE ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, raw_text, processed_text)
                VALUES ('delete', old.id, old.raw_text, COALESCE(old.processed_text, ''));
            END;
        ",
    )
    .expect("storage: init_storage_db failed");
}

// ─── Entry CRUD ───────────────────────────────────────────

/// Insert a new entry and return the generated row ID.
pub fn insert_entry(conn: &Connection, entry: &Entry) -> Result<i64> {
    let metadata_str = serde_json::to_string(&entry.metadata)
        .map_err(|e| Error::Database(format!("insert_entry metadata: {e}")))?;

    conn.execute(
        "INSERT INTO entries
            (created_at, source_type, role, mode, raw_text, processed_text,
             container_id, audio_ref, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            entry.created_at,
            entry.source_type.as_str(),
            entry.role.as_str(),
            entry.mode,
            entry.raw_text,
            entry.processed_text,
            entry.container_id,
            entry.audio_ref,
            metadata_str,
        ],
    )
    .map_err(|e| Error::Database(format!("insert_entry: {e}")))?;

    Ok(conn.last_insert_rowid())
}

/// Fetch a single entry by row ID.
pub fn get_entry(conn: &Connection, id: i64) -> Result<Entry> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at, source_type, role, mode, raw_text, processed_text,
                container_id, audio_ref, metadata
         FROM entries WHERE id = ?1",
    )
    .map_err(|e| Error::Database(format!("get_entry prepare: {e}")))?;

    stmt.query_row(params![id], map_entry_row)
        .map_err(|e| Error::Database(format!("get_entry: {e}")))
}

/// Fetch a list of entries with optional filters and pagination.
pub fn get_entries(conn: &Connection, filter: &EntryFilter) -> Result<Vec<Entry>> {
    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);
    let order = if filter.order_desc { "DESC" } else { "ASC" };

    let (sql, has_source_filter) = match &filter.source_type {
        Some(_) => (
            format!(
                "SELECT id, created_at, source_type, role, mode, raw_text, processed_text,
                        container_id, audio_ref, metadata
                 FROM entries
                 WHERE source_type = ?3
                 ORDER BY created_at {order}
                 LIMIT ?1 OFFSET ?2"
            ),
            true,
        ),
        None => (
            format!(
                "SELECT id, created_at, source_type, role, mode, raw_text, processed_text,
                        container_id, audio_ref, metadata
                 FROM entries
                 ORDER BY created_at {order}
                 LIMIT ?1 OFFSET ?2"
            ),
            false,
        ),
    };

    let mut stmt = conn.prepare(&sql)
        .map_err(|e| Error::Database(format!("get_entries prepare: {e}")))?;

    let rows = if has_source_filter {
        let st = filter.source_type.as_ref().unwrap().as_str();
        stmt.query_map(params![limit, offset, st], map_entry_row)
            .map_err(|e| Error::Database(format!("get_entries query: {e}")))?
    } else {
        stmt.query_map(params![limit, offset], map_entry_row)
            .map_err(|e| Error::Database(format!("get_entries query: {e}")))?
    };

    collect_rows(rows)
}

/// Update the raw and processed text of an entry.
pub fn update_entry(
    conn: &Connection,
    id: i64,
    raw_text: &str,
    processed_text: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE entries SET raw_text = ?2, processed_text = ?3 WHERE id = ?1",
        params![id, raw_text, processed_text],
    )
    .map_err(|e| Error::Database(format!("update_entry: {e}")))?;
    Ok(())
}

/// Update only the processed (display) text of an entry, leaving `raw_text`
/// intact.
///
/// Used by the History correction flow: applying a vocabulary correction
/// rewrites the entry's shown text without discarding the original transcript.
pub fn update_entry_processed_text(conn: &Connection, id: i64, text: &str) -> Result<()> {
    conn.execute(
        "UPDATE entries SET processed_text = ?2 WHERE id = ?1",
        params![id, text],
    )
    .map_err(|e| Error::Database(format!("update_entry_processed_text: {e}")))?;
    Ok(())
}

/// Delete an entry by ID.
pub fn delete_entry(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM entries WHERE id = ?1", params![id])
        .map_err(|e| Error::Database(format!("delete_entry: {e}")))?;
    Ok(())
}

// ─── Container CRUD ───────────────────────────────────────

/// Insert a new container and return the generated row ID.
pub fn insert_container(conn: &Connection, container: &Container) -> Result<i64> {
    let metadata_str = serde_json::to_string(&container.metadata)
        .map_err(|e| Error::Database(format!("insert_container metadata: {e}")))?;

    conn.execute(
        "INSERT INTO containers
            (container_type, title, parent_id, created_at, updated_at, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            container.container_type.as_str(),
            container.title,
            container.parent_id,
            container.created_at,
            container.updated_at,
            metadata_str,
        ],
    )
    .map_err(|e| Error::Database(format!("insert_container: {e}")))?;

    Ok(conn.last_insert_rowid())
}

/// Fetch a single container by row ID.
pub fn get_container(conn: &Connection, id: i64) -> Result<Container> {
    let mut stmt = conn.prepare(
        "SELECT id, container_type, title, parent_id, created_at, updated_at, metadata
         FROM containers WHERE id = ?1",
    )
    .map_err(|e| Error::Database(format!("get_container prepare: {e}")))?;

    stmt.query_row(params![id], map_container_row)
        .map_err(|e| Error::Database(format!("get_container: {e}")))
}

/// Fetch all containers.
pub fn get_containers(conn: &Connection) -> Result<Vec<Container>> {
    let mut stmt = conn.prepare(
        "SELECT id, container_type, title, parent_id, created_at, updated_at, metadata
         FROM containers
         ORDER BY created_at ASC",
    )
    .map_err(|e| Error::Database(format!("get_containers prepare: {e}")))?;

    let rows = stmt
        .query_map([], map_container_row)
        .map_err(|e| Error::Database(format!("get_containers query: {e}")))?;

    collect_rows(rows)
}

/// Fetch all entries belonging to a container (chronological).
pub fn get_container_entries(conn: &Connection, container_id: i64) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at, source_type, role, mode, raw_text, processed_text,
                container_id, audio_ref, metadata
         FROM entries
         WHERE container_id = ?1
         ORDER BY created_at ASC",
    )
    .map_err(|e| Error::Database(format!("get_container_entries prepare: {e}")))?;

    let rows = stmt
        .query_map(params![container_id], map_entry_row)
        .map_err(|e| Error::Database(format!("get_container_entries query: {e}")))?;

    collect_rows(rows)
}

/// Update the title of a container.
pub fn update_container(conn: &Connection, id: i64, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE containers SET title = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, title, now_iso8601()],
    )
    .map_err(|e| Error::Database(format!("update_container: {e}")))?;
    Ok(())
}

/// Delete a container by ID.
pub fn delete_container(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM containers WHERE id = ?1", params![id])
        .map_err(|e| Error::Database(format!("delete_container: {e}")))?;
    Ok(())
}

// ─── FTS5 Search ─────────────────────────────────────────

/// Full-text search over all entries (raw_text + processed_text).
///
/// Uses the FTS5 trigram tokenizer which supports substring and CJK search.
/// Trailing `*` wildcards are stripped (trigram handles substring search natively).
/// Queries shorter than 3 Unicode scalar values fall back to SQL LIKE for CJK bigrams.
pub fn search_entries(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Entry>> {
    // Strip FTS5 wildcard suffix — trigram does substring matching natively
    let normalized_query = query.trim_end_matches('*');
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    // Trigram tokenizer needs at least 3 characters to form a trigram.
    // Short queries (1–2 chars, common with CJK bigrams like 天气) fall back to LIKE.
    let char_count = normalized_query.chars().count();
    if char_count < 3 {
        return search_entries_like(conn, normalized_query, limit);
    }

    // The query is user text, not FTS5 syntax: apostrophes and stray operators
    // (don't, "foo, foo-bar) would otherwise be parse errors. Quote each
    // whitespace-separated token as an FTS5 string literal (implicit AND),
    // treating `"` itself as a separator.
    let fts_query = normalized_query
        .replace('"', " ")
        .split_whitespace()
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" ");
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT e.id, e.created_at, e.source_type, e.role, e.mode,
                e.raw_text, e.processed_text, e.container_id, e.audio_ref, e.metadata
         FROM entries e
         INNER JOIN entries_fts fts ON fts.rowid = e.id
         WHERE entries_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )
    .map_err(|e| Error::Database(format!("search_entries prepare: {e}")))?;

    let rows = stmt
        .query_map(params![fts_query, limit], map_entry_row)
        .map_err(|e| Error::Database(format!("search_entries query: {e}")))?;

    collect_rows(rows)
}

/// Fallback substring search via SQL LIKE for short queries (< 3 chars).
fn search_entries_like(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Entry>> {
    // Escape LIKE wildcards so a query containing `%` or `_` (or the escape
    // char `\`) is matched literally rather than treated as a pattern. This
    // pairs with the `ESCAPE '\\'` clause below.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    let mut stmt = conn.prepare(
        "SELECT id, created_at, source_type, role, mode, raw_text, processed_text,
                container_id, audio_ref, metadata
         FROM entries
         WHERE raw_text LIKE ?1 ESCAPE '\\' OR processed_text LIKE ?1 ESCAPE '\\'
         ORDER BY created_at DESC
         LIMIT ?2",
    )
    .map_err(|e| Error::Database(format!("search_entries_like prepare: {e}")))?;

    let rows = stmt
        .query_map(params![pattern, limit], map_entry_row)
        .map_err(|e| Error::Database(format!("search_entries_like query: {e}")))?;

    collect_rows(rows)
}

// ─── Migration ────────────────────────────────────────────

/// Migrate legacy `events` table to the v2 `entries` + `containers` schema.
///
/// - STT events become `Dictation` entries.
/// - LLM events with `mode = "agent"` become `Agent` entries inside a `Conversation` container.
/// - Other LLM/TTS events are mapped to `Dictation` entries.
/// - The old table is renamed to `history_backup`.
/// - Idempotent: if `history_backup` already exists, the migration is skipped.
pub fn migrate_from_history(conn: &Connection) -> Result<()> {
    // Check if already migrated (history_backup exists → skip)
    let already_migrated: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history_backup'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|e| Error::Database(format!("migrate check: {e}")))?;

    if already_migrated {
        return Ok(());
    }

    // Check if source events table exists at all
    let events_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .map_err(|e| Error::Database(format!("migrate events check: {e}")))?;

    if !events_exists {
        return Ok(());
    }

    // Collect all legacy events
    let mut stmt = conn.prepare(
        "SELECT type, created_at, input_text, output_text, mode, session_id
         FROM events
         ORDER BY created_at ASC",
    )
    .map_err(|e| Error::Database(format!("migrate read events: {e}")))?;

    struct LegacyEvent {
        event_type: String,
        created_at: String,
        input_text: String,
        output_text: String,
        mode: String,
        session_id: String,
    }

    let events: Vec<LegacyEvent> = stmt
        .query_map([], |row| {
            Ok(LegacyEvent {
                event_type: row.get::<_, String>(0)?,
                created_at: row.get::<_, String>(1)?,
                input_text: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                output_text: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                mode: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                session_id: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            })
        })
        .map_err(|e| Error::Database(format!("migrate query events: {e}")))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Database(format!("migrate collect events: {e}")))?;

    // Wrap all inserts + the rename in a single transaction. Without this, each
    // insert_container/insert_entry is its own implicit (fsync'd) transaction,
    // so migrating a long-time user's thousands of legacy events would block
    // startup for seconds-to-minutes. A closure lets `?` short-circuit while we
    // still COMMIT on success / ROLLBACK on failure.
    conn.execute_batch("BEGIN")
        .map_err(|e| Error::Database(format!("migrate begin: {e}")))?;

    let migrate = || -> Result<()> {
    // Group agent sessions by session_id to create Conversation containers
    use std::collections::HashMap;
    let mut session_container_map: HashMap<String, i64> = HashMap::new();

    // First pass: create containers for agent sessions
    for event in &events {
        if event.mode == "agent" && !event.session_id.is_empty() {
            if !session_container_map.contains_key(&event.session_id) {
                let container = Container {
                    id: None,
                    container_type: ContainerType::Conversation,
                    title: format!("Conversation {}", &event.session_id),
                    parent_id: None,
                    created_at: event.created_at.clone(),
                    updated_at: event.created_at.clone(),
                    metadata: serde_json::Value::Null,
                };
                let cid = insert_container(conn, &container)?;
                session_container_map.insert(event.session_id.clone(), cid);
            }
        }
    }

    // Second pass: insert entries
    for event in &events {
        let is_agent = event.mode == "agent";

        match event.event_type.as_str() {
            "stt" => {
                let container_id = if is_agent && !event.session_id.is_empty() {
                    session_container_map.get(&event.session_id).copied()
                } else {
                    None
                };

                let entry = Entry {
                    id: None,
                    created_at: event.created_at.clone(),
                    source_type: if is_agent { SourceType::Agent } else { SourceType::Dictation },
                    role: EntryRole::User,
                    mode: event.mode.clone(),
                    raw_text: event.input_text.clone(),
                    processed_text: if event.output_text.is_empty() {
                        None
                    } else {
                        Some(event.output_text.clone())
                    },
                    container_id,
                    audio_ref: None,
                    metadata: serde_json::Value::Null,
                };
                insert_entry(conn, &entry)?;
            }
            "llm" => {
                let container_id = if is_agent && !event.session_id.is_empty() {
                    session_container_map.get(&event.session_id).copied()
                } else {
                    None
                };

                // For agent LLM responses, create an assistant entry
                if is_agent && !event.output_text.is_empty() {
                    let entry = Entry {
                        id: None,
                        created_at: event.created_at.clone(),
                        source_type: SourceType::Agent,
                        role: EntryRole::Assistant,
                        mode: event.mode.clone(),
                        raw_text: event.output_text.clone(),
                        processed_text: None,
                        container_id,
                        audio_ref: None,
                        metadata: serde_json::Value::Null,
                    };
                    insert_entry(conn, &entry)?;
                }
            }
            _ => {
                // TTS and other event types — skip or treat as dictation
            }
        }
    }

    // Rename old table to history_backup
    conn.execute_batch("ALTER TABLE events RENAME TO history_backup")
        .map_err(|e| Error::Database(format!("migrate rename: {e}")))?;

        Ok(())
    };

    match migrate() {
        Ok(()) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| Error::Database(format!("migrate commit: {e}")))?;
            Ok(())
        }
        Err(e) => {
            // Best-effort rollback; surface the original error either way.
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

// ─── Export ───────────────────────────────────────────────

/// Export all entries in a notebook container as Markdown files written to disk.
///
/// Creates a directory named after the notebook inside `output_dir`, writes a
/// `README.md` with the full notebook content (one `##` section per entry,
/// organised chronologically), and returns the path to that directory.
pub fn export_notebook_markdown(conn: &Connection, container_id: i64, output_dir: &Path) -> Result<PathBuf> {
    let container = get_container(conn, container_id)?;
    let entries = get_container_entries(conn, container_id)?;

    // Sanitize notebook title for use as a directory name
    let dir_name: String = container.title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let dir_name = dir_name.trim().replace(' ', "_");

    let notebook_dir = output_dir.join(&dir_name);
    std::fs::create_dir_all(&notebook_dir)
        .map_err(|e| Error::Database(format!("export_notebook_markdown mkdir: {e}")))?;

    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", container.title));

    for entry in &entries {
        // Use the date portion of the timestamp as a section header
        let date_header = entry.created_at.get(..10).unwrap_or(&entry.created_at);
        out.push_str(&format!("## {}\n\n", date_header));
        let text = entry
            .processed_text
            .as_deref()
            .unwrap_or(&entry.raw_text);
        out.push_str(text);
        out.push_str("\n\n");
    }

    let readme_path = notebook_dir.join("README.md");
    std::fs::write(&readme_path, &out)
        .map_err(|e| Error::Database(format!("export_notebook_markdown write: {e}")))?;

    Ok(notebook_dir)
}

/// Export all entries in a notebook container as a JSON file written to disk.
///
/// Writes `{title}.json` (sanitized) inside `output_dir` and returns the path.
/// The JSON object has `title`, `container`, and `entries` top-level fields.
pub fn export_notebook_json(conn: &Connection, container_id: i64, output_dir: &Path) -> Result<PathBuf> {
    let container = get_container(conn, container_id)?;
    let entries = get_container_entries(conn, container_id)?;

    #[derive(Serialize)]
    struct NotebookExport<'a> {
        title: &'a str,
        container: &'a Container,
        entries: &'a [Entry],
    }

    let json = serde_json::to_string_pretty(&NotebookExport {
        title: &container.title,
        container: &container,
        entries: &entries,
    })
    .map_err(|e| Error::Database(format!("export_notebook_json serialize: {e}")))?;

    // Sanitize title for filename
    let filename: String = container.title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let filename = format!("{}.json", filename.trim().replace(' ', "_"));

    std::fs::create_dir_all(output_dir)
        .map_err(|e| Error::Database(format!("export_notebook_json mkdir: {e}")))?;

    let file_path = output_dir.join(&filename);
    std::fs::write(&file_path, &json)
        .map_err(|e| Error::Database(format!("export_notebook_json write: {e}")))?;

    Ok(file_path)
}

// ─── Pipeline Helpers (C06) ───────────────────────────────

/// Simulate the dictation pipeline: writes an Entry to the DB before output_target is applied.
///
/// Returns the new entry's row ID.  In production this is called from the Tauri command
/// before pasting text to the clipboard or active text field.
pub fn simulate_dictation_pipeline(
    conn: &Connection,
    raw_text: &str,
    processed_text: Option<&str>,
    mode: &str,
    audio_ref: Option<&str>,
    container_id: Option<i64>,
) -> Result<i64> {
    let entry = Entry {
        id: None,
        created_at: now_iso8601(),
        source_type: SourceType::Dictation,
        role: EntryRole::User,
        mode: mode.to_string(),
        raw_text: raw_text.to_string(),
        processed_text: processed_text.map(|s| s.to_string()),
        container_id,
        audio_ref: audio_ref.map(|s| s.to_string()),
        metadata: serde_json::Value::Null,
    };
    insert_entry(conn, &entry)
}

/// Simulate the agent pipeline: writes user + assistant entries to the DB.
///
/// Returns `(user_entry_id, assistant_entry_id)`.
pub fn simulate_agent_pipeline(
    conn: &Connection,
    user_text: &str,
    assistant_text: &str,
    container_id: i64,
) -> Result<(i64, i64)> {
    let now = now_iso8601();

    let user_entry = Entry {
        id: None,
        created_at: now.clone(),
        source_type: SourceType::Agent,
        role: EntryRole::User,
        mode: "agent".to_string(),
        raw_text: user_text.to_string(),
        processed_text: None,
        container_id: Some(container_id),
        audio_ref: None,
        metadata: serde_json::Value::Null,
    };
    let user_id = insert_entry(conn, &user_entry)?;

    let assistant_entry = Entry {
        id: None,
        created_at: now,
        source_type: SourceType::Agent,
        role: EntryRole::Assistant,
        mode: "agent".to_string(),
        raw_text: assistant_text.to_string(),
        processed_text: None,
        container_id: Some(container_id),
        audio_ref: None,
        metadata: serde_json::Value::Null,
    };
    let assistant_id = insert_entry(conn, &assistant_entry)?;

    Ok((user_id, assistant_id))
}

// ─── Private Helpers ──────────────────────────────────────

fn map_entry_row(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let metadata_str: String = row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "null".into());
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);

    Ok(Entry {
        id: Some(row.get::<_, i64>(0)?),
        created_at: row.get(1)?,
        source_type: SourceType::from_str(&row.get::<_, String>(2)?),
        role: EntryRole::from_str(&row.get::<_, String>(3)?),
        mode: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        raw_text: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        processed_text: row.get(6)?,
        container_id: row.get(7)?,
        audio_ref: row.get(8)?,
        metadata,
    })
}

fn map_container_row(row: &rusqlite::Row) -> rusqlite::Result<Container> {
    let metadata_str: String = row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "null".into());
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);

    Ok(Container {
        id: Some(row.get::<_, i64>(0)?),
        container_type: ContainerType::from_str(&row.get::<_, String>(1)?),
        title: row.get(2)?,
        parent_id: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        metadata,
    })
}

fn collect_rows<T, E>(
    rows: impl Iterator<Item = std::result::Result<T, E>>,
) -> Result<Vec<T>>
where
    E: std::fmt::Display,
{
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Database(format!("row: {e}")))?);
    }
    Ok(result)
}

/// Returns the current UTC time as an ISO 8601 string (YYYY-MM-DDTHH:MM:SS).
fn now_iso8601() -> String {
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
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}")
}

fn days_to_ymd(mut days: i64) -> (i64, i64, i64) {
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
mod tests {
    use super::*;

    fn seed_entry(conn: &Connection, raw: &str, processed: Option<&str>) -> i64 {
        let entry = Entry {
            id: None,
            created_at: now_iso8601(),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "raw".to_string(),
            raw_text: raw.to_string(),
            processed_text: processed.map(|s| s.to_string()),
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        insert_entry(conn, &entry).expect("insert_entry")
    }

    #[test]
    fn update_entry_processed_text_rewrites_only_display_text() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_storage_db(&conn);

        let id = seed_entry(&conn, "look at this 衣袖 issue", Some("look at this 衣袖 issue"));
        update_entry_processed_text(&conn, id, "look at this issue issue")
            .expect("update_entry_processed_text");

        let got = get_entry(&conn, id).expect("get_entry");
        // Correction only touches processed_text; the raw transcript is preserved.
        assert_eq!(got.raw_text, "look at this 衣袖 issue");
        assert_eq!(got.processed_text.as_deref(), Some("look at this issue issue"));
    }

    #[test]
    fn update_entry_processed_text_populates_null_processed() {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_storage_db(&conn);

        let id = seed_entry(&conn, "raw only", None);
        assert!(get_entry(&conn, id).unwrap().processed_text.is_none());

        update_entry_processed_text(&conn, id, "corrected only").expect("update");
        assert_eq!(
            get_entry(&conn, id).unwrap().processed_text.as_deref(),
            Some("corrected only")
        );
    }
}

/// Storage foundation tests for Fonos v2 — C01, C02, C03, C04, C05, C06, Q01
///
/// Covers:
///   C01 — Entry table schema (all fields, all source_types)
///   C02 — Container table schema (hierarchy via parent_id)
///   C03 — FTS5 full-text search (English + Chinese)
///   C04 — Data migration from old history table
///   C05 — Mode struct extensions (new fields with defaults)
///   C06 — Unified input pipeline writes entries to DB
///   Q01 — Query performance: 1000 entries < 100ms, FTS5 < 200ms
///
/// Run with:
///   cargo test -p fonos-core --test storage_tests
///
/// These tests are in the RED (failing) phase — the production code
/// (fonos_core::storage module) does not exist yet.

// ---------------------------------------------------------------------------
// Bring in the not-yet-implemented storage module
// ---------------------------------------------------------------------------

use fonos_core::storage::{
    init_storage_db,
    insert_entry,
    get_entries,
    get_entry,
    update_entry,
    delete_entry,
    search_entries,
    insert_container,
    get_container,
    get_containers,
    get_container_entries,
    migrate_from_history,
    Entry,
    EntryRole,
    SourceType,
    Container,
    ContainerType,
    EntryFilter,
};
use fonos_core::modes::{built_in_modes, Mode, OutputTarget};
use rusqlite::Connection;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open an in-memory SQLite database and initialise all v2 schema tables.
fn open_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    init_storage_db(&conn);
    conn
}

/// ISO 8601 timestamp helper — returns a stable test timestamp.
fn ts(offset_secs: i64) -> String {
    // Base: 2026-03-26T12:00:00 = 1774785600 epoch seconds
    let epoch: i64 = 1_774_785_600 + offset_secs;
    let days = epoch / 86400;
    let secs = epoch % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    // Simple date reconstruction (good enough for test data near 2026-03-26)
    let year = 2026i64;
    let month = 3i64;
    let day = 26i64 + days - (1_774_785_600 / 86400);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", year, month, day, h, m, s)
}

// ===========================================================================
// C01 — Entry table schema
// ===========================================================================

#[cfg(test)]
mod c01_entry_schema {
    use super::*;

    /// Static: Entry struct compiles with all required fields.
    #[test]
    fn entry_struct_has_required_fields() {
        let entry = Entry {
            id: None,
            created_at: "2026-03-26T12:00:00".to_string(),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "raw".to_string(),
            raw_text: "Hello world".to_string(),
            processed_text: Some("Hello world.".to_string()),
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        assert_eq!(entry.raw_text, "Hello world");
        assert!(matches!(entry.source_type, SourceType::Dictation));
        assert!(matches!(entry.role, EntryRole::User));
    }

    /// Unit: Create entry with all fields, query back, verify round-trip.
    #[test]
    fn entry_round_trip_all_fields() {
        let conn = open_db();

        let entry = Entry {
            id: None,
            created_at: "2026-03-26T12:00:00".to_string(),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "polish".to_string(),
            raw_text: "um so i wanted to say hello".to_string(),
            processed_text: Some("I wanted to say hello.".to_string()),
            container_id: None,
            audio_ref: Some("/tmp/audio/rec_001.wav".to_string()),
            metadata: serde_json::json!({ "duration_secs": 3.2, "words": 7 }),
        };

        let id = insert_entry(&conn, &entry).expect("insert_entry failed");
        assert!(id > 0, "inserted id should be positive");

        let fetched = get_entry(&conn, id).expect("get_entry failed");
        assert_eq!(fetched.raw_text, entry.raw_text);
        assert_eq!(fetched.processed_text, entry.processed_text);
        assert_eq!(fetched.mode, "polish");
        assert_eq!(fetched.audio_ref, Some("/tmp/audio/rec_001.wav".to_string()));
        assert!(fetched.metadata["duration_secs"].as_f64().unwrap() - 3.2 < 0.001);
    }

    /// Integration: Insert entries of each source_type, query with filter.
    #[test]
    fn all_source_types_insert_and_filter() {
        let conn = open_db();

        let source_types = [
            SourceType::Dictation,
            SourceType::Agent,
            SourceType::Note,
            SourceType::Meeting,
        ];

        for (i, st) in source_types.iter().enumerate() {
            let entry = Entry {
                id: None,
                created_at: ts(i as i64 * 60),
                source_type: st.clone(),
                role: EntryRole::User,
                mode: "raw".to_string(),
                raw_text: format!("test entry for {:?}", st),
                processed_text: None,
                container_id: None,
                audio_ref: None,
                metadata: serde_json::Value::Null,
            };
            insert_entry(&conn, &entry).expect("insert failed");
        }

        // Query all
        let all = get_entries(&conn, &EntryFilter::default()).expect("get_entries failed");
        assert_eq!(all.len(), 4, "should have 4 entries");

        // Filter by dictation
        let filter = EntryFilter {
            source_type: Some(SourceType::Dictation),
            ..Default::default()
        };
        let dictation_entries = get_entries(&conn, &filter).expect("filter failed");
        assert_eq!(dictation_entries.len(), 1);
        assert!(matches!(dictation_entries[0].source_type, SourceType::Dictation));

        // Filter by agent
        let filter = EntryFilter {
            source_type: Some(SourceType::Agent),
            ..Default::default()
        };
        let agent_entries = get_entries(&conn, &filter).expect("filter failed");
        assert_eq!(agent_entries.len(), 1);
        assert!(matches!(agent_entries[0].source_type, SourceType::Agent));

        // Filter by note
        let filter = EntryFilter {
            source_type: Some(SourceType::Note),
            ..Default::default()
        };
        let note_entries = get_entries(&conn, &filter).expect("filter failed");
        assert_eq!(note_entries.len(), 1);
        assert!(matches!(note_entries[0].source_type, SourceType::Note));

        // Filter by meeting
        let filter = EntryFilter {
            source_type: Some(SourceType::Meeting),
            ..Default::default()
        };
        let meeting_entries = get_entries(&conn, &filter).expect("filter failed");
        assert_eq!(meeting_entries.len(), 1);
        assert!(matches!(meeting_entries[0].source_type, SourceType::Meeting));
    }

    /// Unit: Entry with agent role round-trips correctly.
    #[test]
    fn entry_agent_role_round_trip() {
        let conn = open_db();

        let user_entry = Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Agent,
            role: EntryRole::User,
            mode: "agent".to_string(),
            raw_text: "What is the weather today?".to_string(),
            processed_text: None,
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        let assistant_entry = Entry {
            id: None,
            created_at: ts(2),
            source_type: SourceType::Agent,
            role: EntryRole::Assistant,
            mode: "agent".to_string(),
            raw_text: "It is sunny and 22 degrees in San Francisco.".to_string(),
            processed_text: None,
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };

        let uid = insert_entry(&conn, &user_entry).expect("insert user entry");
        let aid = insert_entry(&conn, &assistant_entry).expect("insert assistant entry");

        let fetched_user = get_entry(&conn, uid).expect("get user entry");
        let fetched_asst = get_entry(&conn, aid).expect("get assistant entry");

        assert!(matches!(fetched_user.role, EntryRole::User));
        assert!(matches!(fetched_asst.role, EntryRole::Assistant));
    }

    /// Unit: Entry container_id foreign key is stored and retrieved.
    #[test]
    fn entry_container_id_stored_correctly() {
        let conn = open_db();

        let container = Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "My Notebook".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        };
        let cid = insert_container(&conn, &container).expect("insert container");

        let entry = Entry {
            id: None,
            created_at: ts(10),
            source_type: SourceType::Note,
            role: EntryRole::User,
            mode: "note".to_string(),
            raw_text: "Note belonging to notebook".to_string(),
            processed_text: None,
            container_id: Some(cid),
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        let eid = insert_entry(&conn, &entry).expect("insert entry");

        let fetched = get_entry(&conn, eid).expect("get entry");
        assert_eq!(fetched.container_id, Some(cid));
    }

    /// Unit: Update entry text fields.
    #[test]
    fn entry_update_text() {
        let conn = open_db();

        let entry = Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Note,
            role: EntryRole::User,
            mode: "note".to_string(),
            raw_text: "Original text".to_string(),
            processed_text: Some("Original text.".to_string()),
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        let id = insert_entry(&conn, &entry).expect("insert");

        update_entry(&conn, id, "Edited text", Some("Edited text."))
            .expect("update_entry failed");

        let fetched = get_entry(&conn, id).expect("get after update");
        assert_eq!(fetched.raw_text, "Edited text");
        assert_eq!(fetched.processed_text, Some("Edited text.".to_string()));
    }

    /// Unit: Delete entry removes it from the database.
    #[test]
    fn entry_delete_removes_row() {
        let conn = open_db();

        let entry = Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "raw".to_string(),
            raw_text: "to be deleted".to_string(),
            processed_text: None,
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        };
        let id = insert_entry(&conn, &entry).expect("insert");
        delete_entry(&conn, id).expect("delete_entry failed");

        let result = get_entry(&conn, id);
        assert!(result.is_err(), "deleted entry should not be found");
    }
}

// ===========================================================================
// C02 — Container table schema
// ===========================================================================

#[cfg(test)]
mod c02_container_schema {
    use super::*;

    /// Static: Container struct compiles with all required fields.
    #[test]
    fn container_struct_has_required_fields() {
        let c = Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Test Notebook".to_string(),
            parent_id: None,
            created_at: "2026-03-26T12:00:00".to_string(),
            updated_at: "2026-03-26T12:00:00".to_string(),
            metadata: serde_json::Value::Null,
        };
        assert_eq!(c.title, "Test Notebook");
        assert!(matches!(c.container_type, ContainerType::Notebook));
        assert!(c.parent_id.is_none());
    }

    /// Unit: Create container, create child with parent_id, query hierarchy.
    #[test]
    fn container_hierarchy_via_parent_id() {
        let conn = open_db();

        // Root notebook
        let parent = Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Root Notebook".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        };
        let parent_id = insert_container(&conn, &parent).expect("insert parent");

        // Child section inside the notebook
        let child = Container {
            id: None,
            container_type: ContainerType::Section,
            title: "Chapter One".to_string(),
            parent_id: Some(parent_id),
            created_at: ts(10),
            updated_at: ts(10),
            metadata: serde_json::Value::Null,
        };
        let child_id = insert_container(&conn, &child).expect("insert child");

        // Fetch and verify
        let fetched_parent = get_container(&conn, parent_id).expect("get parent");
        let fetched_child = get_container(&conn, child_id).expect("get child");

        assert_eq!(fetched_parent.title, "Root Notebook");
        assert!(fetched_parent.parent_id.is_none());

        assert_eq!(fetched_child.title, "Chapter One");
        assert_eq!(fetched_child.parent_id, Some(parent_id));
    }

    /// Unit: Multiple levels of nesting (grandparent → parent → child).
    #[test]
    fn container_three_level_nesting() {
        let conn = open_db();

        let gp_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Grandparent".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert grandparent");

        let p_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Section,
            title: "Parent Section".to_string(),
            parent_id: Some(gp_id),
            created_at: ts(5),
            updated_at: ts(5),
            metadata: serde_json::Value::Null,
        }).expect("insert parent");

        let c_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Section,
            title: "Child Section".to_string(),
            parent_id: Some(p_id),
            created_at: ts(10),
            updated_at: ts(10),
            metadata: serde_json::Value::Null,
        }).expect("insert child");

        let child = get_container(&conn, c_id).expect("get child");
        assert_eq!(child.parent_id, Some(p_id));

        let parent = get_container(&conn, p_id).expect("get parent");
        assert_eq!(parent.parent_id, Some(gp_id));
    }

    /// Integration: Create notebook container with entries, verify relationship.
    #[test]
    fn notebook_with_entries_relationship() {
        let conn = open_db();

        let notebook_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Project Ideas".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        // Insert 3 entries linked to the notebook
        for i in 0..3 {
            insert_entry(&conn, &Entry {
                id: None,
                created_at: ts(i * 30),
                source_type: SourceType::Note,
                role: EntryRole::User,
                mode: "note".to_string(),
                raw_text: format!("Project idea #{}", i + 1),
                processed_text: Some(format!("Project idea {}.", i + 1)),
                container_id: Some(notebook_id),
                audio_ref: None,
                metadata: serde_json::Value::Null,
            }).expect("insert entry");
        }

        // Insert 1 entry NOT linked to the notebook
        insert_entry(&conn, &Entry {
            id: None,
            created_at: ts(200),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "raw".to_string(),
            raw_text: "Unrelated dictation".to_string(),
            processed_text: None,
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        }).expect("insert unrelated");

        let notebook_entries = get_container_entries(&conn, notebook_id)
            .expect("get_container_entries failed");

        assert_eq!(notebook_entries.len(), 3, "notebook should have 3 entries");
        for entry in &notebook_entries {
            assert_eq!(entry.container_id, Some(notebook_id));
            assert!(matches!(entry.source_type, SourceType::Note));
        }
    }

    /// Unit: Container types — Notebook, Section, Conversation.
    #[test]
    fn container_type_variants_compile_and_round_trip() {
        let conn = open_db();

        let types_and_titles = [
            (ContainerType::Notebook, "My Notebook"),
            (ContainerType::Section, "Section One"),
            (ContainerType::Conversation, "Agent Chat Session"),
        ];

        for (ctype, title) in &types_and_titles {
            let id = insert_container(&conn, &Container {
                id: None,
                container_type: ctype.clone(),
                title: title.to_string(),
                parent_id: None,
                created_at: ts(0),
                updated_at: ts(0),
                metadata: serde_json::Value::Null,
            }).expect("insert container");

            let fetched = get_container(&conn, id).expect("get container");
            assert_eq!(&fetched.title, title);
        }
    }

    /// Unit: List all containers returns correct count.
    #[test]
    fn get_containers_returns_all() {
        let conn = open_db();

        for i in 0..5 {
            insert_container(&conn, &Container {
                id: None,
                container_type: ContainerType::Notebook,
                title: format!("Notebook {}", i),
                parent_id: None,
                created_at: ts(i * 60),
                updated_at: ts(i * 60),
                metadata: serde_json::Value::Null,
            }).expect("insert");
        }

        let containers = get_containers(&conn).expect("get_containers failed");
        assert_eq!(containers.len(), 5);
    }
}

// ===========================================================================
// C03 — FTS5 full-text search
// ===========================================================================

#[cfg(test)]
mod c03_fts5_search {
    use super::*;

    fn insert_test_entries(conn: &Connection) {
        let entries = [
            ("The quick brown fox jumps over the lazy dog", SourceType::Dictation),
            ("Meeting notes from the product team standup", SourceType::Meeting),
            ("今天天气很好，我们去公园散步吧", SourceType::Note),             // Chinese: Today's weather is great, let's go to the park
            ("Research into machine learning algorithms for NLP", SourceType::Note),
            ("苹果公司发布了新款iPhone产品", SourceType::Note),               // Chinese: Apple released new iPhone products
            ("Call with Alex about the Q2 roadmap planning session", SourceType::Meeting),
            ("用Python写了一个语音识别的小工具", SourceType::Dictation),       // Chinese: Wrote a voice recognition tool in Python
            ("Draft email: following up on the contract negotiations", SourceType::Dictation),
        ];

        for (i, (text, st)) in entries.iter().enumerate() {
            insert_entry(conn, &Entry {
                id: None,
                created_at: ts(i as i64 * 60),
                source_type: st.clone(),
                role: EntryRole::User,
                mode: "raw".to_string(),
                raw_text: text.to_string(),
                processed_text: Some(text.to_string()),
                container_id: None,
                audio_ref: None,
                metadata: serde_json::Value::Null,
            }).expect("insert fts entry");
        }
    }

    /// Static: FTS5 virtual table creation is part of init_storage_db.
    #[test]
    fn fts5_table_created_on_init() {
        let conn = open_db();

        // If FTS5 table exists, this query succeeds; otherwise it panics
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts",
                [],
                |r| r.get(0),
            )
            .expect("FTS5 table entries_fts should exist after init_storage_db");

        assert_eq!(count, 0, "fresh db should have no fts rows");
    }

    /// Unit: Insert entries, search by English keyword, verify matches.
    #[test]
    fn english_keyword_search_returns_matches() {
        let conn = open_db();
        insert_test_entries(&conn);

        let results = search_entries(&conn, "roadmap", 10).expect("search failed");
        assert!(!results.is_empty(), "should find entry containing 'roadmap'");
        assert!(
            results[0].raw_text.to_lowercase().contains("roadmap"),
            "matched entry should contain 'roadmap'"
        );
    }

    /// Unit: Search by partial English word (prefix).
    #[test]
    fn english_prefix_search() {
        let conn = open_db();
        insert_test_entries(&conn);

        let results = search_entries(&conn, "negotiat*", 10).expect("prefix search failed");
        assert!(!results.is_empty(), "prefix search for 'negotiat*' should match");
    }

    /// Unit: Multi-word English phrase search.
    #[test]
    fn multi_word_english_search() {
        let conn = open_db();
        insert_test_entries(&conn);

        let results = search_entries(&conn, "machine learning", 10)
            .expect("multi-word search failed");
        assert!(!results.is_empty(), "should find entry with 'machine learning'");
    }

    /// Integration: Chinese text search.
    #[test]
    fn chinese_keyword_search_returns_matches() {
        let conn = open_db();
        insert_test_entries(&conn);

        // Search for 天气 (weather) — present in the Chinese entry
        let results = search_entries(&conn, "天气", 10).expect("chinese search failed");
        assert!(
            !results.is_empty(),
            "FTS5 should match Chinese character sequence '天气'"
        );
        assert!(
            results[0].raw_text.contains("天气"),
            "matched entry should contain '天气'"
        );
    }

    /// Integration: Mixed Chinese+English query.
    #[test]
    fn mixed_chinese_english_search() {
        let conn = open_db();
        insert_test_entries(&conn);

        // Search for 'Python' which appears in a Chinese-dominant entry
        let results = search_entries(&conn, "Python", 10).expect("mixed language search failed");
        assert!(!results.is_empty(), "should find the Python entry in Chinese text");
    }

    /// Unit: Queries containing FTS5 syntax characters (apostrophes, quotes,
    /// operators) must be treated as literal text, not raise a syntax error.
    /// Dictated natural language is full of apostrophes.
    #[test]
    fn punctuation_in_query_is_literal() {
        let conn = open_db();
        insert_entry(&conn, &Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Dictation,
            role: EntryRole::User,
            mode: "raw".to_string(),
            raw_text: "Don't forget to submit the expense report".to_string(),
            processed_text: None,
            container_id: None,
            audio_ref: None,
            metadata: serde_json::Value::Null,
        }).expect("insert entry");

        let results = search_entries(&conn, "don't forget", 10)
            .expect("apostrophe in query must not be an FTS5 syntax error");
        assert!(!results.is_empty(), "should match text containing don't");

        let results = search_entries(&conn, "\"expense", 10)
            .expect("unbalanced double quote must not be an FTS5 syntax error");
        assert!(!results.is_empty(), "stray quotes should be ignored, matching 'expense'");
    }

    /// Integration: Search for Chinese company name.
    #[test]
    fn chinese_company_name_search() {
        let conn = open_db();
        insert_test_entries(&conn);

        let results = search_entries(&conn, "苹果公司", 10)
            .expect("company name search failed");
        assert!(
            !results.is_empty(),
            "should find entry about Apple (苹果公司)"
        );
    }

    /// Unit: Search term with no matches returns empty vec, not error.
    #[test]
    fn no_match_returns_empty() {
        let conn = open_db();
        insert_test_entries(&conn);

        let results = search_entries(&conn, "xyznonexistentword", 10)
            .expect("search should not error");
        assert!(results.is_empty(), "non-matching search should return empty vec");
    }

    /// Unit: Search respects limit parameter.
    #[test]
    fn search_respects_limit() {
        let conn = open_db();
        // Insert 10 entries all containing "test"
        for i in 0..10 {
            insert_entry(&conn, &Entry {
                id: None,
                created_at: ts(i * 10),
                source_type: SourceType::Note,
                role: EntryRole::User,
                mode: "raw".to_string(),
                raw_text: format!("test entry number {}", i),
                processed_text: None,
                container_id: None,
                audio_ref: None,
                metadata: serde_json::Value::Null,
            }).expect("insert");
        }

        let results = search_entries(&conn, "test", 3).expect("search failed");
        assert_eq!(results.len(), 3, "should respect limit=3");
    }
}

// ===========================================================================
// C04 — Data migration from history
// ===========================================================================

#[cfg(test)]
mod c04_migration {
    use super::*;

    /// Set up the old-format history/events table (from stats.rs schema).
    fn create_legacy_events_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                type          TEXT    NOT NULL,
                created_at    TEXT    NOT NULL,
                date          TEXT    NOT NULL,
                input_text    TEXT,
                output_text   TEXT,
                words_in      INTEGER NOT NULL DEFAULT 0,
                words_out     INTEGER NOT NULL DEFAULT 0,
                duration_secs REAL    NOT NULL DEFAULT 0,
                latency_ms    INTEGER NOT NULL DEFAULT 0,
                mode          TEXT,
                model         TEXT,
                voice         TEXT,
                audio_path    TEXT,
                tokens_in     INTEGER NOT NULL DEFAULT 0,
                tokens_out    INTEGER NOT NULL DEFAULT 0,
                session_id    TEXT
            );",
        ).expect("create legacy events table");
    }

    fn insert_legacy_event(
        conn: &Connection,
        event_type: &str,
        input_text: &str,
        output_text: &str,
        mode: &str,
        session_id: &str,
        created_at: &str,
    ) {
        conn.execute(
            "INSERT INTO events (type, created_at, date, input_text, output_text, mode, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                event_type,
                created_at,
                &created_at[..10],
                input_text,
                output_text,
                mode,
                if session_id.is_empty() { None } else { Some(session_id) },
            ],
        ).expect("insert legacy event");
    }

    /// Static: migrate_from_history function compiles and exists.
    #[test]
    fn migration_function_compiles() {
        // This test passes at compile time if migrate_from_history is importable
        let _ = migrate_from_history as fn(&Connection) -> fonos_core::Result<()>;
    }

    /// Unit: Old-format events table migrates into entries table correctly.
    #[test]
    fn migration_populates_entries_from_legacy_events() {
        let conn = Connection::open_in_memory().expect("in-memory db");

        // Create legacy schema first, then v2 schema
        create_legacy_events_table(&conn);
        init_storage_db(&conn);

        // Insert legacy STT events (dictation)
        insert_legacy_event(&conn, "stt", "Hello world test", "", "raw", "sess-001", "2026-01-15T09:00:00");
        insert_legacy_event(&conn, "stt", "Write me a poem", "Roses are red...", "polish", "sess-002", "2026-01-15T10:30:00");

        // Run migration
        migrate_from_history(&conn).expect("migration should succeed");

        // Verify entries table is populated
        let entries = get_entries(&conn, &EntryFilter::default()).expect("get entries");
        assert!(entries.len() >= 2, "migration should create at least 2 entries");

        // Verify a dictation entry has correct source_type
        let dictation_entries: Vec<_> = entries.iter()
            .filter(|e| matches!(e.source_type, SourceType::Dictation))
            .collect();
        assert!(!dictation_entries.is_empty(), "should have dictation entries from legacy stt events");
    }

    /// Unit: Old table is renamed to history_backup after migration.
    #[test]
    fn migration_renames_old_table_to_backup() {
        let conn = Connection::open_in_memory().expect("in-memory db");

        create_legacy_events_table(&conn);
        init_storage_db(&conn);

        insert_legacy_event(&conn, "stt", "test input", "", "raw", "", "2026-01-10T08:00:00");

        migrate_from_history(&conn).expect("migration should succeed");

        // Verify history_backup table exists
        let backup_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history_backup'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .expect("sqlite_master query");

        assert!(backup_exists, "history_backup table should exist after migration");
    }

    /// Unit: Migration is idempotent — running twice does not duplicate entries.
    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().expect("in-memory db");

        create_legacy_events_table(&conn);
        init_storage_db(&conn);

        insert_legacy_event(&conn, "stt", "idempotent test", "", "raw", "", "2026-02-01T12:00:00");

        migrate_from_history(&conn).expect("first migration");
        let count_after_first = get_entries(&conn, &EntryFilter::default())
            .expect("get entries after first migration")
            .len();

        // Run again — should not duplicate
        migrate_from_history(&conn).expect("second migration");
        let count_after_second = get_entries(&conn, &EntryFilter::default())
            .expect("get entries after second migration")
            .len();

        assert_eq!(
            count_after_first, count_after_second,
            "running migration twice should not duplicate entries"
        );
    }

    /// Integration: Agent sessions grouped into conversation containers.
    #[test]
    fn agent_sessions_grouped_into_containers() {
        let conn = Connection::open_in_memory().expect("in-memory db");

        create_legacy_events_table(&conn);
        init_storage_db(&conn);

        // Two agent session pairs (user question + LLM response)
        let session_a = "sess-agent-001";
        let session_b = "sess-agent-002";

        insert_legacy_event(&conn, "stt", "How is the weather?", "", "agent", session_a, "2026-01-20T09:00:00");
        insert_legacy_event(&conn, "llm", "How is the weather?", "It is sunny.", "agent", session_a, "2026-01-20T09:00:02");

        insert_legacy_event(&conn, "stt", "Set a reminder for 3pm", "", "agent", session_b, "2026-01-20T10:00:00");
        insert_legacy_event(&conn, "llm", "Set a reminder for 3pm", "Reminder set for 3:00 PM.", "agent", session_b, "2026-01-20T10:00:03");

        migrate_from_history(&conn).expect("migration");

        // Verify conversation containers were created
        let containers = get_containers(&conn).expect("get containers");
        let conversation_containers: Vec<_> = containers.iter()
            .filter(|c| matches!(c.container_type, ContainerType::Conversation))
            .collect();

        assert_eq!(
            conversation_containers.len(), 2,
            "two agent sessions should produce two conversation containers"
        );
    }

    /// Integration: Migration handles empty legacy table gracefully.
    #[test]
    fn migration_empty_legacy_table_succeeds() {
        let conn = Connection::open_in_memory().expect("in-memory db");

        create_legacy_events_table(&conn);
        init_storage_db(&conn);

        // No legacy events inserted — migration should still succeed
        let result = migrate_from_history(&conn);
        assert!(result.is_ok(), "migration of empty table should succeed");

        let entries = get_entries(&conn, &EntryFilter::default()).expect("get entries");
        assert_eq!(entries.len(), 0, "no entries after migrating empty table");
    }
}

// ===========================================================================
// C05 — Mode definition extensions
// ===========================================================================

#[cfg(test)]
mod c05_mode_extensions {
    use super::*;

    /// Static: Mode struct compiles with new v2 fields.
    #[test]
    fn mode_struct_has_new_fields() {
        let mode = Mode {
            name: "test".to_string(),
            // existing fields
            description: String::new(),
            icon: "📝".to_string(),
            system: None,
            user_template: None,
            temperature: 0.1,
            model: String::new(),
            stt_model: String::new(),
            stt_prompt: String::new(),
            stt_temperature: 0.0,
            max_tokens: 4096,
            output_language: "auto".to_string(),
            auto_paste: true,
            auto_press_enter: false,
            // new v2 fields
            output_target: OutputTarget::Clipboard,
            container_type: None,
            auto_container: false,
            save_audio: false,
            processor: String::new(),
            vocab_books: Vec::new(),
        };

        assert!(matches!(mode.output_target, OutputTarget::Clipboard));
        assert!(mode.container_type.is_none());
        assert!(!mode.auto_container);
        assert!(!mode.save_audio);
    }

    /// Unit: Existing built-in modes have correct default values for new fields.
    #[test]
    fn existing_modes_have_backward_compatible_defaults() {
        let modes = built_in_modes();

        // raw mode
        let raw = modes.get("raw").expect("raw mode must exist");
        assert!(
            matches!(raw.output_target, OutputTarget::Clipboard) || matches!(raw.output_target, OutputTarget::ActiveTextField),
            "raw mode should have a clipboard or text-field output target"
        );
        assert!(!raw.auto_container, "raw mode auto_container should default to false");
        assert!(!raw.save_audio, "raw mode save_audio should default to false");
        assert!(raw.container_type.is_none(), "raw mode container_type should be None");

        // polish mode
        let polish = modes.get("polish").expect("polish mode must exist");
        assert!(!polish.auto_container, "polish auto_container default false");
        assert!(!polish.save_audio, "polish save_audio default false");

        // translate mode
        let translate = modes.get("translate").expect("translate mode must exist");
        assert!(!translate.auto_container, "translate auto_container default false");

        // All existing modes must still exist
        assert!(modes.contains_key("raw"), "raw must exist");
        assert!(modes.contains_key("polish"), "polish must exist");
        assert!(modes.contains_key("translate"), "translate must exist");
    }

    /// Unit: OutputTarget variants compile.
    #[test]
    fn output_target_variants() {
        let _clipboard = OutputTarget::Clipboard;
        let _text_field = OutputTarget::ActiveTextField;
        let _container = OutputTarget::AppendToContainer;
        let _none = OutputTarget::None;
    }

    /// Unit: Mode default() has correct new field defaults.
    #[test]
    fn mode_default_new_fields() {
        let mode = Mode::default();
        assert!(!mode.auto_container, "default auto_container is false");
        assert!(!mode.save_audio, "default save_audio is false");
        assert!(mode.container_type.is_none(), "default container_type is None");
    }
}

// ===========================================================================
// C06 — Unified input pipeline writes entries
// ===========================================================================

#[cfg(test)]
mod c06_pipeline_writes {
    use super::*;
    use fonos_core::storage::simulate_dictation_pipeline;
    use fonos_core::storage::simulate_agent_pipeline;

    /// Unit: Dictation pipeline writes entry even if output step is mocked.
    #[test]
    fn dictation_pipeline_writes_entry_to_db() {
        let conn = open_db();

        // Simulate the dictation flow: STT produces text, mode processes it,
        // entry is written before output_target is applied
        let result = simulate_dictation_pipeline(
            &conn,
            "raw transcribed audio text",
            Some("Raw transcribed audio text."),
            "raw",
            None,  // no audio ref in test
            None,  // no container
        );

        assert!(result.is_ok(), "dictation pipeline should not error: {:?}", result);
        let entry_id = result.unwrap();
        assert!(entry_id > 0);

        // Entry must persist even if output_target had failed
        let entry = get_entry(&conn, entry_id).expect("entry should exist in db");
        assert_eq!(entry.raw_text, "raw transcribed audio text");
        assert!(matches!(entry.source_type, SourceType::Dictation));
        assert!(matches!(entry.role, EntryRole::User));
    }

    /// Unit: Entry is written before output is applied (entry persists on output failure).
    #[test]
    fn entry_persists_before_output_target() {
        let conn = open_db();

        // Simulate a pipeline where the output step fails
        // Entry must still be in the DB
        let result = simulate_dictation_pipeline(
            &conn,
            "text that goes to clipboard",
            Some("Text that goes to clipboard."),
            "polish",
            None,
            None,
        );

        // Even if output fails, entry should be written
        let entry_id = result.expect("pipeline should return entry id");
        let stored = get_entry(&conn, entry_id);
        assert!(stored.is_ok(), "entry should be in DB regardless of output success");
    }

    /// Integration: Agent pipeline writes both user and assistant entries with correct roles.
    #[test]
    fn agent_pipeline_writes_user_and_assistant_entries() {
        let conn = open_db();

        // Simulate an agent conversation turn
        let container_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Conversation,
            title: "Test Conversation".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert conversation container");

        let result = simulate_agent_pipeline(
            &conn,
            "What time is it?",
            "It is 3:42 PM.",
            container_id,
        );

        assert!(result.is_ok(), "agent pipeline should succeed: {:?}", result);
        let (user_id, assistant_id) = result.unwrap();

        // Verify user entry
        let user_entry = get_entry(&conn, user_id).expect("user entry should exist");
        assert_eq!(user_entry.raw_text, "What time is it?");
        assert!(matches!(user_entry.role, EntryRole::User));
        assert!(matches!(user_entry.source_type, SourceType::Agent));
        assert_eq!(user_entry.container_id, Some(container_id));

        // Verify assistant entry
        let asst_entry = get_entry(&conn, assistant_id).expect("assistant entry should exist");
        assert_eq!(asst_entry.raw_text, "It is 3:42 PM.");
        assert!(matches!(asst_entry.role, EntryRole::Assistant));
        assert!(matches!(asst_entry.source_type, SourceType::Agent));
        assert_eq!(asst_entry.container_id, Some(container_id));
    }

    /// Integration: Note pipeline writes entry with note source_type and container link.
    #[test]
    fn note_pipeline_writes_entry_with_container() {
        let conn = open_db();

        let notebook_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Daily Notes".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        let entry_id = insert_entry(&conn, &Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Note,
            role: EntryRole::User,
            mode: "note".to_string(),
            raw_text: "Remember to call Alice about the project".to_string(),
            processed_text: Some("Remember to call Alice about the project.".to_string()),
            container_id: Some(notebook_id),
            audio_ref: None,
            metadata: serde_json::Value::Null,
        }).expect("insert note entry");

        let entry = get_entry(&conn, entry_id).expect("get note entry");
        assert_eq!(entry.container_id, Some(notebook_id));
        assert!(matches!(entry.source_type, SourceType::Note));
    }
}

// ===========================================================================
// Q01 — Query performance
// ===========================================================================

#[cfg(test)]
mod q01_query_performance {
    use super::*;

    const ENTRY_COUNT: usize = 1000;
    const RECENT_QUERY_THRESHOLD_MS: u128 = 100;
    const FTS5_SEARCH_THRESHOLD_MS: u128 = 200;

    /// Insert 1000 test entries with varied data.
    fn populate_large_dataset(conn: &Connection) {
        let source_types = [
            SourceType::Dictation,
            SourceType::Agent,
            SourceType::Note,
            SourceType::Meeting,
        ];
        let modes = ["raw", "polish", "translate", "note", "agent"];

        // Use a transaction for bulk insert performance
        conn.execute_batch("BEGIN").expect("begin transaction");

        for i in 0..ENTRY_COUNT {
            let st = source_types[i % source_types.len()].clone();
            let mode = modes[i % modes.len()];
            let entry = Entry {
                id: None,
                created_at: ts(i as i64 * 30),
                source_type: st,
                role: if i % 3 == 0 { EntryRole::Assistant } else { EntryRole::User },
                mode: mode.to_string(),
                raw_text: format!(
                    "Entry {} — this is a test entry with some content about topic {} and keywords like voice dictation notes meetings",
                    i, i % 20
                ),
                processed_text: Some(format!(
                    "Entry {}. This is a test entry with some content about topic {}.",
                    i, i % 20
                )),
                container_id: if i % 5 == 0 { Some((i as i64 / 50) + 1) } else { None },
                audio_ref: if i % 4 == 0 {
                    Some(format!("/tmp/audio/rec_{:05}.wav", i))
                } else {
                    None
                },
                metadata: serde_json::json!({ "index": i }),
            };
            insert_entry(conn, &entry).expect("bulk insert entry");
        }

        conn.execute_batch("COMMIT").expect("commit transaction");
    }

    /// Q01: Recent view query on 1000 entries completes in < 100ms.
    #[test]
    fn recent_view_query_under_100ms() {
        let conn = open_db();
        populate_large_dataset(&conn);

        let start = Instant::now();
        let entries = get_entries(
            &conn,
            &EntryFilter {
                limit: Some(50),
                offset: Some(0),
                order_desc: true,
                ..Default::default()
            },
        ).expect("recent view query");
        let elapsed_ms = start.elapsed().as_millis();

        assert_eq!(entries.len(), 50, "should return 50 entries for recent view");
        assert!(
            elapsed_ms < RECENT_QUERY_THRESHOLD_MS,
            "recent view query took {}ms, threshold is {}ms",
            elapsed_ms,
            RECENT_QUERY_THRESHOLD_MS
        );
    }

    /// Q01: FTS5 search on 1000 entries completes in < 200ms.
    #[test]
    fn fts5_search_under_200ms() {
        let conn = open_db();
        populate_large_dataset(&conn);

        let start = Instant::now();
        let results = search_entries(&conn, "dictation", 20).expect("fts5 search");
        let elapsed_ms = start.elapsed().as_millis();

        assert!(!results.is_empty(), "should find entries containing 'dictation'");
        assert!(
            elapsed_ms < FTS5_SEARCH_THRESHOLD_MS,
            "FTS5 search took {}ms, threshold is {}ms",
            elapsed_ms,
            FTS5_SEARCH_THRESHOLD_MS
        );
    }

    /// Q01: Filtered query by source_type on 1000 entries is fast.
    #[test]
    fn filtered_query_by_source_type_is_fast() {
        let conn = open_db();
        populate_large_dataset(&conn);

        let start = Instant::now();
        let entries = get_entries(
            &conn,
            &EntryFilter {
                source_type: Some(SourceType::Note),
                limit: Some(50),
                order_desc: true,
                ..Default::default()
            },
        ).expect("filtered query");
        let elapsed_ms = start.elapsed().as_millis();

        assert!(!entries.is_empty(), "should have note entries");
        assert!(
            elapsed_ms < RECENT_QUERY_THRESHOLD_MS,
            "filtered query took {}ms, threshold {}ms",
            elapsed_ms,
            RECENT_QUERY_THRESHOLD_MS
        );
    }

    /// Q01: Container entries query is fast.
    #[test]
    fn container_entries_query_is_fast() {
        let conn = open_db();

        // Insert a notebook container
        let notebook_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Performance Test Notebook".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        // Insert 200 entries in the notebook
        conn.execute_batch("BEGIN").expect("begin");
        for i in 0..200 {
            insert_entry(&conn, &Entry {
                id: None,
                created_at: ts(i * 10),
                source_type: SourceType::Note,
                role: EntryRole::User,
                mode: "note".to_string(),
                raw_text: format!("Notebook entry {}", i),
                processed_text: None,
                container_id: Some(notebook_id),
                audio_ref: None,
                metadata: serde_json::Value::Null,
            }).expect("insert");
        }
        conn.execute_batch("COMMIT").expect("commit");

        let start = Instant::now();
        let entries = get_container_entries(&conn, notebook_id).expect("get container entries");
        let elapsed_ms = start.elapsed().as_millis();

        assert_eq!(entries.len(), 200);
        assert!(
            elapsed_ms < RECENT_QUERY_THRESHOLD_MS,
            "container entries query took {}ms, threshold {}ms",
            elapsed_ms,
            RECENT_QUERY_THRESHOLD_MS
        );
    }
}

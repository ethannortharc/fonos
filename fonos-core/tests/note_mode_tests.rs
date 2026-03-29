/// Note mode tests for Fonos v2 — C07, C10, C14
///
/// Covers:
///   C07 — Note mode configuration (processor, output_target, container_type,
///          auto_container, save_audio fields)
///   C10 — Note hotkey (Option+N) — hotkey parsing and config field presence
///   C14 — Export notebook as Markdown and JSON
///
/// Run with:
///   cargo test -p fonos-core --test note_mode_tests
///
/// These tests are in the RED (failing) phase — the production code
/// (note mode definition and export functions) does not exist yet.

use fonos_core::modes::{built_in_modes, OutputTarget, ContainerKind};
use fonos_core::config::AppConfig;
use fonos_core::storage::{
    init_storage_db,
    insert_entry,
    insert_container,
    export_notebook_markdown,
    export_notebook_json,
    Entry,
    EntryRole,
    SourceType,
    Container,
    ContainerType,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn open_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    init_storage_db(&conn);
    conn
}

fn ts(offset_secs: i64) -> String {
    let epoch: i64 = 1_774_785_600 + offset_secs;
    let h = (epoch % 86400) / 3600;
    let m = (epoch % 3600) / 60;
    let s = epoch % 60;
    format!("2026-03-26T{:02}:{:02}:{:02}", h, m, s)
}

// ===========================================================================
// C07 — Note mode configuration
// ===========================================================================

#[cfg(test)]
mod c07_note_mode_config {
    use super::*;

    /// Unit: Note mode exists in built_in_modes().
    #[test]
    fn note_mode_exists_in_built_in_modes() {
        let modes = built_in_modes();
        assert!(
            modes.contains_key("note"),
            "built_in_modes() must contain a 'note' key"
        );
    }

    /// Unit: Note mode has output_target = AppendToContainer.
    #[test]
    fn note_mode_output_target_is_append_to_container() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        assert!(
            matches!(note.output_target, OutputTarget::AppendToContainer),
            "note mode output_target should be AppendToContainer, got {:?}",
            note.output_target
        );
    }

    /// Unit: Note mode has container_type = Notebook.
    #[test]
    fn note_mode_container_type_is_notebook() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        assert!(
            matches!(note.container_type, Some(ContainerKind::Notebook)),
            "note mode container_type should be Some(Notebook), got {:?}",
            note.container_type
        );
    }

    /// Unit: Note mode has auto_container = true (pick mode).
    #[test]
    fn note_mode_auto_container_is_true() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        assert!(
            note.auto_container,
            "note mode auto_container should be true (pick mode)"
        );
    }

    /// Unit: Note mode processor field is "light_polish".
    #[test]
    fn note_mode_processor_is_light_polish() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        // processor is either a dedicated field or expressed via user_template/system
        // The spec says processor=light_polish — verify the mode name/description
        // reflects this, OR that a dedicated `processor` field is set
        assert!(
            note.name.to_lowercase().contains("note") || !note.description.is_empty(),
            "note mode should have a meaningful name and description"
        );
        // Verify the processor field if it exists
        // If Mode has a `processor` field:
        assert_eq!(
            note.processor, "light_polish",
            "note mode processor should be 'light_polish'"
        );
    }

    /// Unit: Note mode save_audio field is configurable (default value exists).
    #[test]
    fn note_mode_save_audio_field_present() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        // save_audio should be a bool field; default value doesn't matter for compilation
        let _save_audio: bool = note.save_audio;
    }

    /// Unit: Note mode does not interfere with existing mode configs.
    #[test]
    fn note_mode_does_not_shadow_existing_modes() {
        let modes = built_in_modes();

        // All pre-existing modes must still be present and unchanged
        let raw = modes.get("raw").expect("raw must exist");
        assert_eq!(raw.name, "Raw", "raw mode name unchanged");
        assert!(!raw.auto_container, "raw mode auto_container still false");
        assert!(
            matches!(raw.output_target, OutputTarget::Clipboard) ||
            matches!(raw.output_target, OutputTarget::ActiveTextField),
            "raw mode output target unchanged"
        );

        let polish = modes.get("polish").expect("polish must exist");
        assert_eq!(polish.name, "Polish", "polish mode name unchanged");

        let translate = modes.get("translate").expect("translate must exist");
        assert_eq!(translate.name, "Translate", "translate mode name unchanged");
    }

    /// Unit: Note mode icon is set to a non-empty string.
    #[test]
    fn note_mode_has_icon() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        assert!(!note.icon.is_empty(), "note mode icon should not be empty");
    }

    /// Unit: Note mode description is non-empty.
    #[test]
    fn note_mode_has_description() {
        let modes = built_in_modes();
        let note = modes.get("note").expect("note mode must exist");
        assert!(!note.description.is_empty(), "note mode should have a description");
    }
}

// ===========================================================================
// C10 — Note hotkey (Option+N)
// ===========================================================================

#[cfg(test)]
mod c10_note_hotkey {
    use super::*;
    use fonos_core::hotkey::parse_hotkey;

    /// Unit: Hotkey parsing succeeds for the default note hotkey "option+n".
    #[test]
    fn note_hotkey_parses_successfully() {
        let result = parse_hotkey("option+n");
        assert!(
            result.is_ok(),
            "parse_hotkey('option+n') should succeed, got: {:?}",
            result
        );
    }

    /// Unit: Note hotkey config field exists in AppConfig.
    #[test]
    fn app_config_has_hotkey_note_field() {
        let config = AppConfig::default();
        // hotkey_note field must exist; accessing it should compile
        let hotkey: &str = &config.hotkey_note;
        assert!(!hotkey.is_empty(), "default hotkey_note should not be empty");
    }

    /// Unit: Default note hotkey is "option+n".
    #[test]
    fn default_note_hotkey_is_option_n() {
        let config = AppConfig::default();
        assert_eq!(
            config.hotkey_note, "option+n",
            "default note hotkey should be 'option+n'"
        );
    }

    /// Unit: Note hotkey does not conflict with existing hotkeys.
    #[test]
    fn note_hotkey_does_not_conflict() {
        let config = AppConfig::default();
        let existing_hotkeys = [
            &config.hotkey_dictation,
            &config.hotkey_tts,
            &config.hotkey_agent,
            &config.hotkey_agent_panel,
        ];

        for existing in &existing_hotkeys {
            assert_ne!(
                config.hotkey_note.as_str(),
                existing.as_str(),
                "note hotkey '{}' conflicts with existing hotkey '{}'",
                config.hotkey_note,
                existing
            );
        }
    }

    /// Unit: Common alternative hotkey values for note can also be parsed.
    #[test]
    fn note_hotkey_alt_values_parse() {
        let variants = ["option+n", "opt+n", "alt+n", "cmd+shift+n"];
        for variant in &variants {
            let result = parse_hotkey(variant);
            assert!(
                result.is_ok(),
                "parse_hotkey('{}') should succeed",
                variant
            );
        }
    }

    /// Integration: Config includes hotkey_note field when serialized to JSON.
    #[test]
    fn config_serializes_hotkey_note() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).expect("serialize config");
        assert!(
            json.contains("hotkey_note"),
            "serialized config JSON should contain 'hotkey_note'"
        );
    }

    /// Integration: Config round-trips hotkey_note field through JSON.
    #[test]
    fn config_hotkey_note_round_trips() {
        let mut config = AppConfig::default();
        config.hotkey_note = "cmd+shift+n".to_string();

        let json = serde_json::to_string(&config).expect("serialize");
        let loaded: AppConfig = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(loaded.hotkey_note, "cmd+shift+n");
    }
}

// ===========================================================================
// C14 — Export notebook
// ===========================================================================

#[cfg(test)]
mod c14_export_notebook {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;

    /// Build a notebook with entries in the in-memory DB, return the notebook id.
    fn create_test_notebook(conn: &Connection) -> i64 {
        let notebook_id = insert_container(conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Field Notes".to_string(),
            parent_id: None,
            created_at: "2026-03-26T09:00:00".to_string(),
            updated_at: "2026-03-26T18:00:00".to_string(),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        let notes = [
            ("2026-03-26T09:15:00", "First observation at the site.", "First observation at the site."),
            ("2026-03-26T11:30:00", "um so the second thing we noticed was the light changes", "The second thing we noticed was the light changes."),
            ("2026-03-26T14:00:00", "天空中有很多云，风向是北风", "天空中有很多云，风向是北风。"),
            ("2026-03-26T16:45:00", "Final notes before end of day wrap-up.", "Final notes before end of day wrap-up."),
        ];

        for (created_at, raw, processed) in &notes {
            insert_entry(conn, &Entry {
                id: None,
                created_at: created_at.to_string(),
                source_type: SourceType::Note,
                role: EntryRole::User,
                mode: "note".to_string(),
                raw_text: raw.to_string(),
                processed_text: Some(processed.to_string()),
                container_id: Some(notebook_id),
                audio_ref: None,
                metadata: serde_json::Value::Null,
            }).expect("insert note entry");
        }

        notebook_id
    }

    /// Unit: Export function produces correct markdown from test data.
    #[test]
    fn export_markdown_produces_valid_content() {
        let conn = open_db();
        let notebook_id = create_test_notebook(&conn);
        let tmp = TempDir::new().expect("temp dir");

        let output_path = export_notebook_markdown(&conn, notebook_id, tmp.path())
            .expect("export_notebook_markdown failed");

        assert!(output_path.exists(), "output path should exist after export");

        let readme_path = output_path.join("README.md");
        assert!(readme_path.exists(), "README.md should exist");

        let readme_content = std::fs::read_to_string(&readme_path)
            .expect("read README.md");

        // Verify notebook title is in the README
        assert!(
            readme_content.contains("Field Notes"),
            "README.md should contain notebook title 'Field Notes', got:\n{}",
            readme_content
        );
    }

    /// Unit: Exported markdown contains all entries.
    #[test]
    fn export_markdown_contains_all_entries() {
        let conn = open_db();
        let notebook_id = create_test_notebook(&conn);
        let tmp = TempDir::new().expect("temp dir");

        let output_path = export_notebook_markdown(&conn, notebook_id, tmp.path())
            .expect("export markdown");

        // Walk all .md files in output and accumulate content
        let mut all_content = String::new();
        for entry in std::fs::read_dir(&output_path).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                all_content.push_str(
                    &std::fs::read_to_string(&path).expect("read md file")
                );
            }
        }

        // All entry texts should appear somewhere in the exported markdown
        assert!(
            all_content.contains("First observation at the site"),
            "export should contain first entry text"
        );
        assert!(
            all_content.contains("light changes"),
            "export should contain second entry text"
        );
        assert!(
            all_content.contains("天空中有很多云"),
            "export should contain Chinese entry text"
        );
        assert!(
            all_content.contains("Final notes before end of day"),
            "export should contain final entry text"
        );
    }

    /// Unit: Exported markdown uses entries organized by date.
    #[test]
    fn export_markdown_organized_by_date() {
        let conn = open_db();
        let notebook_id = create_test_notebook(&conn);
        let tmp = TempDir::new().expect("temp dir");

        let output_path = export_notebook_markdown(&conn, notebook_id, tmp.path())
            .expect("export markdown");

        // The output folder should contain a date-based file or README with date headers
        let entries: Vec<_> = std::fs::read_dir(&output_path)
            .expect("read output dir")
            .filter_map(|e| e.ok())
            .collect();

        assert!(!entries.is_empty(), "export output should contain at least one file");

        // Either a README.md with date sections, or date-named .md files
        let has_readme = entries.iter().any(|e| e.file_name() == "README.md");
        let has_date_files = entries.iter().any(|e| {
            e.file_name().to_string_lossy().starts_with("2026")
        });
        assert!(
            has_readme || has_date_files,
            "export should have README.md or date-organized files"
        );
    }

    /// Unit: Export JSON produces valid JSON with correct structure.
    #[test]
    fn export_json_produces_valid_json() {
        let conn = open_db();
        let notebook_id = create_test_notebook(&conn);
        let tmp = TempDir::new().expect("temp dir");

        let json_path = export_notebook_json(&conn, notebook_id, tmp.path())
            .expect("export_notebook_json failed");

        assert!(json_path.exists(), "JSON export file should exist");

        let content = std::fs::read_to_string(&json_path).expect("read json file");
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .expect("exported JSON should be valid");

        // Top-level object should have notebook title and entries array
        assert!(
            parsed.get("title").is_some(),
            "JSON export should have 'title' field"
        );
        assert!(
            parsed.get("entries").is_some(),
            "JSON export should have 'entries' array"
        );

        let entries = parsed["entries"].as_array().expect("entries should be array");
        assert_eq!(entries.len(), 4, "JSON export should have 4 entries");
    }

    /// Unit: JSON export contains required entry fields.
    #[test]
    fn export_json_entries_have_required_fields() {
        let conn = open_db();
        let notebook_id = create_test_notebook(&conn);
        let tmp = TempDir::new().expect("temp dir");

        let json_path = export_notebook_json(&conn, notebook_id, tmp.path())
            .expect("export json");

        let content = std::fs::read_to_string(&json_path).expect("read json");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");

        let entries = parsed["entries"].as_array().expect("entries array");
        let first = &entries[0];

        // Each entry should have: created_at, raw_text (or text), processed_text
        assert!(
            first.get("created_at").is_some(),
            "entry should have created_at field"
        );
        assert!(
            first.get("raw_text").or_else(|| first.get("text")).is_some(),
            "entry should have raw_text or text field"
        );
    }

    /// Integration: Notebook with audio refs — audio paths included in export.
    #[test]
    fn export_includes_audio_refs_when_present() {
        let conn = open_db();

        let notebook_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Audio Notes".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        insert_entry(&conn, &Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Note,
            role: EntryRole::User,
            mode: "note".to_string(),
            raw_text: "Note with audio attached".to_string(),
            processed_text: Some("Note with audio attached.".to_string()),
            container_id: Some(notebook_id),
            audio_ref: Some("/tmp/audio/note_001.wav".to_string()),
            metadata: serde_json::Value::Null,
        }).expect("insert entry with audio");

        let tmp = TempDir::new().expect("temp dir");

        // JSON export should reference the audio file
        let json_path = export_notebook_json(&conn, notebook_id, tmp.path())
            .expect("export json with audio");

        let content = std::fs::read_to_string(&json_path).expect("read json");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");
        let entries = parsed["entries"].as_array().expect("entries");
        let first = &entries[0];

        assert!(
            first.get("audio_ref").is_some(),
            "JSON export entry should include audio_ref when present"
        );
    }

    /// Unit: Export function returns error for non-existent notebook id.
    #[test]
    fn export_nonexistent_notebook_returns_error() {
        let conn = open_db();
        let tmp = TempDir::new().expect("temp dir");

        let result = export_notebook_markdown(&conn, 99999, tmp.path());
        assert!(
            result.is_err(),
            "export of non-existent notebook should return an error"
        );
    }

    /// Unit: Markdown export uses processed_text when available, falls back to raw_text.
    #[test]
    fn export_markdown_uses_processed_text_preferentially() {
        let conn = open_db();

        let notebook_id = insert_container(&conn, &Container {
            id: None,
            container_type: ContainerType::Notebook,
            title: "Processed Notes".to_string(),
            parent_id: None,
            created_at: ts(0),
            updated_at: ts(0),
            metadata: serde_json::Value::Null,
        }).expect("insert notebook");

        insert_entry(&conn, &Entry {
            id: None,
            created_at: ts(0),
            source_type: SourceType::Note,
            role: EntryRole::User,
            mode: "note".to_string(),
            raw_text: "um so i wanted to write about the project".to_string(),
            processed_text: Some("I wanted to write about the project.".to_string()),
            container_id: Some(notebook_id),
            audio_ref: None,
            metadata: serde_json::Value::Null,
        }).expect("insert entry");

        let tmp = TempDir::new().expect("temp dir");
        let output_path = export_notebook_markdown(&conn, notebook_id, tmp.path())
            .expect("export markdown");

        let readme = std::fs::read_to_string(output_path.join("README.md"))
            .expect("read README");

        // Processed text should appear, NOT raw filler-word version
        assert!(
            readme.contains("I wanted to write about the project"),
            "README should use processed_text"
        );
    }
}

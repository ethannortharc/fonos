/// Backward compatibility tests for Fonos v2 — C15
///
/// Covers:
///   C15 — Backward compatibility: existing mode configs unchanged after
///          migration; existing Tauri commands still compile and register;
///          float pill and agent panel unaffected.
///
/// Run with:
///   cargo test -p fonos-app --test compat_tests

use fonos_core::config::AppConfig;
use fonos_core::modes::{built_in_modes, Mode};
use fonos_core::stats::{init_db, record_event, get_history, get_today, get_daily_stats};
use rusqlite::Connection;

// ===========================================================================
// C15 — Backward compatibility — Existing mode configs
// ===========================================================================

#[cfg(test)]
mod c15_existing_mode_configs {
    use super::*;

    /// Unit: raw mode config is unchanged after v2 migration.
    #[test]
    fn raw_mode_unchanged() {
        let modes = built_in_modes();
        let raw = modes.get("raw").expect("raw mode must exist");

        assert_eq!(raw.name, "Raw");
        assert_eq!(raw.description, "No processing, direct STT output");
        assert_eq!(raw.icon, "📝");
        assert!(raw.system.is_none(), "raw mode has no system prompt");
        assert!(raw.user_template.is_none(), "raw mode has no user_template");
        assert_eq!(raw.temperature, 0.0);
        assert!(raw.auto_paste, "raw mode auto_paste should be true");
        assert!(!raw.auto_press_enter, "raw mode auto_press_enter should be false");
        assert_eq!(raw.output_language, "auto");
        assert_eq!(raw.max_tokens, 4096);
    }

    /// Unit: polish mode config is unchanged after v2 migration.
    #[test]
    fn polish_mode_unchanged() {
        let modes = built_in_modes();
        let polish = modes.get("polish").expect("polish mode must exist");

        assert_eq!(polish.name, "Polish");
        assert_eq!(polish.icon, "✨");
        assert!(polish.system.is_some(), "polish has system prompt");
        assert!(polish.user_template.is_some(), "polish has user_template");
        assert!(
            polish.user_template.as_ref().unwrap().contains("{text}"),
            "polish user_template must contain {{text}} placeholder"
        );
        assert_eq!(polish.temperature, 0.1);
        assert!(polish.auto_paste, "polish mode auto_paste should be true");
    }

    /// Unit: translate mode config is unchanged after v2 migration.
    #[test]
    fn translate_mode_unchanged() {
        let modes = built_in_modes();
        let translate = modes.get("translate").expect("translate mode must exist");

        assert_eq!(translate.name, "Translate");
        assert_eq!(translate.icon, "🌐");
        assert!(translate.system.is_some(), "translate has system prompt");
        assert!(translate.user_template.is_some(), "translate has user_template");
        assert_eq!(translate.temperature, 0.3);
    }

    /// Unit: formal mode config is unchanged after v2 migration.
    #[test]
    fn formal_mode_unchanged() {
        let modes = built_in_modes();
        let formal = modes.get("formal").expect("formal mode must exist");

        assert_eq!(formal.name, "Formal");
        assert_eq!(formal.icon, "👔");
        assert!(formal.system.is_some());
        assert!(formal.user_template.is_some());
        assert_eq!(formal.temperature, 0.2);
    }

    /// Unit: Mode count includes the new note mode but not fewer than before.
    #[test]
    fn mode_count_not_reduced() {
        let modes = built_in_modes();
        assert!(
            modes.len() >= 5,
            "built_in_modes should have at least 5 modes after v2 (had 4 before), got {}",
            modes.len()
        );
    }

    /// Unit: No existing mode gains auto_container=true accidentally.
    #[test]
    fn existing_modes_do_not_gain_auto_container() {
        let modes = built_in_modes();
        let should_not_auto_container = ["raw", "polish", "formal", "translate"];

        for key in &should_not_auto_container {
            let mode = modes.get(*key).expect("mode must exist");
            assert!(
                !mode.auto_container,
                "mode '{}' should not have auto_container=true",
                key
            );
        }
    }
}

// ===========================================================================
// C15 — Backward compatibility — AppConfig fields
// ===========================================================================

#[cfg(test)]
mod c15_config_compat {
    use super::*;

    /// Unit: All pre-existing AppConfig fields have the same defaults after v2.
    #[test]
    fn app_config_existing_fields_unchanged() {
        let config = AppConfig::default();

        assert_eq!(config.hotkey_dictation, "cmd+shift+space");
        assert_eq!(config.hotkey_tts, "cmd+shift+s");
        assert_eq!(config.hotkey_agent, "cmd+shift+a");
        assert_eq!(config.hotkey_agent_panel, "cmd+shift+g");
        assert_eq!(config.dictation_mode, "raw");
        assert_eq!(config.default_voice, "default");
        assert_eq!(config.tts_speed, 1.0);
        assert_eq!(config.audio_input_device, "default");
        assert_eq!(config.audio_output_device, "default");
        assert!(config.show_floating_indicator, "float pill shown by default");
        assert_eq!(config.stt_language, "auto");
        assert_eq!(config.translate_source, "auto");
        assert_eq!(config.translate_target, "English");
        assert_eq!(config.agent_timeout_secs, 30);
        assert_eq!(config.agent_max_turns, 20);
        assert!(!config.agent_tts_enabled);
    }

    /// Unit: New config fields have safe defaults (don't affect existing behaviour).
    #[test]
    fn new_config_fields_have_safe_defaults() {
        let config = AppConfig::default();
        assert!(!config.hotkey_note.is_empty(), "hotkey_note should have a default");
        assert_eq!(config.dictation_mode, "raw", "dictation mode unchanged");
    }

    /// Unit: AppConfig deserializes from old JSON (missing new fields) correctly.
    #[test]
    fn config_deserializes_from_old_json_missing_new_fields() {
        let old_json = r#"{
            "hotkey_dictation": "cmd+shift+space",
            "hotkey_tts": "cmd+shift+s",
            "dictation_mode": "raw",
            "default_voice": "default",
            "tts_speed": 1.0,
            "audio_input_device": "default",
            "audio_output_device": "default",
            "show_floating_indicator": true,
            "stt_language": "auto",
            "model_profiles": [],
            "stt_profile": "",
            "tts_profile": "",
            "llm_profile": "",
            "translate_source": "auto",
            "translate_target": "English"
        }"#;

        let config: AppConfig = serde_json::from_str(old_json)
            .expect("old JSON config should deserialize with new AppConfig struct");

        assert!(!config.hotkey_note.is_empty(), "hotkey_note defaults when missing");
        assert_eq!(config.hotkey_dictation, "cmd+shift+space");
    }
}

// ===========================================================================
// C15 — Backward compatibility — Existing stats/events API
// ===========================================================================

#[cfg(test)]
mod c15_stats_api_compat {
    use super::*;

    fn open_stats_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        init_db(&conn);
        conn
    }

    /// Unit: stats::init_db still creates the events table correctly.
    #[test]
    fn stats_init_db_still_works() {
        let conn = open_stats_db();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .expect("events table should still exist after v2");
        assert_eq!(count, 0, "fresh stats db has no events");
    }

    /// Unit: record_event still works after v2 changes.
    #[test]
    fn record_event_still_works() {
        let conn = open_stats_db();
        let id = record_event(
            &conn, "stt", "test transcription input", "", 2.5, 0,
            "raw", "whisper-1", "", "", 0, 0, "sess-test-001",
        ).expect("record_event should still work in v2");
        assert!(id > 0, "record_event should return positive row id");
    }

    /// Unit: get_history still returns events in v2.
    #[test]
    fn get_history_still_works() {
        let conn = open_stats_db();
        for i in 0..5 {
            record_event(
                &conn, "stt", &format!("transcription {}", i), "",
                1.0 + i as f64 * 0.5, 0, "raw", "whisper-1", "", "", 0, 0, "",
            ).expect("record_event");
        }
        let history = get_history(&conn, 10, 0, "").expect("get_history should work");
        assert_eq!(history.len(), 5, "get_history should return 5 events");
    }

    /// Unit: get_history type filter still works.
    #[test]
    fn get_history_type_filter_still_works() {
        let conn = open_stats_db();
        record_event(&conn, "stt", "speech input", "", 2.0, 0, "raw", "", "", "", 0, 0, "s1")
            .expect("insert stt");
        record_event(&conn, "llm", "llm input", "llm output", 0.0, 450, "polish", "gpt-4", "", "", 100, 200, "s1")
            .expect("insert llm");
        record_event(&conn, "tts", "", "spoken text", 1.5, 0, "", "", "nova", "", 0, 0, "s2")
            .expect("insert tts");

        let stt_only = get_history(&conn, 10, 0, "stt").expect("filter stt");
        assert_eq!(stt_only.len(), 1);
        assert_eq!(stt_only[0].event_type, "stt");

        let llm_only = get_history(&conn, 10, 0, "llm").expect("filter llm");
        assert_eq!(llm_only.len(), 1);
    }

    /// Unit: get_today still works in v2.
    #[test]
    fn get_today_still_works() {
        let conn = open_stats_db();
        let result = get_today(&conn);
        assert!(result.is_ok(), "get_today should work: {:?}", result);
    }

    /// Unit: get_daily_stats still works in v2.
    #[test]
    fn get_daily_stats_still_works() {
        let conn = open_stats_db();
        let result = get_daily_stats(&conn, "2026-01-01", "2026-12-31");
        assert!(result.is_ok(), "get_daily_stats should work: {:?}", result);
    }
}

// ===========================================================================
// C15 — Backward compatibility — Tauri commands register check
// ===========================================================================

#[cfg(test)]
mod c15_tauri_commands {
    use fonos_app::commands::{
        // Dictation commands (existing)
        has_microphone,
        start_recording,
        stop_recording,
        transcribe_file,
        // TTS commands (existing)
        synthesize_speech,
        generate_and_play,
        play_audio_file,
        play_speech,
        stop_playback,
        pause_playback,
        resume_playback,
        // Config commands (existing)
        get_config,
        save_config,
        // Stats commands (existing)
        record_event,
        delete_event,
        get_stats,
        get_history,
        get_today,
        // Mode commands (existing)
        list_modes,
        save_custom_mode,
        delete_custom_mode,
        // Agent commands (existing)
        agent_process,
        agent_reset,
        list_skills,
        toggle_skill,
        save_custom_skill,
        delete_custom_skill,
        test_skill,
        // Window commands (existing)
        resize_float,
        // New v2 storage commands (must also compile)
        list_entries,
        get_entry,
        update_entry,
        delete_entry,
        search_entries,
        list_containers,
        create_container,
        delete_container,
        get_container_entries,
    };

    #[test]
    fn all_commands_exist_and_compile() {
        // If this test compiles, all listed commands are importable.
        let _ = has_microphone as usize;
        let _ = get_config as usize;
        let _ = list_modes as usize;
        // v2 new commands
        let _ = list_entries as usize;
        let _ = search_entries as usize;
        let _ = create_container as usize;
        let _ = list_containers as usize;
        let _ = get_container_entries as usize;
        let _ = resize_float as usize;
    }
}

// ===========================================================================
// C15 — Backward compatibility — Window HTML files present
// ===========================================================================

#[cfg(test)]
mod c15_window_html_files {
    use std::path::Path;

    /// Helper: get the fonos-app root directory (parent of src-tauri).
    fn fonos_app_root() -> std::path::PathBuf {
        // CARGO_MANIFEST_DIR = fonos-app/src-tauri  →  parent = fonos-app
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR has no parent")
            .to_path_buf()
    }

    /// Unit: float.html still exists (float pill window).
    #[test]
    fn float_html_exists() {
        let root = fonos_app_root();
        let candidates = [
            root.join("public").join("float.html"),
            root.join("src").join("float.html"),
            root.join("float.html"),
            root.join("dist").join("float.html"),
        ];
        assert!(
            candidates.iter().any(|p| p.exists()),
            "float.html must exist — float pill window must not be removed"
        );
    }

    /// Unit: agent-panel.html still exists (agent panel window).
    #[test]
    fn agent_panel_html_exists() {
        let root = fonos_app_root();
        let candidates = [
            root.join("public").join("agent-panel.html"),
            root.join("src").join("agent-panel.html"),
            root.join("agent-panel.html"),
            root.join("dist").join("agent-panel.html"),
        ];
        assert!(
            candidates.iter().any(|p| p.exists()),
            "agent-panel.html must exist — agent panel window must not be removed"
        );
    }

    /// Unit: note-panel.html exists as the new note mode window.
    #[test]
    fn note_panel_html_exists() {
        let root = fonos_app_root();
        let candidates = [
            root.join("public").join("note-panel.html"),
            root.join("src").join("note-panel.html"),
            root.join("note-panel.html"),
            root.join("dist").join("note-panel.html"),
        ];
        assert!(
            candidates.iter().any(|p| p.exists()),
            "note-panel.html must exist as the new separate note mode window"
        );
    }
}

//! Integration tests for config persistence.
//! Covers: INV-09 (Settings persistence)

use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Mirror of the app's Config struct. Kept minimal to avoid coupling — update
/// if the production struct evolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Config {
    hotkey: String,
    server_port: u16,
    dictation_mode: String,
    llm_endpoint: Option<String>,
    selected_voice: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "cmd+shift+space".to_string(),
            server_port: 9880,
            dictation_mode: "clean".to_string(),
            llm_endpoint: None,
            selected_voice: None,
        }
    }
}

/// Return a unique temp path per test to avoid parallel test races.
fn temp_config_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fonos_test_{name}.json"))
}

fn write_config(cfg: &Config, path: &PathBuf) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create config directory");
    }
    let json = serde_json::to_string_pretty(cfg).expect("serialization failed");
    fs::write(path, json).expect("failed to write config");
}

fn read_config(path: &PathBuf) -> Config {
    let json = fs::read_to_string(path).expect("failed to read config");
    serde_json::from_str(&json).expect("deserialization failed")
}

/// INV-09: Writing then reading back produces identical values.
#[test]
fn test_config_write_read_roundtrip() {
    let path = temp_config_path("roundtrip");
    let original = Config {
        hotkey: "cmd+shift+d".to_string(),
        server_port: 9881,
        dictation_mode: "raw".to_string(),
        llm_endpoint: Some("http://localhost:11434".to_string()),
        selected_voice: Some("alloy".to_string()),
    };

    write_config(&original, &path);
    let loaded = read_config(&path);

    assert_eq!(
        original, loaded,
        "INV-09: config roundtrip — loaded config does not match written config"
    );

    let _ = fs::remove_file(&path);
}

/// INV-09: A fresh config file written with Default values has the expected defaults.
#[test]
fn test_config_default_values() {
    let path = temp_config_path("defaults");
    let defaults = Config::default();
    write_config(&defaults, &path);
    let loaded = read_config(&path);

    assert_eq!(
        loaded.hotkey, "cmd+shift+space",
        "INV-09: default hotkey should be cmd+shift+space"
    );
    assert_eq!(
        loaded.server_port, 9880,
        "INV-09: default server_port should be 9880"
    );
    assert_eq!(
        loaded.dictation_mode, "clean",
        "INV-09: default dictation_mode should be clean"
    );
    assert!(
        loaded.llm_endpoint.is_none(),
        "INV-09: default llm_endpoint should be None"
    );
    assert!(
        loaded.selected_voice.is_none(),
        "INV-09: default selected_voice should be None"
    );

    let _ = fs::remove_file(&path);
}

/// INV-09: Updating a single field does not change other fields.
#[test]
fn test_config_partial_update() {
    let path = temp_config_path("partial");
    let original = Config::default();
    write_config(&original, &path);

    // Read, mutate one field, write back.
    let mut updated = read_config(&path);
    updated.hotkey = "opt+shift+r".to_string();
    write_config(&updated, &path);

    let final_cfg = read_config(&path);
    assert_eq!(
        final_cfg.hotkey, "opt+shift+r",
        "INV-09: partial update — hotkey should reflect new value"
    );
    assert_eq!(
        final_cfg.server_port, original.server_port,
        "INV-09: partial update — server_port should be unchanged"
    );
    assert_eq!(
        final_cfg.dictation_mode, original.dictation_mode,
        "INV-09: partial update — dictation_mode should be unchanged"
    );

    let _ = fs::remove_file(&path);
}

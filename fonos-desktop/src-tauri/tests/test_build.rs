//! Build gate tests.
//! Covers: INV-01 (App launches — Cargo.toml and tauri.conf.json parse correctly)
//!
//! Note: compilation itself is implicitly validated by running `cargo test` at
//! all. These tests provide explicit artifact-level checks of the config files.

use std::path::Path;

/// The src-tauri directory, wherever the checkout lives — never a hardcoded
/// absolute path (those pass locally and break on CI).
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

// ---------------------------------------------------------------------------
// INV-01 Level 1: Static config file validation
// ---------------------------------------------------------------------------

/// INV-01: Cargo.toml exists and is a valid TOML file.
#[test]
fn test_cargo_toml_parses() {
    let cargo_toml_path = Path::new(MANIFEST_DIR).join("Cargo.toml");
    assert!(
        cargo_toml_path.exists(),
        "INV-01: Cargo.toml not found at {:?}",
        cargo_toml_path
    );

    let contents = std::fs::read_to_string(&cargo_toml_path)
        .expect("INV-01: failed to read Cargo.toml");

    // Validate it's parseable as TOML.
    let parsed: Result<toml::Value, _> = toml::from_str(&contents);
    assert!(
        parsed.is_ok(),
        "INV-01: Cargo.toml failed TOML parse: {:?}",
        parsed.err()
    );

    let table = parsed.unwrap();
    assert!(
        table.get("package").is_some(),
        "INV-01: Cargo.toml missing [package] section"
    );
    assert!(
        table.get("dependencies").is_some(),
        "INV-01: Cargo.toml missing [dependencies] section"
    );
}

/// INV-01: tauri.conf.json exists and is valid JSON with expected top-level keys.
#[test]
fn test_tauri_conf_parses() {
    let conf_path = Path::new(MANIFEST_DIR).join("tauri.conf.json");
    assert!(
        conf_path.exists(),
        "INV-01: tauri.conf.json not found at {:?}",
        conf_path
    );

    let contents = std::fs::read_to_string(&conf_path)
        .expect("INV-01: failed to read tauri.conf.json");

    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&contents);
    assert!(
        parsed.is_ok(),
        "INV-01: tauri.conf.json failed JSON parse: {:?}",
        parsed.err()
    );

    let conf = parsed.unwrap();
    assert!(
        conf.get("identifier").is_some() || conf.get("tauri").is_some(),
        "INV-01: tauri.conf.json missing expected top-level key ('identifier' or 'tauri')"
    );
}

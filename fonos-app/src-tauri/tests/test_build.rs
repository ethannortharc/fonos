//! Build gate tests.
//! Covers: INV-01 (App launches — Cargo.toml and tauri.conf.json parse correctly,
//!          cargo check succeeds)
//!
//! Note: `cargo check` is implicitly validated by running `cargo test` at all.
//! These tests provide explicit artifact-level checks.

use std::path::Path;
use std::process::Command;

const WORKSPACE: &str = "/Users/ethan/Projects/design/fonos/fonos-app";

// ---------------------------------------------------------------------------
// INV-01 Level 1: Static config file validation
// ---------------------------------------------------------------------------

/// INV-01: Cargo.toml exists and is a valid TOML file.
#[test]
fn test_cargo_toml_parses() {
    let cargo_toml_path = Path::new(WORKSPACE).join("src-tauri").join("Cargo.toml");
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
    // tauri.conf.json may be in src-tauri/ directly.
    let conf_path = Path::new(WORKSPACE).join("src-tauri").join("tauri.conf.json");
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

// ---------------------------------------------------------------------------
// INV-01 Level 2: cargo check
// ---------------------------------------------------------------------------

/// INV-01: `cargo check` in the src-tauri directory exits with code 0.
/// This verifies the Rust code compiles without errors.
///
/// Note: This test is inherently validated by `cargo test` succeeding, but
/// we make it explicit so the ratchet can track it independently.
#[test]
fn test_cargo_check() {
    let output = Command::new("cargo")
        .args(["check", "--manifest-path", "src-tauri/Cargo.toml"])
        .current_dir(WORKSPACE)
        .output()
        .expect("INV-01: failed to run cargo check");

    assert!(
        output.status.success(),
        "INV-01: cargo check failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

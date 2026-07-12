//! Window / capability parity guard.
//! Covers: recurrence guard for the "new panel window missing from the
//! capability grant → silent event-listen denial" bug class (dialog-panel in
//! v0.6.0, call-panel in P2 Task 8).
//!
//! Every window label declared in tauri.conf.json's `app.windows` must also
//! appear in capabilities/default.json's `windows` array, or that window's
//! webview silently loses `core:event:allow-listen` (and friends) at runtime
//! with no compile-time or launch-time error.

use serde_json::Value;

// include_str! resolves relative to this source file, so this test is robust
// to being run from any working directory / CI checkout layout.
const TAURI_CONF: &str = include_str!("../tauri.conf.json");
const CAPABILITIES_DEFAULT: &str = include_str!("../capabilities/default.json");

/// Every window label in tauri.conf.json must be granted in
/// capabilities/default.json's `windows` array.
#[test]
fn test_every_window_label_has_capability_grant() {
    let conf: Value = serde_json::from_str(TAURI_CONF)
        .expect("tauri.conf.json failed to parse as JSON");
    let caps: Value = serde_json::from_str(CAPABILITIES_DEFAULT)
        .expect("capabilities/default.json failed to parse as JSON");

    let windows = conf
        .get("app")
        .and_then(|a| a.get("windows"))
        .and_then(|w| w.as_array())
        .expect("tauri.conf.json missing app.windows array");

    let labels: Vec<&str> = windows
        .iter()
        .map(|w| {
            w.get("label")
                .and_then(|l| l.as_str())
                .expect("tauri.conf.json window entry missing string 'label'")
        })
        .collect();

    let granted = caps
        .get("windows")
        .and_then(|w| w.as_array())
        .expect("capabilities/default.json missing 'windows' array");

    let granted_labels: Vec<&str> = granted
        .iter()
        .map(|w| {
            w.as_str()
                .expect("capabilities/default.json 'windows' entry is not a string")
        })
        .collect();

    for label in &labels {
        assert!(
            granted_labels.contains(label),
            "window label '{label}' is declared in tauri.conf.json's app.windows \
             but missing from capabilities/default.json's 'windows' array — \
             this window's webview will silently be denied core:event:allow-listen \
             (and other granted permissions) at runtime. Fix: add \"{label}\" to \
             the 'windows' array in src-tauri/capabilities/default.json.",
        );
    }
}

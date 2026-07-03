//! Tests for text injection into focused applications (issue #6).
//! Covers: INV-10 (Text injection — clipboard paste + simulated typing,
//! strategy resolved from AppConfig global default plus per-app overrides).
//!
//! The Accessibility-permission end-to-end test is `#[ignore]`d: it types
//! into the focused window and requires the runner to be trusted for
//! Accessibility on macOS.

use fonos_core::config::{AppConfig, InjectionAppOverride};
use fonos_desktop::injection::{
    inject_text_with_strategy, resolve_strategy_for_app, InjectionMethod, InjectionStrategy,
};

/// Build an AppConfig with a fixed global strategy and the given overrides,
/// so tests are independent of the platform-specific default.
fn config_with(strategy: &str, overrides: Vec<InjectionAppOverride>) -> AppConfig {
    AppConfig {
        injection_strategy: strategy.into(),
        injection_app_overrides: overrides,
        ..Default::default()
    }
}

fn override_for(app: &str, strategy: &str) -> InjectionAppOverride {
    InjectionAppOverride {
        app: app.into(),
        strategy: strategy.into(),
    }
}

// ---------------------------------------------------------------------------
// INV-10 Level 1: InjectionStrategy::parse (pure string parsing)
// ---------------------------------------------------------------------------

/// INV-10: "paste" parses to Paste.
#[test]
fn test_parse_paste() {
    assert_eq!(InjectionStrategy::parse("paste"), InjectionStrategy::Paste);
}

/// INV-10: "type" parses to Type.
#[test]
fn test_parse_type() {
    assert_eq!(InjectionStrategy::parse("type"), InjectionStrategy::Type);
}

/// INV-10: parsing is case-insensitive and trims surrounding whitespace.
#[test]
fn test_parse_type_case_and_whitespace() {
    assert_eq!(
        InjectionStrategy::parse("TYPE  "),
        InjectionStrategy::Type,
        "INV-10: strategy parsing must be case-insensitive and whitespace-tolerant"
    );
}

/// INV-10: empty string falls back to the default (Paste).
#[test]
fn test_parse_empty_defaults_to_paste() {
    assert_eq!(InjectionStrategy::parse(""), InjectionStrategy::Paste);
}

/// INV-10: unrecognized values fall back to the default (Paste).
#[test]
fn test_parse_garbage_defaults_to_paste() {
    assert_eq!(InjectionStrategy::parse("garbage"), InjectionStrategy::Paste);
}

/// INV-10: "typing" is an accepted alias for Type.
#[test]
fn test_parse_typing_alias() {
    assert_eq!(InjectionStrategy::parse("typing"), InjectionStrategy::Type);
}

/// INV-10: "keystrokes" is an accepted alias for Type.
#[test]
fn test_parse_keystrokes_alias() {
    assert_eq!(
        InjectionStrategy::parse("keystrokes"),
        InjectionStrategy::Type
    );
}

// ---------------------------------------------------------------------------
// INV-10 Level 1: resolve_strategy_for_app (config + per-app override logic)
// ---------------------------------------------------------------------------

/// INV-10: an explicit "paste" default with no app name resolves to Paste.
/// (The real default is platform-dependent — "type" on Linux, "paste"
/// elsewhere — so the test sets the strategy explicitly to stay portable.)
#[test]
fn test_resolve_default_no_app() {
    let config = config_with("paste", vec![]);
    assert_eq!(
        resolve_strategy_for_app(&config, None),
        InjectionStrategy::Paste,
        "INV-10: with no app name, resolution falls back to the global default"
    );
}

/// INV-10: a per-app override matches the app name as a case-insensitive
/// substring ("terminal" matches "Terminal").
#[test]
fn test_resolve_override_case_insensitive_substring() {
    let config = config_with("paste", vec![override_for("terminal", "type")]);
    assert_eq!(
        resolve_strategy_for_app(&config, Some("Terminal")),
        InjectionStrategy::Type,
        "INV-10: matching override should win over the default"
    );
}

/// INV-10: when no override matches, resolution falls back to the default.
#[test]
fn test_resolve_no_matching_override_uses_default() {
    let config = config_with("paste", vec![override_for("terminal", "type")]);
    assert_eq!(
        resolve_strategy_for_app(&config, Some("Safari")),
        InjectionStrategy::Paste,
        "INV-10: a non-matching override must not change the default"
    );
}

/// INV-10: the first matching override in the list wins.
#[test]
fn test_resolve_first_matching_override_wins() {
    let config = config_with(
        "paste",
        vec![
            override_for("term", "type"),
            override_for("terminal", "paste"),
        ],
    );
    assert_eq!(
        resolve_strategy_for_app(&config, Some("Terminal")),
        InjectionStrategy::Type,
        "INV-10: earlier overrides take precedence over later ones"
    );
}

/// INV-10: overrides with an empty app pattern are skipped (they would
/// otherwise match every app).
#[test]
fn test_resolve_empty_pattern_is_skipped() {
    let config = config_with("paste", vec![override_for("", "type")]);
    assert_eq!(
        resolve_strategy_for_app(&config, Some("Safari")),
        InjectionStrategy::Paste,
        "INV-10: empty override patterns must be ignored"
    );
}

/// INV-10: a None app name ignores overrides entirely and uses the default.
#[test]
fn test_resolve_none_app_with_overrides_uses_default() {
    let config = config_with("type", vec![override_for("terminal", "paste")]);
    assert_eq!(
        resolve_strategy_for_app(&config, None),
        InjectionStrategy::Type,
        "INV-10: without an app name, per-app overrides do not apply"
    );
}

/// INV-10: an unknown strategy string in a matching override falls back to
/// Paste (parse's default).
#[test]
fn test_resolve_unknown_strategy_in_override_defaults_to_paste() {
    let config = config_with("type", vec![override_for("terminal", "nonsense")]);
    assert_eq!(
        resolve_strategy_for_app(&config, Some("Terminal")),
        InjectionStrategy::Paste,
        "INV-10: an unrecognized override strategy parses to Paste"
    );
}

// ---------------------------------------------------------------------------
// INV-10 Level 3: Real end-to-end injection (needs Accessibility permission)
// ---------------------------------------------------------------------------

/// INV-10: inject "fonos-test" via the Paste strategy and verify it succeeds.
///
/// IGNORED by default: this actually types into the currently focused window
/// (clipboard save → set text → Cmd+V → clipboard restore) and requires the
/// test runner to be trusted for Accessibility on macOS. Run it manually with
/// a focused text field:
///   `cargo test -p fonos-desktop --test test_injection -- --ignored --nocapture`
#[test]
#[ignore = "types into the focused window; requires Accessibility permission (macOS)"]
fn test_inject_text_paste_end_to_end() {
    let result = inject_text_with_strategy("fonos-test", InjectionStrategy::Paste);
    assert!(
        result.is_ok(),
        "INV-10: inject_text_with_strategy(Paste) should succeed (got: {:?})",
        result
    );
    assert_eq!(
        result.unwrap(),
        InjectionMethod::ClipboardPaste,
        "INV-10: the Paste strategy should report the ClipboardPaste method"
    );
}

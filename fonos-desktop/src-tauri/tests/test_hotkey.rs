//! Tests for hotkey registration and event dispatch.
//! Covers: INV-04 (Hotkey registration)
//!
//! Exercises the real `hotkey::HotkeyManager` (src/hotkey.rs — macOS-only,
//! CGEventTap-backed). `parse_hotkey`, `register`, and `replace_hotkeys` are
//! pure logic and need no hardware or Accessibility trust, so they're tested
//! directly here. Live event dispatch is covered separately (see bottom).

#![cfg(target_os = "macos")]

use core_graphics::event::{CGEventFlags, KeyCode};
use fonos_desktop::hotkey::HotkeyManager;

// ---------------------------------------------------------------------------
// INV-04: Hotkey config parsing (HotkeyManager::parse_hotkey)
// ---------------------------------------------------------------------------

/// INV-04: The default combo "cmd+shift+space" parses into cmd+shift
/// modifiers and the space key.
#[test]
fn test_hotkey_config_parsing_default() {
    let cfg = HotkeyManager::parse_hotkey("cmd+shift+space", "toggle_dictation")
        .expect("INV-04: failed to parse default hotkey");
    let expected_mods =
        (CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift).bits();
    assert_eq!(
        cfg.modifiers, expected_mods,
        "INV-04: cmd+shift modifiers should be set, and nothing else"
    );
    assert_eq!(cfg.keycode, KeyCode::SPACE, "INV-04: key should be space");
    assert_eq!(cfg.label, "toggle_dictation");
}

/// INV-04: A combo stacking all four modifiers sets all four modifier bits.
#[test]
fn test_hotkey_config_parsing_all_modifiers() {
    let cfg = HotkeyManager::parse_hotkey("cmd+ctrl+opt+shift+r", "test_combo")
        .expect("INV-04: failed to parse multi-modifier combo");
    let expected_mods = (CGEventFlags::CGEventFlagCommand
        | CGEventFlags::CGEventFlagControl
        | CGEventFlags::CGEventFlagAlternate
        | CGEventFlags::CGEventFlagShift)
        .bits();
    assert_eq!(
        cfg.modifiers, expected_mods,
        "INV-04: all four modifier bits should be set"
    );
    assert_eq!(cfg.keycode, KeyCode::ANSI_R);
}

/// INV-04: modifier aliases ("command"/"option"/"control" and "opt"/"ctrl")
/// map to the same bits as their canonical short forms, and parsing is
/// case-insensitive for both modifiers and the key.
#[test]
fn test_hotkey_config_parsing_modifier_aliases_and_case() {
    let canonical = HotkeyManager::parse_hotkey("cmd+shift+opt+ctrl+r", "a").unwrap();
    let aliased = HotkeyManager::parse_hotkey("command+shift+option+control+r", "b").unwrap();
    assert_eq!(canonical.modifiers, aliased.modifiers);
    assert_eq!(canonical.keycode, aliased.keycode);

    let alt_alias = HotkeyManager::parse_hotkey("alt+r", "c").unwrap();
    assert_eq!(alt_alias.modifiers, CGEventFlags::CGEventFlagAlternate.bits());

    let upper = HotkeyManager::parse_hotkey("CMD+SHIFT+SPACE", "d").unwrap();
    let expected_mods =
        (CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift).bits();
    assert_eq!(upper.modifiers, expected_mods);
    assert_eq!(upper.keycode, KeyCode::SPACE);
}

/// INV-04: An unknown modifier returns an error naming the bad token.
#[test]
fn test_hotkey_config_parsing_unknown_modifier() {
    let result = HotkeyManager::parse_hotkey("win+space", "x");
    assert!(
        result.is_err(),
        "INV-04: unknown modifier 'win' should produce an error"
    );
    assert_eq!(result.unwrap_err(), "unknown modifier: win");
}

/// INV-04: A bare key with no modifiers parses with modifiers == 0.
#[test]
fn test_hotkey_config_parsing_no_modifier() {
    let cfg =
        HotkeyManager::parse_hotkey("f5", "y").expect("INV-04: failed to parse bare key 'f5'");
    assert_eq!(
        cfg.modifiers, 0,
        "INV-04: no modifiers should produce a 0 bitmask"
    );
    assert_eq!(cfg.keycode, KeyCode::F5);
}

/// INV-04: An unrecognised key name (after otherwise-valid modifiers)
/// returns an error naming the bad key.
#[test]
fn test_hotkey_config_parsing_unknown_key() {
    let result = HotkeyManager::parse_hotkey("cmd+nonexistentkey", "z");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "unknown key: nonexistentkey");
}

/// INV-04: An empty combo string is rejected. Note the dedicated "empty
/// hotkey spec" error arm in parse_hotkey is actually unreachable —
/// `"".split('+')` yields `[""]`, never an empty Vec — so this bottoms out
/// via the "unknown key" arm instead. Asserting only `is_err()` here so the
/// test doesn't pin down that implementation detail.
#[test]
fn test_hotkey_config_parsing_empty_spec_errors() {
    let result = HotkeyManager::parse_hotkey("", "e");
    assert!(result.is_err(), "INV-04: empty combo string should be rejected");
}

/// INV-04: The label argument (not part of the combo string) is carried
/// through verbatim onto the resulting config.
#[test]
fn test_hotkey_config_label_passthrough() {
    let cfg = HotkeyManager::parse_hotkey("ctrl+a", "my_custom_label").unwrap();
    assert_eq!(cfg.label, "my_custom_label");
}

// ---------------------------------------------------------------------------
// INV-04: HotkeyManager registration (pure logic, no hardware)
// ---------------------------------------------------------------------------

/// INV-04: register() appends the parsed config to the manager's hotkey
/// list.
#[test]
fn test_hotkey_manager_register() {
    let mut manager = HotkeyManager::new();
    let cfg = HotkeyManager::parse_hotkey("cmd+shift+space", "toggle_dictation").unwrap();
    manager.register(cfg);

    let hotkeys = manager.hotkeys_ref();
    let guard = hotkeys.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].label, "toggle_dictation");
}

/// INV-04: replace_hotkeys() clears any existing bindings and installs the
/// new set (used for hot-reload).
#[test]
fn test_hotkey_manager_replace_hotkeys() {
    let mut manager = HotkeyManager::new();
    manager.register(HotkeyManager::parse_hotkey("cmd+shift+space", "a").unwrap());

    manager.replace_hotkeys(vec![
        HotkeyManager::parse_hotkey("ctrl+opt+r", "b").unwrap(),
        HotkeyManager::parse_hotkey("f5", "c").unwrap(),
    ]);

    let hotkeys = manager.hotkeys_ref();
    let guard = hotkeys.lock().unwrap();
    assert_eq!(guard.len(), 2, "INV-04: replace_hotkeys should drop the old binding");
    assert_eq!(guard[0].label, "b");
    assert_eq!(guard[1].label, "c");
}

// ---------------------------------------------------------------------------
// INV-04: Event dispatch — not covered here.
// ---------------------------------------------------------------------------
// Live dispatch needs a real CGEventTap, which requires Accessibility trust
// and can't run headlessly; it's covered manually/E2E instead.

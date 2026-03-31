//! Tests for hotkey registration and event dispatch.
//! Covers: INV-04 (Hotkey registration)

// ---------------------------------------------------------------------------
// INV-04 Level 1: Config parsing (pure logic, no hardware)
// ---------------------------------------------------------------------------

/// Minimal representation of a parsed hotkey.
/// Mirrors what hotkey.rs should expose once implemented.
#[derive(Debug, PartialEq)]
struct HotkeyConfig {
    /// Bitmask: bit 0 = cmd, bit 1 = shift, bit 2 = opt, bit 3 = ctrl
    modifiers: u8,
    key: String,
}

const MOD_CMD: u8 = 0b0001;
const MOD_SHIFT: u8 = 0b0010;
const MOD_OPT: u8 = 0b0100;
const MOD_CTRL: u8 = 0b1000;

/// Parse a hotkey string of the form "mod+mod+key" into a HotkeyConfig.
/// This is the pure-logic version — the real implementation lives in hotkey.rs.
fn parse_hotkey(spec: &str) -> Result<HotkeyConfig, String> {
    let parts: Vec<&str> = spec.split('+').collect();
    if parts.is_empty() {
        return Err("empty hotkey spec".into());
    }
    let key = parts.last().unwrap().to_lowercase();
    let mut modifiers: u8 = 0;
    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "cmd" | "command" => modifiers |= MOD_CMD,
            "shift" => modifiers |= MOD_SHIFT,
            "opt" | "option" | "alt" => modifiers |= MOD_OPT,
            "ctrl" | "control" => modifiers |= MOD_CTRL,
            unknown => return Err(format!("unknown modifier: {unknown}")),
        }
    }
    Ok(HotkeyConfig { modifiers, key })
}

/// INV-04: "cmd+shift+space" parses into cmd+shift modifiers and "space" key.
#[test]
fn test_hotkey_config_parsing_default() {
    let cfg = parse_hotkey("cmd+shift+space").expect("INV-04: failed to parse default hotkey");
    assert_eq!(
        cfg.modifiers & MOD_CMD,
        MOD_CMD,
        "INV-04: cmd modifier should be set"
    );
    assert_eq!(
        cfg.modifiers & MOD_SHIFT,
        MOD_SHIFT,
        "INV-04: shift modifier should be set"
    );
    assert_eq!(cfg.key, "space", "INV-04: key should be 'space'");
}

/// INV-04: "opt+shift+r" parses correctly.
#[test]
fn test_hotkey_config_parsing_opt_shift_r() {
    let cfg = parse_hotkey("opt+shift+r").expect("INV-04: failed to parse opt+shift+r");
    assert_eq!(
        cfg.modifiers & MOD_OPT,
        MOD_OPT,
        "INV-04: opt modifier should be set"
    );
    assert_eq!(
        cfg.modifiers & MOD_SHIFT,
        MOD_SHIFT,
        "INV-04: shift modifier should be set"
    );
    assert_eq!(
        cfg.modifiers & MOD_CMD,
        0,
        "INV-04: cmd modifier should NOT be set"
    );
    assert_eq!(cfg.key, "r", "INV-04: key should be 'r'");
}

/// INV-04: An unknown modifier returns an error.
#[test]
fn test_hotkey_config_parsing_unknown_modifier() {
    let result = parse_hotkey("win+space");
    assert!(
        result.is_err(),
        "INV-04: unknown modifier 'win' should produce an error"
    );
}

/// INV-04: A single key with no modifier parses with modifiers == 0.
#[test]
fn test_hotkey_config_parsing_no_modifier() {
    let cfg = parse_hotkey("f5").expect("INV-04: failed to parse bare key 'f5'");
    assert_eq!(cfg.modifiers, 0, "INV-04: no modifiers should produce 0 bitmask");
    assert_eq!(cfg.key, "f5");
}

// ---------------------------------------------------------------------------
// INV-04 Level 2: Event dispatch (failing — hotkey.rs not yet implemented)
// ---------------------------------------------------------------------------

/// INV-04: A simulated keypress matching the registered hotkey must trigger
/// a Tauri event on the event bus.
///
/// This test is a failing placeholder. Once hotkey.rs implements:
///   - `HotkeyManager::new(config: HotkeyConfig) -> Self`
///   - `HotkeyManager::register(&self) -> Result<(), HotkeyError>`
///   - `HotkeyManager::simulate_press(&self)` (test-only helper)
/// and exposes an event receiver, this test can be made concrete.
#[test]
#[cfg(not(feature = "ci"))]
fn test_hotkey_event_dispatch() {
    // --- FAILING PLACEHOLDER ---
    panic!(
        "INV-04 [NOT IMPLEMENTED]: hotkey::HotkeyManager not yet available. \
         Implement hotkey.rs (CGEventTapCreate + Tauri event emit) to make this test pass."
    );

    // Reference implementation sketch:
    //
    // use fonos_app::hotkey::HotkeyManager;
    // use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    //
    // let triggered = Arc::new(AtomicBool::new(false));
    // let triggered_clone = triggered.clone();
    //
    // let cfg = parse_hotkey("cmd+shift+space").unwrap();
    // let mut manager = HotkeyManager::new(cfg);
    // manager.on_press(move || { triggered_clone.store(true, Ordering::SeqCst); });
    // manager.register().expect("INV-04: failed to register hotkey");
    // manager.simulate_press();
    //
    // std::thread::sleep(std::time::Duration::from_millis(100));
    // assert!(triggered.load(Ordering::SeqCst), "INV-04: event not fired after simulated press");
}

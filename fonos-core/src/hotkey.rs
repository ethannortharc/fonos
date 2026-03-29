//! Hotkey parsing utilities — platform-independent subset.
//!
//! This module provides a `parse_hotkey` function that validates a hotkey
//! combo string (e.g. `"cmd+shift+space"`, `"option+n"`) without depending
//! on any macOS-specific framework.  The full hotkey *registration* lives in
//! `fonos-app/src-tauri/src/hotkey.rs` which uses CGEventTap.

/// Parse a hotkey combo string and return `Ok(())` if the combo is valid,
/// or `Err(description)` if any part is unrecognised.
///
/// # Combo format
/// `modifier(+modifier)*+key` — e.g. `"cmd+shift+space"`, `"option+n"`.
///
/// Recognised modifiers: `cmd`/`command`, `shift`, `alt`/`opt`/`option`,
/// `ctrl`/`control`.
///
/// The key portion is any single ASCII letter, digit, or well-known name
/// (`space`, `enter`, `return`, `tab`, `escape`, `esc`, `backspace`,
/// `delete`, `up`, `down`, `left`, `right`, `f1`–`f20`, and all letter/digit
/// keys a–z, 0–9).
pub fn parse_hotkey(combo: &str) -> Result<(), String> {
    let parts: Vec<&str> = combo.split('+').collect();
    if parts.is_empty() || combo.is_empty() {
        return Err("empty hotkey spec".into());
    }

    let key_str = parts.last().unwrap().to_lowercase();
    let modifier_parts = &parts[..parts.len() - 1];

    // Validate modifiers
    for part in modifier_parts {
        match part.to_lowercase().as_str() {
            "cmd" | "command" | "shift" | "alt" | "opt" | "option" | "ctrl" | "control" => {}
            unknown => return Err(format!("unknown modifier: {unknown}")),
        }
    }

    // Validate key
    if key_str.is_empty() {
        return Err("empty key in hotkey spec".into());
    }

    // Single letter or digit
    if key_str.len() == 1 {
        let c = key_str.chars().next().unwrap();
        if c.is_ascii_alphanumeric() {
            return Ok(());
        }
    }

    // Named keys
    match key_str.as_str() {
        "space" | "enter" | "return" | "tab" | "escape" | "esc"
        | "backspace" | "delete" | "up" | "down" | "left" | "right"
        | "home" | "end" | "pageup" | "pagedown"
        | "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10"
        | "f11" | "f12" | "f13" | "f14" | "f15" | "f16" | "f17" | "f18" | "f19" | "f20" => {
            Ok(())
        }
        unknown => Err(format!("unknown key: {unknown}")),
    }
}

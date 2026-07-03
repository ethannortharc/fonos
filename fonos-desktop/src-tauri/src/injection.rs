//! Text injection at the cursor position.
//!
//! Two strategies are supported (issue #6):
//!
//! * **Paste** — clipboard sequence (save clipboard → set text → Cmd+V →
//!   restore clipboard). Fast and reliable for long text, but briefly
//!   occupies the clipboard.
//! * **Type** — simulated keystrokes carrying unicode payloads (macOS
//!   `CGEventKeyboardSetUnicodeString`, Linux `xdotool type`). Never touches
//!   the clipboard, but is slower and can be throttled by some apps.
//!
//! The strategy is picked from `AppConfig`: a global default plus per-app
//! overrides matched against the frontmost application's name.
//!
//! Both strategies *insert at the cursor* and preserve the field's
//! surrounding text. The Accessibility API is deliberately not used here:
//! `AXUIElementSetAttributeValue(AXValue, …)` *replaces the entire field's
//! contents* rather than inserting at the cursor, which would clobber
//! whatever the user already typed.
//!
//! Failure detection: on macOS, injection fails fast with a clear message
//! when the Accessibility permission is missing (CGEventPost would silently
//! no-op) or when a secure input field is active (macOS blocks simulated
//! input into password fields).

use fonos_core::config::AppConfig;
use std::time::Duration;

/// Delay after writing to the clipboard before sending the paste keystroke,
/// so the pasteboard is visible system-wide.
const CLIPBOARD_SETTLE_MS: u64 = 30;
/// Delay after the paste keystroke before restoring the clipboard. Slow
/// (often Electron) apps read the pasteboard asynchronously; restoring too
/// early makes them paste the *restored* content instead.
const PASTE_SETTLE_MS: u64 = 150;
/// Max UTF-16 units per keyboard event in Type mode (CGEvent payload limit).
#[cfg(target_os = "macos")]
const TYPE_CHUNK_UNITS: usize = 20;
/// Pause between Type-mode keyboard events so apps don't drop them.
#[cfg(target_os = "macos")]
const TYPE_CHUNK_DELAY_MS: u64 = 2;

/// Which injection method was used.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectionMethod {
    /// A simulated paste sequence (Cmd+V on macOS, Ctrl+Shift+V on Linux).
    ClipboardPaste,
    /// Simulated typing (unicode keyboard events / `xdotool type`).
    SimulatedTyping,
}

/// How text should be delivered to the target app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionStrategy {
    Paste,
    Type,
}

impl InjectionStrategy {
    /// Parse a config string. Unknown values fall back to Paste (the default).
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "type" | "typing" | "keystrokes" => Self::Type,
            _ => Self::Paste,
        }
    }
}

/// Resolve the injection strategy for the given frontmost app name.
///
/// Per-app overrides are matched as case-insensitive substrings of the app
/// name; the first match in the list wins. Falls back to the global default.
pub fn resolve_strategy_for_app(config: &AppConfig, app_name: Option<&str>) -> InjectionStrategy {
    if let Some(name) = app_name {
        let name_lower = name.to_lowercase();
        for o in &config.injection_app_overrides {
            let pat = o.app.trim().to_lowercase();
            if !pat.is_empty() && name_lower.contains(&pat) {
                return InjectionStrategy::parse(&o.strategy);
            }
        }
    }
    InjectionStrategy::parse(&config.injection_strategy)
}

/// Injects `text` at the current cursor position using the strategy resolved
/// from `config` (global default + per-app overrides).
///
/// Returns the method used, or a clear error string when injection could not
/// be delivered (missing Accessibility permission, secure input field,
/// clipboard failure, …).
pub fn inject_text(text: &str, config: &AppConfig) -> Result<InjectionMethod, String> {
    // Only pay for the frontmost-app lookup (an osascript round-trip on
    // macOS) when overrides exist.
    let app_name = if config.injection_app_overrides.is_empty() {
        None
    } else {
        let name = crate::commands::selection::frontmost_app();
        if name.is_empty() { None } else { Some(name) }
    };
    let strategy = resolve_strategy_for_app(config, app_name.as_deref());
    inject_text_with_strategy(text, strategy)
}

/// Injects `text` with an explicit strategy, bypassing config resolution.
///
/// If the chosen strategy fails for a reason that isn't a preflight failure
/// (those block both strategies), the other strategy is tried once before
/// reporting an error. Fallback is safe: every failure mode of a strategy
/// happens before any text reaches the target app.
pub fn inject_text_with_strategy(
    text: &str,
    strategy: InjectionStrategy,
) -> Result<InjectionMethod, String> {
    preflight_input_checks()?;
    match try_strategy(text, strategy) {
        Ok(method) => Ok(method),
        Err(primary_err) => {
            let alt = match strategy {
                InjectionStrategy::Paste => InjectionStrategy::Type,
                InjectionStrategy::Type => InjectionStrategy::Paste,
            };
            eprintln!("fonos: {strategy:?} injection failed ({primary_err}); trying {alt:?}");
            match try_strategy(text, alt) {
                Ok(method) => Ok(method),
                Err(alt_err) => Err(format!(
                    "{primary_err} (fallback {alt:?} also failed: {alt_err})"
                )),
            }
        }
    }
}

fn try_strategy(text: &str, strategy: InjectionStrategy) -> Result<InjectionMethod, String> {
    match strategy {
        InjectionStrategy::Type => {
            typing_injection(text)?;
            Ok(InjectionMethod::SimulatedTyping)
        }
        InjectionStrategy::Paste => {
            clipboard_paste_injection(text)?;
            Ok(InjectionMethod::ClipboardPaste)
        }
    }
}

/// Simulate pressing the Enter/Return key.
pub fn press_enter() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        preflight_input_checks()?;
        unsafe { simulate_key(0x24, false) } // 0x24 = Return key
    }
    #[cfg(target_os = "linux")]
    {
        run_xdotool(&["key", "Return"])
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err("press_enter is not supported on this platform".to_string())
    }
}

/// Whether this process is trusted for Accessibility (required to post
/// keyboard events on macOS). Always true on other platforms.
pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { AXIsProcessTrusted() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Fail fast on conditions that make simulated input silently vanish.
fn preflight_input_checks() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if !accessibility_trusted() {
            return Err(
                "Accessibility permission not granted — Fonos can't deliver keystrokes. \
                 Enable Fonos in System Settings > Privacy & Security > Accessibility."
                    .to_string(),
            );
        }
        if unsafe { IsSecureEventInputEnabled() != 0 } {
            return Err(
                "A secure input field is active (likely a password field) — \
                 macOS blocks simulated input here."
                    .to_string(),
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Paste strategy
// ---------------------------------------------------------------------------

/// Copy to clipboard, simulate paste, restore clipboard.
///
/// The previous clipboard *text* is restored even when the paste keystroke
/// fails, with one retry. Non-text clipboard content (images, rich data)
/// cannot be snapshotted via `arboard` and is not restored.
fn clipboard_paste_injection(text: &str) -> Result<(), String> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to open clipboard: {}", e))?;

    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    // Give the clipboard a moment to be available system-wide.
    std::thread::sleep(Duration::from_millis(CLIPBOARD_SETTLE_MS));

    let paste_result = send_paste_keystroke();

    // Wait for the target app to read the pasteboard before restoring.
    std::thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));

    // Always restore, even when the paste keystroke failed.
    if let Some(prev) = previous {
        if clipboard.set_text(&prev).is_err() {
            std::thread::sleep(Duration::from_millis(50));
            if let Err(e) = clipboard.set_text(&prev) {
                eprintln!("fonos: clipboard restore failed after retry: {e}");
            }
        }
    }

    paste_result
}

#[cfg(target_os = "macos")]
fn send_paste_keystroke() -> Result<(), String> {
    unsafe { simulate_key(0x09, true) } // 0x09 = 'v', with Cmd
}

#[cfg(target_os = "linux")]
fn send_paste_keystroke() -> Result<(), String> {
    // Ctrl+Shift+V works in terminals; most GUI apps accept it too.
    run_xdotool(&["key", "--clearmodifiers", "ctrl+shift+v"])
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn send_paste_keystroke() -> Result<(), String> {
    Err("paste injection is not supported on this platform".to_string())
}

// ---------------------------------------------------------------------------
// Type strategy
// ---------------------------------------------------------------------------

/// Type `text` as simulated keystrokes without touching the clipboard.
#[cfg(target_os = "macos")]
fn typing_injection(text: &str) -> Result<(), String> {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            unsafe { simulate_key(0x24, false)? }; // Return between lines
            std::thread::sleep(Duration::from_millis(TYPE_CHUNK_DELAY_MS));
        }
        // Chunk on char boundaries so surrogate pairs never split across events.
        let mut buf: Vec<u16> = Vec::with_capacity(TYPE_CHUNK_UNITS + 2);
        for ch in line.chars() {
            let mut units = [0u16; 2];
            let encoded = ch.encode_utf16(&mut units);
            if buf.len() + encoded.len() > TYPE_CHUNK_UNITS {
                unsafe { post_unicode_keystroke(&buf)? };
                buf.clear();
                std::thread::sleep(Duration::from_millis(TYPE_CHUNK_DELAY_MS));
            }
            buf.extend_from_slice(encoded);
        }
        if !buf.is_empty() {
            unsafe { post_unicode_keystroke(&buf)? };
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn typing_injection(text: &str) -> Result<(), String> {
    run_xdotool(&["type", "--clearmodifiers", "--delay", "0", "--", text])
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn typing_injection(_text: &str) -> Result<(), String> {
    Err("typing injection is not supported on this platform".to_string())
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn run_xdotool(args: &[&str]) -> Result<(), String> {
    match std::process::Command::new("xdotool").args(args).output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(format!(
            "xdotool {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Err(format!("xdotool not available: {e}")),
    }
}

/// Simulate a single key press (down + up) with optional Command modifier.
#[cfg(target_os = "macos")]
unsafe fn simulate_key(key_code: u16, with_cmd: bool) -> Result<(), String> {
    use std::ptr;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;

    let source = ptr::null_mut::<std::ffi::c_void>();

    let down = CGEventCreateKeyboardEvent(source, key_code, true);
    if down.is_null() {
        return Err("CGEventCreateKeyboardEvent failed (key down)".to_string());
    }
    if with_cmd {
        CGEventSetFlags(down, K_CG_EVENT_FLAG_MASK_COMMAND);
    }
    CGEventPost(K_CG_SESSION_EVENT_TAP, down);
    CFRelease(down);

    let up = CGEventCreateKeyboardEvent(source, key_code, false);
    if up.is_null() {
        return Err("CGEventCreateKeyboardEvent failed (key up)".to_string());
    }
    if with_cmd {
        CGEventSetFlags(up, K_CG_EVENT_FLAG_MASK_COMMAND);
    }
    CGEventPost(K_CG_SESSION_EVENT_TAP, up);
    CFRelease(up);
    Ok(())
}

/// Post one keyboard event pair carrying a unicode payload (Type strategy).
#[cfg(target_os = "macos")]
unsafe fn post_unicode_keystroke(units: &[u16]) -> Result<(), String> {
    use std::ptr;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;

    let source = ptr::null_mut::<std::ffi::c_void>();

    for key_down in [true, false] {
        let event = CGEventCreateKeyboardEvent(source, 0, key_down);
        if event.is_null() {
            return Err("CGEventCreateKeyboardEvent failed (unicode keystroke)".to_string());
        }
        CGEventKeyboardSetUnicodeString(event, units.len(), units.as_ptr());
        CGEventPost(K_CG_SESSION_EVENT_TAP, event);
        CFRelease(event);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// macOS FFI
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *mut std::ffi::c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;

    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);

    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);

    fn CGEventKeyboardSetUnicodeString(
        event: *mut std::ffi::c_void,
        string_length: usize,
        unicode_string: *const u16,
    );

    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// Returns non-zero when this process may use Accessibility APIs
    /// (and therefore post CGEvents to other apps).
    fn AXIsProcessTrusted() -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    /// Returns non-zero while a secure input field (password entry) is
    /// focused anywhere on the system; simulated input is blocked then.
    fn IsSecureEventInputEnabled() -> u8;
}

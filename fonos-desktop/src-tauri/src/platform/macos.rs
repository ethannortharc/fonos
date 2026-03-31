//! macOS platform implementation.
//!
//! Implements the platform traits using CoreGraphics, AXUIElement, and
//! AppleScript. Delegates to `crate::injection` for text injection so that
//! existing code paths continue to work unchanged.

use super::{AppFocus, CursorPosition, KeySimulator, TextInjector};

/// macOS platform implementation.
pub struct MacOSPlatform;

// ---------------------------------------------------------------------------
// TextInjector
// ---------------------------------------------------------------------------

impl TextInjector for MacOSPlatform {
    fn inject_text(&self, text: &str) -> Result<(), String> {
        // Delegate to the existing injection module to keep behaviour identical.
        crate::injection::inject_text(text).map(|_| ())
    }

    fn press_enter(&self) {
        crate::injection::press_enter();
    }
}

// ---------------------------------------------------------------------------
// KeySimulator
// ---------------------------------------------------------------------------

impl KeySimulator for MacOSPlatform {
    fn simulate_copy(&self) {
        // macOS virtual keycode 0x08 = 'c'
        unsafe { simulate_key(0x08, true) };
    }

    fn simulate_paste(&self) {
        // macOS virtual keycode 0x09 = 'v'
        unsafe { simulate_key(0x09, true) };
    }
}

// ---------------------------------------------------------------------------
// AppFocus
// ---------------------------------------------------------------------------

impl AppFocus for MacOSPlatform {
    fn frontmost_app_name(&self) -> String {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to get name of first application process whose frontmost is true"#)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            }
            _ => String::new(),
        }
    }

    fn activate_app(&self, name: &str) {
        let script = format!(
            "tell application \"{}\" to activate",
            name.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output();
    }
}

// ---------------------------------------------------------------------------
// CursorPosition
// ---------------------------------------------------------------------------

impl CursorPosition for MacOSPlatform {
    fn cursor_position() -> (f64, f64) {
        let source = core_graphics::event_source::CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
        )
        .expect("CGEventSource");
        let event = core_graphics::event::CGEvent::new(source).expect("CGEvent");
        let loc = event.location();
        (loc.x, loc.y)
    }
}

// ---------------------------------------------------------------------------
// Helpers (CGEvent keyboard simulation)
// ---------------------------------------------------------------------------

/// Simulate a single key press (down + up) with optional Command modifier.
///
/// This is the same implementation as in `injection.rs` but exposed for the
/// `KeySimulator` trait. Both copies are kept during Phase 1 to avoid breaking
/// the existing injection module.
unsafe fn simulate_key(key_code: u16, with_cmd: bool) {
    use std::ptr;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;

    let source = ptr::null_mut::<std::ffi::c_void>();

    let down = CGEventCreateKeyboardEvent(source, key_code, true);
    if !down.is_null() {
        if with_cmd {
            CGEventSetFlags(down, K_CG_EVENT_FLAG_MASK_COMMAND);
        }
        CGEventPost(K_CG_SESSION_EVENT_TAP, down);
        CFRelease(down);
    }

    let up = CGEventCreateKeyboardEvent(source, key_code, false);
    if !up.is_null() {
        if with_cmd {
            CGEventSetFlags(up, K_CG_EVENT_FLAG_MASK_COMMAND);
        }
        CGEventPost(K_CG_SESSION_EVENT_TAP, up);
        CFRelease(up);
    }
}

extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *mut std::ffi::c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;

    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);

    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);

    fn CFRelease(cf: *mut std::ffi::c_void);
}

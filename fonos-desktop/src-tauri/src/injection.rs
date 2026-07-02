//! Text injection at the cursor position.
//!
//! macOS uses a clipboard-paste sequence (save clipboard → set text → Cmd+V →
//! restore clipboard); Linux uses `xdotool type`. Both *insert at the cursor*
//! and preserve the field's surrounding text. The Accessibility API is
//! deliberately not used here: `AXUIElementSetAttributeValue(AXValue, …)`
//! *replaces the entire field's contents* rather than inserting at the cursor,
//! which would clobber whatever the user already typed.

use std::time::Duration;

/// Which injection method was used.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectionMethod {
    /// A simulated paste / type sequence (Cmd+V on macOS, `xdotool` on Linux).
    ClipboardPaste,
}

/// Injects `text` at the current cursor position.
///
/// Uses a simulated paste (macOS) or `xdotool type` (Linux) so the text is
/// inserted at the cursor without overwriting surrounding content. Returns the
/// method used, or an error string.
pub fn inject_text(text: &str) -> Result<InjectionMethod, String> {
    let injector = TextInjector::new();
    injector.inject(text)
}

/// Simulate pressing the Enter/Return key.
pub fn press_enter() {
    #[cfg(target_os = "macos")]
    unsafe { simulate_key(0x24, false); } // 0x24 = Return key
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdotool").args(["key", "Return"]).output(); }
}

/// Simulate a single key press (down + up) with optional Command modifier.
#[cfg(target_os = "macos")]
unsafe fn simulate_key(key_code: u16, with_cmd: bool) {
    use std::ptr;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;

    let source = ptr::null_mut::<std::ffi::c_void>();

    let down = CGEventCreateKeyboardEvent(source, key_code, true);
    if !down.is_null() {
        if with_cmd { CGEventSetFlags(down, K_CG_EVENT_FLAG_MASK_COMMAND); }
        CGEventPost(K_CG_SESSION_EVENT_TAP, down);
        CFRelease(down);
    }

    let up = CGEventCreateKeyboardEvent(source, key_code, false);
    if !up.is_null() {
        if with_cmd { CGEventSetFlags(up, K_CG_EVENT_FLAG_MASK_COMMAND); }
        CGEventPost(K_CG_SESSION_EVENT_TAP, up);
        CFRelease(up);
    }
}

// ---------------------------------------------------------------------------
// TextInjector
// ---------------------------------------------------------------------------

pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    /// Inject text at the current cursor position.
    /// macOS: clipboard paste (Cmd+V). Linux: xdotool type (universal).
    pub fn inject(&self, text: &str) -> Result<InjectionMethod, String> {
        #[cfg(target_os = "linux")]
        {
            // xdotool type works in terminals, editors, and GUI apps
            let result = std::process::Command::new("xdotool")
                .args(["type", "--clearmodifiers", "--delay", "0", "--", text])
                .output();
            match result {
                Ok(out) if out.status.success() => return Ok(InjectionMethod::ClipboardPaste),
                _ => {
                    eprintln!("fonos: xdotool type failed, falling back to clipboard paste");
                    // Fall through to clipboard paste
                }
            }
        }
        self.clipboard_paste_injection(text)?;
        Ok(InjectionMethod::ClipboardPaste)
    }

    /// Copy to clipboard, simulate Cmd+V, restore clipboard.
    fn clipboard_paste_injection(&self, text: &str) -> Result<(), String> {
        use arboard::Clipboard;

        let mut clipboard = Clipboard::new()
            .map_err(|e| format!("Failed to open clipboard: {}", e))?;

        // Save the current clipboard content (best-effort; ignore errors).
        let previous = clipboard.get_text().ok();

        // Write the new text to the clipboard.
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to set clipboard: {}", e))?;

        // Give the clipboard a moment to be available system-wide.
        std::thread::sleep(Duration::from_millis(30));

        // Simulate paste: Cmd+V (macOS) or xdotool (Linux).
        #[cfg(target_os = "macos")]
        {
            unsafe { simulate_key(0x09, true) }; // 0x09 = 'v'
        }
        #[cfg(target_os = "linux")]
        {
            // Ctrl+Shift+V works in terminals; Ctrl+V works in GUI apps.
            // Try both with a small delay — the app will only respond to one.
            let _ = std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "ctrl+shift+v"])
                .output();
        }

        // Wait for the paste to complete before restoring clipboard.
        std::thread::sleep(Duration::from_millis(80));

        // Restore the previous clipboard content.
        if let Some(prev) = previous {
            // Best-effort; ignore errors.
            let _ = clipboard.set_text(&prev);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// macOS-specific helpers
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

    fn CFRelease(cf: *mut std::ffi::c_void);
}

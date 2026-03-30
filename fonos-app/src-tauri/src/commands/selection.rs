//! Commands for reading selected text from other apps and replacing it.
//!
//! Uses CGEvent for keystroke simulation (same as injection.rs) and arboard
//! for clipboard access — much more reliable than osascript System Events.

use serde::Serialize;
use std::process::Command;

/// Result of grabbing the currently selected text from the frontmost app.
#[derive(Debug, Clone, Serialize)]
pub struct SelectionContext {
    /// The selected text (empty if nothing was selected).
    pub text: String,
    /// Name of the frontmost application.
    pub app_name: String,
    /// Whether the focused element appears to be an editable text field.
    pub editable: bool,
}

// ── CGEvent key simulation (mirrors injection.rs) ────────────────────────────

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

#[cfg(target_os = "macos")]
const K_CG_SESSION_EVENT_TAP: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;

/// Simulate a key press with Command modifier via CGEvent.
#[cfg(target_os = "macos")]
unsafe fn simulate_cmd_key(key_code: u16) {
    use std::ptr;
    let source = ptr::null_mut();

    let down = CGEventCreateKeyboardEvent(source, key_code, true);
    if !down.is_null() {
        CGEventSetFlags(down, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventPost(K_CG_SESSION_EVENT_TAP, down);
        CFRelease(down);
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    let up = CGEventCreateKeyboardEvent(source, key_code, false);
    if !up.is_null() {
        CGEventSetFlags(up, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventPost(K_CG_SESSION_EVENT_TAP, up);
        CFRelease(up);
    }
}

// Key codes: 0x08 = 'c', 0x09 = 'v'

/// Get the name of the frontmost application.
fn frontmost_app() -> String {
    Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first application process whose frontmost is true",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Activate a named application (bring it to front).
fn activate_app(name: &str) {
    let script = format!(
        "tell application \"{}\" to activate",
        name.replace('"', "\\\"")
    );
    let _ = Command::new("osascript")
        .args(["-e", &script])
        .output();
}

/// Grab the currently selected text from the frontmost application.
///
/// Flow: save clipboard → Cmd+C via CGEvent → read clipboard → restore.
#[tauri::command]
pub async fn grab_selection() -> Result<SelectionContext, String> {
    use arboard::Clipboard;

    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("clipboard error: {e}"))?;

    // Save current clipboard
    let saved = clipboard.get_text().ok();

    // Clear clipboard
    let _ = clipboard.set_text("");

    let app_name = frontmost_app();

    // Small delay then simulate Cmd+C via CGEvent
    std::thread::sleep(std::time::Duration::from_millis(30));
    #[cfg(target_os = "macos")]
    unsafe { simulate_cmd_key(0x08); } // 0x08 = 'c'
    std::thread::sleep(std::time::Duration::from_millis(100));

    let text = clipboard.get_text().unwrap_or_default();

    // Restore original clipboard
    if let Some(ref prev) = saved {
        let _ = clipboard.set_text(prev);
    }

    eprintln!(
        "fonos: grab_selection app={} text_len={}",
        app_name, text.len()
    );

    Ok(SelectionContext {
        text,
        app_name,
        editable: true, // we always attempt replace; Cmd+V will silently fail if not editable
    })
}

/// Replace the current selection in the target app with the given text.
///
/// Flow: activate target app → set clipboard → Cmd+V via CGEvent → restore clipboard.
#[tauri::command]
pub async fn replace_selection(text: String, target_app: Option<String>) -> Result<(), String> {
    use arboard::Clipboard;

    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("clipboard error: {e}"))?;

    // Save current clipboard
    let saved = clipboard.get_text().ok();

    // Set replacement text
    clipboard.set_text(&text)
        .map_err(|e| format!("clipboard set error: {e}"))?;

    // Switch focus back to the original app
    if let Some(ref app) = target_app {
        if !app.is_empty() {
            eprintln!("fonos: replace_selection — activating {}", app);
            activate_app(app);
            // Wait for app activation + focus
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    }

    // Cmd+V via CGEvent
    #[cfg(target_os = "macos")]
    unsafe { simulate_cmd_key(0x09); } // 0x09 = 'v'

    // Wait for paste to complete, then restore
    std::thread::sleep(std::time::Duration::from_millis(100));
    if let Some(ref prev) = saved {
        let _ = clipboard.set_text(prev);
    }

    Ok(())
}

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
#[cfg(target_os = "macos")]
pub(crate) fn frontmost_app() -> String {
    Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get name of first application process whose frontmost is true"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub(crate) fn frontmost_app() -> String {
    Command::new("xdotool").args(["getactivewindow", "getwindowname"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn frontmost_app() -> String { String::new() }

/// Activate a named application (bring it to front).
#[cfg(target_os = "macos")]
fn activate_app(name: &str) {
    let script = format!("tell application \"{}\" to activate", name.replace('"', "\\\""));
    let _ = Command::new("osascript").args(["-e", &script]).output();
}

#[cfg(target_os = "linux")]
fn activate_app(name: &str) {
    let _ = Command::new("xdotool").args(["search", "--name", name, "windowactivate"]).output();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn activate_app(_name: &str) {}

// ── Clipboard snapshot / restore ─────────────────────────────────────────────
//
// The selection commands hijack the system clipboard: they clear it, drive a
// synthetic Cmd/Ctrl+C (grab) or +V (replace), then must hand the user back
// exactly what they had. Saving only `get_text()` — the behavior before this
// fix — silently destroyed a copied image, a Finder/file-manager selection, or
// rich text, degrading it to plain text or to nothing at all. These helpers
// capture and restore the richest content this arboard version can carry across
// the round-trip.

/// A best-effort capture of every clipboard flavor arboard 3.6.1 can *read* on
/// this platform, taken before the selection commands clobber the clipboard.
///
/// Every getter is implemented on both backends we ship against — macOS's
/// NSPasteboard and Linux's default X11 backend (used even under Wayland,
/// because we don't enable arboard's `wayland-data-control` feature) — so text,
/// HTML, image, and file list are each genuinely readable, not text fallbacks.
struct ClipboardSnapshot {
    text: Option<String>,
    html: Option<String>,
    /// `arboard::ImageData` is in scope because we build arboard with its default
    /// features, which include `image-data`. Turning those off would make this a
    /// compile error rather than a silent regression to text-only save/restore.
    image: Option<arboard::ImageData<'static>>,
    files: Option<Vec<std::path::PathBuf>>,
}

/// Capture the current clipboard. Every probe is best-effort: a flavor that is
/// absent (or that the backend declines) simply stays `None` instead of
/// aborting the capture.
fn snapshot_clipboard(clipboard: &mut arboard::Clipboard) -> ClipboardSnapshot {
    ClipboardSnapshot {
        text: clipboard.get_text().ok(),
        html: clipboard.get().html().ok(),
        image: clipboard.get_image().ok(),
        files: clipboard.get().file_list().ok(),
    }
}

/// Restore a snapshot with the single richest write that reproduces the most of
/// what was captured. arboard's `set_*` calls each replace the *entire*
/// clipboard, so there is no way to layer flavors — we pick one in priority
/// order:
///
///   image → `set_image`            (screenshots, copied pictures)
///   html  → `set_html(html, text)` (rich text; the plain-text alt rides along)
///   files → `set().file_list`      (Finder / file-manager selections)
///   text  → `set_text`             (the common case)
///
/// Gaps this cannot round-trip (arboard 3.6.1):
///   * Only one flavor group survives. `set_html` carrying its plain-text alt is
///     the *only* two-flavor write available, so a clipboard holding a richer
///     mix (e.g. image + file list, or image + html) keeps just the top-priority
///     flavor and drops the rest. Every individual flavor — text, html, image,
///     and file lists (yes, file lists too, on both X11 and Wayland) — is
///     otherwise fully round-trippable in 3.6.1, so this is the sole hard gap.
///   * macOS: `get().html()` returns HTML wrapped in arboard's own
///     `<html><head>…</head><body>…</body></html>`, and `set_html` wraps once
///     more, so a round-tripped fragment gains one nested wrapper. The markup is
///     preserved; only the envelope doubles.
///   * Linux X11: `set().file_list` canonicalizes paths and silently skips any
///     that no longer exist; if all are gone the file-list restore no-ops.
fn restore_clipboard(clipboard: &mut arboard::Clipboard, snap: ClipboardSnapshot) {
    if let Some(image) = snap.image {
        let _ = clipboard.set_image(image);
    } else if let Some(html) = snap.html {
        // `set_html` also takes the plain-text alternative, so text/html and
        // text/plain both come back from this single write.
        let _ = clipboard.set_html(html, snap.text);
    } else if let Some(files) = snap.files {
        let _ = clipboard.set().file_list(&files);
    } else if let Some(text) = snap.text {
        let _ = clipboard.set_text(text);
    }
}

/// Grab the currently selected text from the frontmost application.
///
/// Flow: save clipboard → Cmd+C via CGEvent → read clipboard → restore.
#[tauri::command]
pub async fn grab_selection() -> Result<SelectionContext, String> {
    // The whole sequence is blocking (clipboard I/O, CGEvent posting, and the
    // short settle sleeps between keystrokes) with no await points, so run it on
    // a blocking thread rather than stalling a tokio worker.
    tokio::task::spawn_blocking(grab_selection_blocking)
        .await
        .map_err(|e| format!("grab_selection task failed: {e}"))?
}

/// Blocking implementation of [`grab_selection`]; runs on a dedicated thread.
fn grab_selection_blocking() -> Result<SelectionContext, String> {
    use arboard::Clipboard;

    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("clipboard error: {e}"))?;

    // Save the user's clipboard — every flavor we can read, not just text — so
    // the synthetic Cmd/Ctrl+C below can't destroy a copied image, file
    // selection, or rich text (see [`snapshot_clipboard`]).
    let snapshot = snapshot_clipboard(&mut clipboard);

    // Clear clipboard
    let _ = clipboard.set_text("");

    let app_name = frontmost_app();

    // Simulate Copy: Cmd+C (macOS) or Ctrl+C (Linux)
    std::thread::sleep(std::time::Duration::from_millis(30));
    #[cfg(target_os = "macos")]
    unsafe { simulate_cmd_key(0x08); } // 0x08 = 'c'
    #[cfg(target_os = "linux")]
    { let _ = Command::new("xdotool").args(["key", "--clearmodifiers", "ctrl+c"]).output(); }

    // The clipboard was cleared above, so "the copy landed" == "text turned
    // non-empty". Poll instead of one fixed sleep: X11 clipboard transfer is
    // an async request the owning app answers (a busy Chrome tab can take
    // several hundred ms), while a fast owner exits on the first round.
    // Bounded so a genuinely empty selection still returns (empty ⇒ the
    // engine's empty-input event) within ~600 ms.
    let mut text = String::new();
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Ok(t) = clipboard.get_text() {
            if !t.is_empty() {
                text = t;
                break;
            }
        }
    }

    // Restore the user's original clipboard (richest single write).
    restore_clipboard(&mut clipboard, snapshot);

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

    // Save the user's clipboard — every flavor we can read — before we overwrite
    // it with the replacement text, so it survives the paste (see
    // [`snapshot_clipboard`]). Captured synchronously here, before any await, so
    // the snapshot (not a live Clipboard borrow) is what crosses the awaits.
    let snapshot = snapshot_clipboard(&mut clipboard);

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

    // Simulate Paste: Cmd+V (macOS) or Ctrl+V (Linux)
    #[cfg(target_os = "macos")]
    unsafe { simulate_cmd_key(0x09); } // 0x09 = 'v'
    #[cfg(target_os = "linux")]
    { let _ = Command::new("xdotool").args(["key", "--clearmodifiers", "ctrl+v"]).output(); }

    // Wait for paste to complete, then restore. Async sleep so the tokio
    // worker isn't blocked while other hotkey events are pending.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    restore_clipboard(&mut clipboard, snapshot);

    Ok(())
}

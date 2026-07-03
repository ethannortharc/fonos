//! Surface classified pipeline errors on the float pill / activity feed.
//!
//! Classification lives in [`fonos_core::error_class`]; this adapter serializes
//! the result into the `float:error` Tauri event payload:
//!
//! ```json
//! {"message": "Invalid or missing API key — check Settings > Models", "pane": null}
//! {"message": "Accessibility permission not granted …",              "pane": "accessibility"}
//! ```
//!
//! The float pill (public/float.html) parses this payload; when `pane` is set it
//! renders a clickable error that opens the relevant System Settings pane via
//! the `open_settings_pane` command.

use tauri::Emitter;

pub use fonos_core::error_class::{classify_error, SurfacedError};

/// Emit a `float:error` event carrying a classified JSON payload.
///
/// The full raw error is always logged to stderr. When the classifier replaced
/// the message with a canned one, the first 120 chars of the raw cause are also
/// logged so the detail isn't lost from the pill message.
///
/// Accepts a `&tauri::AppHandle` (which implements [`tauri::Emitter`]); matches
/// how the hotkey callbacks in `main.rs` and the commands in `dictation.rs`
/// already emit float events.
pub fn emit_float_error(app: &tauri::AppHandle, raw: &str) {
    let surfaced = classify_error(raw);

    // Always log the full raw error for debugging.
    eprintln!("fonos: pipeline error: {raw}");
    // When surfaced as a canned message, tie it back to the raw cause in the log.
    if surfaced.message != raw {
        let cause: String = raw.chars().take(120).collect();
        eprintln!("fonos:   surfaced as: {} ({})", surfaced.message, cause);
    }

    let payload = serde_json::json!({
        "message": surfaced.message,
        "pane": surfaced.pane,
    })
    .to_string();

    let _ = app.emit("float:error", payload);
}

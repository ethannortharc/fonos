//! OS permission checks and System Settings deep links.
//!
//! Used by the float pill (clickable permission errors) and the first-run
//! onboarding wizard.

use std::sync::atomic::{AtomicBool, Ordering};

/// Process-level latch tracking whether we've observed Accessibility trust yet.
/// Drives the false→true flip detection: the first time trust flips on we must
/// re-arm the hotkey tap and repaint the tray (not on every poll). Reset when
/// trust is observed as *false* so a later re-grant re-fires. Process-scoped
/// (not the persistent funnel row) so it works across sessions.
static AX_TRUSTED_SEEN: AtomicBool = AtomicBool::new(false);

/// Whether this process is trusted for Accessibility (needed for global
/// hotkeys and text injection on macOS). Always true on other platforms.
///
/// On the false→true flip (first observation this process): records the
/// `ax_granted` funnel milestone (record-once), re-arms the global hotkey tap
/// (its listener thread exits at launch when AX is missing — `hotkey:reload`
/// only swaps bindings, never re-installs the tap), and refreshes the tray
/// health panel so the Dictation row clears its ⚠️.
#[tauri::command]
pub fn check_accessibility(app: tauri::AppHandle, state: tauri::State<'_, super::AppState>) -> bool {
    let trusted = crate::injection::accessibility_trusted();
    if trusted {
        // `swap` returns the prior value; `false` here means this is the flip.
        if !AX_TRUSTED_SEEN.swap(true, Ordering::SeqCst) {
            if let Ok(db) = state.db.lock() {
                let _ = fonos_core::funnel::record(&db, "ax_granted");
            }
            // Re-arm the CGEventTap now that AX is granted (macOS only; no-op if
            // the tap is already alive — `start`'s `running` guard covers that).
            #[cfg(target_os = "macos")]
            crate::hotkey::restart_after_ax_grant();
            // Repaint the tray: the Dictation row's AX-gated ⚠️ can now clear.
            crate::tray::refresh_tray_status(&app, None);
        }
    } else {
        // Trust is (still) missing — re-arm the latch so a later grant re-fires.
        AX_TRUSTED_SEEN.store(false, Ordering::SeqCst);
    }
    trusted
}

/// Trigger the OS Accessibility permission prompt (macOS) and return the
/// current trusted state. Non-macOS platforms return true immediately.
#[tauri::command]
pub fn request_accessibility() -> bool {
    crate::injection::accessibility_prompt()
}

/// Settings panes that can be deep-linked. Keys are stable identifiers used
/// by the frontend and in `float:error` payloads.
#[cfg(target_os = "macos")]
const SETTINGS_PANES: &[(&str, &str)] = &[
    (
        "microphone",
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
    ),
    (
        "accessibility",
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    ),
    (
        "speech_recognition",
        "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition",
    ),
    (
        "screen_recording",
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
    ),
];

/// Open the OS settings pane for the given permission.
///
/// `pane` must be one of: `microphone`, `accessibility`, `speech_recognition`,
/// `screen_recording`. Only the allowlisted URLs above are ever opened.
#[tauri::command]
pub fn open_settings_pane(pane: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = SETTINGS_PANES
            .iter()
            .find(|(key, _)| *key == pane)
            .map(|(_, url)| *url)
            .ok_or_else(|| format!("Unknown settings pane: {pane}"))?;
        std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|e| format!("Failed to open System Settings: {e}"))
            .and_then(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err("Failed to open System Settings".to_string())
                }
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Err("Settings deep links are only supported on macOS".to_string())
    }
}

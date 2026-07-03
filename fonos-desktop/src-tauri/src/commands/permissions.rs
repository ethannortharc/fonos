//! OS permission checks and System Settings deep links.
//!
//! Used by the float pill (clickable permission errors) and the first-run
//! onboarding wizard.

/// Whether this process is trusted for Accessibility (needed for global
/// hotkeys and text injection on macOS). Always true on other platforms.
#[tauri::command]
pub fn check_accessibility() -> bool {
    crate::injection::accessibility_trusted()
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

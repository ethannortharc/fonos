//! Auto-update capability probe.
//!
//! `tauri-plugin-updater` can report an update is *available* on every
//! platform, but whether `downloadAndInstall()` can actually swap the
//! running binary in place depends on how the app was installed:
//!
//! - macOS / Windows: the updater always performs an in-place install.
//! - Linux: only an AppImage can be swapped in place (that's the only
//!   Linux bundle format `bundle.createUpdaterArtifacts` produces — see
//!   `tauri.linux.conf.json`). deb/rpm installs must be updated through the
//!   system package manager instead, so offering the in-app "Update" button
//!   there would just fail. The AppImage runtime sets the `APPIMAGE` env var
//!   (absolute path to the running image) on the process it launches, which
//!   is the documented way to detect "am I running from an AppImage right
//!   now" (https://docs.appimage.org/packaging-guide/environment-variables.html).

/// The frontend calls this once an update is found and only renders the
/// one-click "Update" control when it returns `true`; otherwise it links out
/// to the GitHub release instead (see `UpdatesSection` in GeneralTab.tsx).
#[tauri::command]
pub fn update_supports_self_install() -> bool {
    if cfg!(target_os = "linux") {
        std::env::var_os("APPIMAGE").is_some()
    } else {
        true
    }
}

/// Open the GitHub releases page in the user's default browser. The webview
/// blocks `target="_blank"` navigation (no opener plugin is installed), so the
/// "manual update" link goes through the OS opener instead. Restricted to the
/// releases page rather than accepting an arbitrary URL from the frontend.
#[tauri::command]
pub fn open_releases_page() -> Result<(), String> {
    const URL: &str = "https://github.com/ethannortharc/fonos/releases/latest";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err("Opening URLs is not supported on this platform".to_string());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    std::process::Command::new(opener)
        .arg(URL)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open browser: {e}"))
}

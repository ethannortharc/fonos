//! Library entry point — re-exports modules for integration tests.

pub mod audio;
pub mod commands;
pub mod adapters;
pub mod error_surface;
// macOS-only CGEventTap hotkey manager. Declared here too (not just in the bin's
// `main.rs`) so `commands::permissions::check_accessibility` can re-arm the tap
// via `crate::hotkey` in both the lib and bin compilations of this source.
#[cfg(target_os = "macos")]
pub mod hotkey;
pub mod injection;
pub mod tray;
pub mod window;

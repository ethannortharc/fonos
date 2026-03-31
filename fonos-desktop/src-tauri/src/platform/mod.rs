//! Platform abstraction layer.
//!
//! Each platform (macos, windows, linux) implements these traits.
//! The correct implementation is selected at compile time via `#[cfg(target_os)]`.

pub mod audio;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos as current;

/// Text injection into the focused application.
pub trait TextInjector: Send + Sync {
    /// Inject `text` at the current cursor position in the frontmost app.
    fn inject_text(&self, text: &str) -> Result<(), String>;

    /// Simulate pressing the Enter/Return key.
    fn press_enter(&self);
}

/// Simulate keyboard shortcuts (Cmd/Ctrl+C, Cmd/Ctrl+V).
pub trait KeySimulator: Send + Sync {
    /// Simulate a copy shortcut (Cmd+C on macOS, Ctrl+C elsewhere).
    fn simulate_copy(&self);

    /// Simulate a paste shortcut (Cmd+V on macOS, Ctrl+V elsewhere).
    fn simulate_paste(&self);
}

/// Get info about the frontmost application and manage focus.
pub trait AppFocus: Send + Sync {
    /// Return the name of the frontmost (focused) application.
    fn frontmost_app_name(&self) -> String;

    /// Bring the named application to the front.
    fn activate_app(&self, name: &str);
}

/// Cursor position for window placement.
pub trait CursorPosition {
    /// Return the current cursor position in screen coordinates.
    fn cursor_position() -> (f64, f64);
}

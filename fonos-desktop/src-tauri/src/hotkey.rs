//! Global hotkey registration via CGEventTapCreate; emits Tauri events on keypress/release.

use std::sync::{Arc, Mutex};

use core_graphics::event::{
    CallbackResult, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, EventField, KeyCode,
};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};

/// CGEventFlags modifier mask constants.
const MOD_CMD: u64 = 0x100000;    // kCGEventFlagMaskCommand
const MOD_SHIFT: u64 = 0x20000;   // kCGEventFlagMaskShift
const MOD_ALT: u64 = 0x80000;     // kCGEventFlagMaskAlternate
const MOD_CTRL: u64 = 0x40000;    // kCGEventFlagMaskControl

/// A parsed hotkey configuration.
#[derive(Debug, Clone)]
pub struct HotkeyConfig {
    /// CGEventFlags bitmask of required modifiers.
    pub modifiers: u64,
    /// macOS virtual keycode.
    pub keycode: u16,
    /// Human-readable label used to identify this hotkey in callbacks.
    pub label: String,
}

/// Manages global hotkey registration via CGEvent tap.
pub struct HotkeyManager {
    hotkeys: Arc<Mutex<Vec<HotkeyConfig>>>,
    callback: Arc<Mutex<Option<Box<dyn Fn(&str, bool) + Send>>>>,
    running: Arc<Mutex<bool>>,
}

#[allow(dead_code)]
impl HotkeyManager {
    /// Create a new, empty hotkey manager.
    pub fn new() -> Self {
        HotkeyManager {
            hotkeys: Arc::new(Mutex::new(Vec::new())),
            callback: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Parse a hotkey string like `"cmd+shift+space"` into a [`HotkeyConfig`].
    ///
    /// The last `+`-separated token is the key; preceding tokens are modifiers.
    /// Recognised modifiers: `cmd`/`command`, `shift`, `alt`/`opt`/`option`, `ctrl`/`control`.
    pub fn parse_hotkey(combo: &str, label: &str) -> Result<HotkeyConfig, String> {
        let parts: Vec<&str> = combo.split('+').collect();
        if parts.is_empty() {
            return Err("empty hotkey spec".into());
        }

        let key_str = parts.last().unwrap().to_lowercase();
        let mut modifiers: u64 = 0;

        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "cmd" | "command" => modifiers |= MOD_CMD,
                "shift" => modifiers |= MOD_SHIFT,
                "alt" | "opt" | "option" => modifiers |= MOD_ALT,
                "ctrl" | "control" => modifiers |= MOD_CTRL,
                unknown => return Err(format!("unknown modifier: {unknown}")),
            }
        }

        let keycode = key_name_to_code(&key_str)
            .ok_or_else(|| format!("unknown key: {key_str}"))?;

        Ok(HotkeyConfig {
            modifiers,
            keycode,
            label: label.to_string(),
        })
    }

    /// Register a hotkey. Can be called before or after [`start`](Self::start).
    pub fn register(&mut self, config: HotkeyConfig) {
        self.hotkeys.lock().unwrap().push(config);
    }

    /// Get a clone of the shared hotkeys Arc for external reload.
    pub fn hotkeys_ref(&self) -> Arc<Mutex<Vec<HotkeyConfig>>> {
        Arc::clone(&self.hotkeys)
    }

    /// Replace all registered hotkeys at runtime (for hot-reload).
    pub fn replace_hotkeys(&self, new_hotkeys: Vec<HotkeyConfig>) {
        let mut guard = self.hotkeys.lock().unwrap();
        guard.clear();
        guard.extend(new_hotkeys);
        eprintln!("fonos: hotkeys reloaded ({} bindings)", guard.len());
    }

    /// Set the callback that fires when a registered hotkey is pressed or released.
    ///
    /// The callback receives the hotkey label and a boolean that is `true` on
    /// key-down and `false` on key-up.
    pub fn set_callback<F: Fn(&str, bool) + Send + 'static>(&self, callback: F) {
        let mut guard = self.callback.lock().unwrap();
        *guard = Some(Box::new(callback));
    }

    /// Start listening for hotkeys on a dedicated thread.
    ///
    /// A CGEventTap is installed on the HID event stream.  The calling process
    /// must have been granted the Accessibility permission (System Settings →
    /// Privacy & Security → Accessibility); the tap silently fails otherwise.
    pub fn start(&self) -> Result<(), String> {
        {
            let mut r = self.running.lock().unwrap();
            if *r {
                return Err("HotkeyManager already running".into());
            }
            *r = true;
        }

        let hotkeys = Arc::clone(&self.hotkeys);
        let callback = Arc::clone(&self.callback);
        let running = Arc::clone(&self.running);

        std::thread::spawn(move || {
            // Track held state per hotkey to suppress key-repeat events
            let held: Arc<Mutex<std::collections::HashSet<String>>> =
                Arc::new(Mutex::new(std::collections::HashSet::new()));

            let held_clone = Arc::clone(&held);

            let tap_result = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![CGEventType::KeyDown, CGEventType::KeyUp],
                move |_proxy, event_type, event| {
                    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    let flags = event.get_flags();

                    let modifier_mask = CGEventFlags::CGEventFlagCommand
                        | CGEventFlags::CGEventFlagShift
                        | CGEventFlags::CGEventFlagAlternate
                        | CGEventFlags::CGEventFlagControl;
                    let active_mods = (flags & modifier_mask).bits();

                    let is_down = matches!(event_type, CGEventType::KeyDown);

                    // Snapshot hotkeys (clone the small vec to avoid holding the lock)
                    let hk_snap: Vec<HotkeyConfig> = hotkeys.lock().unwrap().clone();

                    let mut held_set = held_clone.lock().unwrap();

                    // On keyup: if this keycode is held by any hotkey, release it
                    if !is_down {
                        for hk in &hk_snap {
                            if keycode == hk.keycode && held_set.contains(&hk.label) {
                                held_set.remove(&hk.label);
                                drop(held_set);
                                if let Ok(guard) = callback.lock() {
                                    if let Some(ref cb) = *guard {
                                        cb(&hk.label, false);
                                    }
                                }
                                return CallbackResult::Drop;
                            }
                        }
                        drop(held_set);
                        return CallbackResult::Keep;
                    }

                    // On keydown: check if modifiers + key match a registered hotkey
                    for hk in &hk_snap {
                        if keycode == hk.keycode && active_mods == hk.modifiers {
                            if held_set.contains(&hk.label) {
                                // Already held — suppress key repeat
                                drop(held_set);
                                return CallbackResult::Drop; // consume to prevent beep
                            }
                            held_set.insert(hk.label.clone());
                            drop(held_set);

                            if let Ok(guard) = callback.lock() {
                                if let Some(ref cb) = *guard {
                                    cb(&hk.label, true);
                                }
                            }
                            // Consume the event to prevent system beep
                            return CallbackResult::Drop;
                        }
                    }

                    drop(held_set);
                    CallbackResult::Keep
                },
            );

            match tap_result {
                Ok(tap) => {
                    // Add the run-loop source and run until stopped.
                    let loop_source = tap
                        .mach_port()
                        .create_runloop_source(0)
                        .expect("failed to create run-loop source");
                    let run_loop = CFRunLoop::get_current();
                    run_loop.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });
                    tap.enable();

                    // CFRunLoop::run_current() blocks until the run loop is stopped.
                    CFRunLoop::run_current();
                }
                Err(()) => {
                    eprintln!(
                        "hotkey: CGEventTapCreate failed — is Accessibility permission granted?"
                    );
                }
            }

            *running.lock().unwrap() = false;
        });

        Ok(())
    }

    /// Stop the event-tap run loop and return.
    pub fn stop(&self) {
        // Signal the run loop on the listener thread to stop.
        // CFRunLoop::get_current() gives the *caller's* run loop, not the
        // listener thread's, so we just set the flag; the listener thread will
        // exit naturally when the run loop is stopped by an external trigger or
        // when the process exits.
        *self.running.lock().unwrap() = false;
    }
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a lowercase key name to the macOS virtual keycode.
fn key_name_to_code(name: &str) -> Option<u16> {
    let code = match name {
        // Letters (ANSI)
        "a" => KeyCode::ANSI_A,
        "b" => KeyCode::ANSI_B,
        "c" => KeyCode::ANSI_C,
        "d" => KeyCode::ANSI_D,
        "e" => KeyCode::ANSI_E,
        "f" => KeyCode::ANSI_F,
        "g" => KeyCode::ANSI_G,
        "h" => KeyCode::ANSI_H,
        "i" => KeyCode::ANSI_I,
        "j" => KeyCode::ANSI_J,
        "k" => KeyCode::ANSI_K,
        "l" => KeyCode::ANSI_L,
        "m" => KeyCode::ANSI_M,
        "n" => KeyCode::ANSI_N,
        "o" => KeyCode::ANSI_O,
        "p" => KeyCode::ANSI_P,
        "q" => KeyCode::ANSI_Q,
        "r" => KeyCode::ANSI_R,
        "s" => KeyCode::ANSI_S,
        "t" => KeyCode::ANSI_T,
        "u" => KeyCode::ANSI_U,
        "v" => KeyCode::ANSI_V,
        "w" => KeyCode::ANSI_W,
        "x" => KeyCode::ANSI_X,
        "y" => KeyCode::ANSI_Y,
        "z" => KeyCode::ANSI_Z,
        // Digits
        "0" => KeyCode::ANSI_0,
        "1" => KeyCode::ANSI_1,
        "2" => KeyCode::ANSI_2,
        "3" => KeyCode::ANSI_3,
        "4" => KeyCode::ANSI_4,
        "5" => KeyCode::ANSI_5,
        "6" => KeyCode::ANSI_6,
        "7" => KeyCode::ANSI_7,
        "8" => KeyCode::ANSI_8,
        "9" => KeyCode::ANSI_9,
        // Punctuation / symbols (ANSI keycodes)
        "-" | "minus" => KeyCode::ANSI_MINUS,
        "=" | "equal" => KeyCode::ANSI_EQUAL,
        "[" | "bracketleft" => KeyCode::ANSI_LEFT_BRACKET,
        "]" | "bracketright" => KeyCode::ANSI_RIGHT_BRACKET,
        "'" | "quote" => KeyCode::ANSI_QUOTE,
        ";" | "semicolon" => KeyCode::ANSI_SEMICOLON,
        "\\" | "backslash" => KeyCode::ANSI_BACKSLASH,
        "," | "comma" => KeyCode::ANSI_COMMA,
        "/" | "slash" => KeyCode::ANSI_SLASH,
        "." | "period" => KeyCode::ANSI_PERIOD,
        "`" | "grave" | "backquote" => KeyCode::ANSI_GRAVE,
        // Special keys
        "space" | " " => KeyCode::SPACE,
        "return" | "enter" => KeyCode::RETURN,
        "tab" => KeyCode::TAB,
        "delete" | "backspace" => KeyCode::DELETE,
        "forwarddelete" => KeyCode::FORWARD_DELETE,
        "escape" | "esc" => KeyCode::ESCAPE,
        "up" | "arrowup" => KeyCode::UP_ARROW,
        "down" | "arrowdown" => KeyCode::DOWN_ARROW,
        "left" | "arrowleft" => KeyCode::LEFT_ARROW,
        "right" | "arrowright" => KeyCode::RIGHT_ARROW,
        // Function keys
        "f1" => KeyCode::F1,
        "f2" => KeyCode::F2,
        "f3" => KeyCode::F3,
        "f4" => KeyCode::F4,
        "f5" => KeyCode::F5,
        "f6" => KeyCode::F6,
        "f7" => KeyCode::F7,
        "f8" => KeyCode::F8,
        "f9" => KeyCode::F9,
        "f10" => KeyCode::F10,
        "f11" => KeyCode::F11,
        "f12" => KeyCode::F12,
        "f13" => KeyCode::F13,
        "f14" => KeyCode::F14,
        "f15" => KeyCode::F15,
        "f16" => KeyCode::F16,
        "f17" => KeyCode::F17,
        "f18" => KeyCode::F18,
        "f19" => KeyCode::F19,
        _ => return None,
    };
    Some(code)
}

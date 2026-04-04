//! Text injection into focused UI elements via AXUIElement; clipboard paste as fallback.

use std::time::Duration;

/// Which injection method was ultimately used.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectionMethod {
    /// Direct AXUIElement value setting succeeded.
    Accessibility,
    /// Clipboard paste (Cmd+V) was used as fallback.
    ClipboardPaste,
}

/// Injects `text` at the current cursor position.
///
/// Tries the macOS Accessibility API first. If that fails (permission not
/// granted or the focused element does not expose a settable value), it falls
/// back to a clipboard-paste sequence (save clipboard → set text → Cmd+V →
/// restore clipboard).
///
/// Returns the method that succeeded, or an error string.
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

    /// Try AXUIElement-based injection (direct value setting).
    fn try_ax_injection(&self, text: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            use std::ptr;

            // Safety: These are raw C API calls to the macOS Accessibility framework.
            // We follow the Core Foundation memory management rules (Get rule vs Create rule).
            unsafe {
                // Obtain the system-wide AXUIElement.
                let system_element = ax_sys::AXUIElementCreateSystemWide();
                if system_element.is_null() {
                    return Err("AXUIElementCreateSystemWide returned null".into());
                }

                // Get the focused UI element.
                let mut focused: ax_sys::AXUIElementRef = ptr::null_mut();
                let attr_name = ax_sys::cf_string("AXFocusedUIElement");
                let err = ax_sys::AXUIElementCopyAttributeValue(
                    system_element,
                    attr_name,
                    &mut focused as *mut _ as *mut ax_sys::CFTypeRef,
                );
                ax_sys::CFRelease(system_element as ax_sys::CFTypeRef);
                ax_sys::CFRelease(attr_name as ax_sys::CFTypeRef);

                if err != 0 || focused.is_null() {
                    return Err(format!("Could not get focused element (err={})", err));
                }

                // Set kAXValueAttribute to the new text.
                let value_attr = ax_sys::cf_string("AXValue");
                let cf_text = ax_sys::cf_string_from_str(text);
                let set_err = ax_sys::AXUIElementSetAttributeValue(
                    focused,
                    value_attr,
                    cf_text as ax_sys::CFTypeRef,
                );
                ax_sys::CFRelease(focused as ax_sys::CFTypeRef);
                ax_sys::CFRelease(value_attr as ax_sys::CFTypeRef);
                ax_sys::CFRelease(cf_text as ax_sys::CFTypeRef);

                if set_err != 0 {
                    return Err(format!("AXUIElementSetAttributeValue failed (err={})", set_err));
                }

                Ok(())
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = text;
            Err("AX injection only supported on macOS".into())
        }
    }

    /// Fallback: copy to clipboard, simulate Cmd+V, restore clipboard.
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

    /// Check if the focused element supports AX value setting.
    fn focused_element_supports_value(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            unsafe { ax_sys::focused_element_has_settable_value() }
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
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

// ---------------------------------------------------------------------------
// Thin wrappers over macOS AX / CF C APIs
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod ax_sys {
    use std::ffi::{c_char, c_void, CString};
    use std::ptr;

    pub type CFTypeRef = *mut c_void;
    pub type CFStringRef = *mut c_void;
    pub type AXUIElementRef = *mut c_void;
    pub type AXError = i32;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        pub fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> AXError;
        pub fn AXIsProcessTrusted() -> bool;
        pub fn CFRelease(cf: CFTypeRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
    }

    // kCFStringEncodingUTF8 = 0x08000100
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    /// Create a CFStringRef from a static &str (UTF-8).
    pub fn cf_string(s: &str) -> CFStringRef {
        let c = CString::new(s).expect("null byte in attribute name");
        unsafe {
            CFStringCreateWithCString(ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        }
    }

    /// Create a CFStringRef from an arbitrary &str (UTF-8).
    pub fn cf_string_from_str(s: &str) -> CFStringRef {
        // For strings that may contain null bytes we would need a different API,
        // but injection text is not expected to contain null bytes.
        let c = CString::new(s).unwrap_or_else(|_| CString::new("<null>").unwrap());
        unsafe {
            CFStringCreateWithCString(ptr::null_mut(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        }
    }

    /// Returns true if the focused element exposes a settable AXValue and the
    /// process has Accessibility permission.
    pub unsafe fn focused_element_has_settable_value() -> bool {
        if !AXIsProcessTrusted() {
            return false;
        }

        // Try to copy the focused element; if that works, assume it supports
        // value-setting (the actual set may still fail, handled in try_ax_injection).
        let system_element = AXUIElementCreateSystemWide();
        if system_element.is_null() {
            return false;
        }

        let attr = cf_string("AXFocusedUIElement");
        let mut focused: CFTypeRef = std::ptr::null_mut();
        let err = AXUIElementCopyAttributeValue(
            system_element,
            attr,
            &mut focused as *mut CFTypeRef,
        );
        CFRelease(system_element as CFTypeRef);
        CFRelease(attr as CFTypeRef);

        if err != 0 || focused.is_null() {
            return false;
        }

        CFRelease(focused);
        true
    }
}

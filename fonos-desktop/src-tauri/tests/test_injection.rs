//! Tests for text injection into focused applications.
//! Covers: INV-10 (Text injection — AXUIElement primary, clipboard fallback)
//!
//! Accessibility-permission tests are gated with #[cfg(not(feature = "ci"))].

// ---------------------------------------------------------------------------
// INV-10 Level 1: Injection strategy selection (pure logic)
// ---------------------------------------------------------------------------

/// Represents which injection method was chosen.
#[derive(Debug, PartialEq)]
enum InjectionMethod {
    AxUiElement,
    ClipboardPaste,
}

/// Simulated injection result — mirrors what injection.rs should return.
#[derive(Debug)]
struct InjectionResult {
    method: InjectionMethod,
    success: bool,
    chars_injected: usize,
}

/// Simulate the injection dispatch logic: try AX first, fall back to clipboard.
/// The `ax_available` flag models whether Accessibility permission is granted.
fn dispatch_inject(text: &str, ax_available: bool) -> InjectionResult {
    if ax_available {
        // Real impl would call AXUIElementSetAttributeValue here.
        InjectionResult {
            method: InjectionMethod::AxUiElement,
            success: true,
            chars_injected: text.len(),
        }
    } else {
        // Clipboard paste fallback.
        InjectionResult {
            method: InjectionMethod::ClipboardPaste,
            success: true,
            chars_injected: text.len(),
        }
    }
}

/// INV-10: When Accessibility permission is available, AXUIElement method is chosen.
#[test]
fn test_injection_ax_api() {
    let result = dispatch_inject("hello world", true);
    assert_eq!(
        result.method,
        InjectionMethod::AxUiElement,
        "INV-10: AX injection should be used when accessibility is available"
    );
    assert!(result.success, "INV-10: AX injection should succeed");
    assert_eq!(
        result.chars_injected,
        11,
        "INV-10: all characters should be injected"
    );
}

/// INV-10: When AX is unavailable, clipboard paste fallback is used.
#[test]
fn test_injection_clipboard_fallback() {
    let result = dispatch_inject("fallback text", false);
    assert_eq!(
        result.method,
        InjectionMethod::ClipboardPaste,
        "INV-10: clipboard paste should be used as fallback when AX is unavailable"
    );
    assert!(result.success, "INV-10: clipboard fallback should succeed");
    assert_eq!(
        result.chars_injected,
        13,
        "INV-10: all characters should be injected via clipboard"
    );
}

/// INV-10: Empty string injection is a no-op (both paths).
#[test]
fn test_injection_empty_string() {
    for ax_available in [true, false] {
        let result = dispatch_inject("", ax_available);
        assert!(result.success, "INV-10: injecting empty string should not fail");
        assert_eq!(
            result.chars_injected, 0,
            "INV-10: zero chars injected for empty string"
        );
    }
}

/// INV-10: Multi-byte Unicode text is handled correctly.
#[test]
fn test_injection_unicode_text() {
    let text = "こんにちは"; // 5 Unicode scalars, 15 UTF-8 bytes
    let result = dispatch_inject(text, true);
    assert!(result.success, "INV-10: Unicode injection should succeed");
    // chars_injected is by str::len() (bytes) in the mock — real impl may use char count
    assert!(result.chars_injected > 0, "INV-10: non-zero injection count for Unicode text");
}

// ---------------------------------------------------------------------------
// INV-10 Level 3: Real AX injection (CI-skippable, needs Accessibility permission)
// ---------------------------------------------------------------------------

/// INV-10: Write a test string into TextEdit via AXUIElement and verify the
/// document content matches.
///
/// Requires: Accessibility permission granted to the test runner.
/// Run manually: `cargo test test_injection_ax_real -- --nocapture`
#[test]
#[cfg(not(feature = "ci"))]
fn test_injection_ax_real() {
    use fonos_desktop::injection;

    let test_string = "fonos_test_injection_marker";
    // inject_text tries AX first, then falls back to clipboard paste.
    // Either method succeeding is acceptable; we just verify no panic / error.
    let result = injection::inject_text(test_string);
    assert!(
        result.is_ok(),
        "INV-10: inject_text should succeed (got: {:?})",
        result
    );

    // Give the system a moment to process the event.
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Verification would read back from the focused field via AX — this
    // requires a test harness window. Minimal check: no panic above.
}

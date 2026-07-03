//! Classify raw capture→process→inject pipeline errors into short, actionable
//! messages and surface them on the float pill / activity feed.
//!
//! A raw error string (from the LLM layer, injection, mic capture, …) is
//! classified into a [`SurfacedError`]: a concise user-facing message plus an
//! optional System Settings pane to deep-link to. It is emitted as a
//! `float:error` Tauri event whose payload is JSON:
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

/// A classified error ready to surface in the UI.
///
/// Serialized to the JSON `float:error` payload described in the module docs.
pub struct SurfacedError {
    /// Short, user-facing message shown in the pill / activity feed.
    pub message: String,
    /// System Settings pane to deep-link to when this is a permission error.
    /// One of the panes accepted by `open_settings_pane` (`"microphone"`,
    /// `"accessibility"`, …), or `None` for non-permission errors.
    pub pane: Option<&'static str>,
}

/// Classify a raw pipeline error into a short actionable message and an optional
/// System Settings pane id.
///
/// Matching is case-insensitive substring matching. LLM-layer errors embed HTTP
/// status codes in their text (e.g. `"LLM API error 401: …"`,
/// `"Anthropic error 429: …"`, `"LLM request failed: …"`), so those codes drive
/// the classification. Permission / environment messages are already actionable
/// and are kept verbatim (only tagged with a pane where relevant).
pub fn classify_error(raw: &str) -> SurfacedError {
    let lower = raw.to_lowercase();
    let has = |needle: &str| lower.contains(needle);

    // ── Permission / environment errors ─────────────────────────────────────
    // Kept verbatim (they already tell the user what to do); some carry a pane.
    if has("microphone permission") {
        return SurfacedError { message: raw.to_string(), pane: Some("microphone") };
    }
    if has("no microphone found") {
        return SurfacedError { message: raw.to_string(), pane: None };
    }
    if has("accessibility permission") {
        return SurfacedError { message: raw.to_string(), pane: Some("accessibility") };
    }
    if has("secure input") {
        return SurfacedError { message: raw.to_string(), pane: None };
    }
    if has("no llm profile")
        || has("no api key in profile")
        || (has("profile") && has("not found"))
    {
        return SurfacedError { message: raw.to_string(), pane: None };
    }

    // ── LLM / provider errors ───────────────────────────────────────────────
    // Replaced with a canned actionable message; the raw cause is kept only in
    // the log (see `emit_float_error`).
    if has("401")
        || has("403")
        || has("unauthorized")
        || has("invalid api key")
        || has("invalid x-api-key")
    {
        return SurfacedError {
            message: "Invalid or missing API key — check Settings > Models".to_string(),
            pane: None,
        };
    }
    if has("429") || has("rate limit") {
        return SurfacedError {
            message: "Rate limited by the provider — try again shortly".to_string(),
            pane: None,
        };
    }
    if has("404") || has("model not found") || has("does not exist") {
        return SurfacedError {
            message: "Model not found — check the model name in Settings > Models".to_string(),
            pane: None,
        };
    }
    if has("request failed")
        || has("connection")
        || has("timed out")
        || has("timeout")
        || has("dns")
        || has("network")
    {
        return SurfacedError {
            message: "Network error reaching the provider — check your connection / endpoint"
                .to_string(),
            pane: None,
        };
    }

    // ── Fallback ────────────────────────────────────────────────────────────
    // Unknown error — surface it verbatim, truncated so it fits the pill.
    SurfacedError {
        message: raw.chars().take(200).collect(),
        pane: None,
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_401_maps_to_api_key() {
        let s = classify_error("LLM API error 401: invalid x-api-key");
        assert!(s.message.contains("API key"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn forbidden_403_maps_to_api_key() {
        let s = classify_error("Anthropic error 403: Forbidden");
        assert!(s.message.contains("API key"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn rate_limit_429_maps_to_rate_limited() {
        let s = classify_error("Anthropic error 429: rate limit exceeded");
        assert!(s.message.contains("Rate limited"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn model_not_found_404_maps_to_model_message() {
        let s = classify_error("LLM API error 404: the model `foo` does not exist");
        assert!(s.message.contains("Model not found"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn request_failed_maps_to_network() {
        let s = classify_error("LLM request failed: connection refused");
        assert!(s.message.contains("Network error"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn timeout_maps_to_network() {
        let s = classify_error("Google error: operation timed out after 30s");
        assert!(s.message.contains("Network error"), "got: {}", s.message);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn microphone_permission_keeps_message_and_sets_pane() {
        let raw = "Microphone permission denied. Grant access in System Settings > Privacy > Microphone.";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, Some("microphone"));
    }

    #[test]
    fn no_microphone_found_keeps_message_no_pane() {
        let raw = "No microphone found. Connect an audio input device.";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn accessibility_permission_sets_accessibility_pane() {
        let raw = "Injection failed: Accessibility permission not granted — Fonos can't deliver keystrokes.";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, Some("accessibility"));
    }

    #[test]
    fn secure_input_keeps_message_no_pane() {
        let raw = "A secure input field is active (likely a password field) — macOS blocks simulated input here.";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn profile_error_kept_verbatim() {
        let raw = "No LLM profile configured — pick a model in Settings.";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn profile_not_found_kept_verbatim() {
        let raw = "Profile 'stt-fast' not found";
        let s = classify_error(raw);
        assert_eq!(s.message, raw);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn unknown_error_truncated_to_200_chars() {
        let raw = "x".repeat(500);
        let s = classify_error(&raw);
        assert_eq!(s.message.chars().count(), 200);
        assert_eq!(s.pane, None);
    }

    #[test]
    fn permission_error_takes_priority_over_status_code() {
        // A message that carries both a permission phrase and a stray code must
        // classify as the permission error (checked first), not the API key one.
        let raw = "Accessibility permission not granted (error 403 while posting event)";
        let s = classify_error(raw);
        assert_eq!(s.pane, Some("accessibility"));
        assert_eq!(s.message, raw);
    }
}

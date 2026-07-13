/// AppControlSkill — open, switch, and manage applications on macOS.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that opens applications, opens URLs (optionally in a specific browser), or lists running apps.
pub struct AppControlSkill;

/// Conservative TLD allowlist used only to spot domain-shaped tokens (e.g.
/// "github.com") that were mistakenly passed as `app`. This is NOT a
/// site→URL mapping — it never resolves or guesses a URL, it only flags
/// text that looks like it belongs in `url` instead of `app`.
const DOMAIN_LIKE_TLDS: &[&str] = &[
    "com", "org", "net", "io", "dev", "co", "gov", "edu", "app", "ai", "me", "info", "biz", "us",
    "uk", "ca",
];

/// Returns `true` if `text` contains a whitespace-delimited token that looks
/// like a domain name (e.g. "github.com", "github.com/foo/bar", "google.com").
///
/// Deliberately conservative: requires a dot-separated segment whose final
/// part matches a known TLD shape, so plain app names ("Things 3", "iTerm2",
/// "Visual Studio Code") never match (they contain no dot at all).
fn contains_domain_like_token(text: &str) -> bool {
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| {
            matches!(c, '"' | '\'' | ',' | '!' | '?' | ';' | ':' | '(' | ')')
        });
        // Only look at the host portion, in case a path follows (e.g. "/foo/bar").
        let host_part = word.split('/').next().unwrap_or(word);
        let segments: Vec<&str> = host_part.split('.').collect();
        if segments.len() < 2 {
            continue;
        }
        let last = segments[segments.len() - 1].to_lowercase();
        let last_clean: String = last.chars().filter(|c| c.is_alphabetic()).collect();
        if !DOMAIN_LIKE_TLDS.contains(&last_clean.as_str()) {
            continue;
        }
        let prior = segments[segments.len() - 2];
        if !prior.is_empty() && prior.chars().next().is_some_and(|c| c.is_alphanumeric()) {
            return true;
        }
    }
    false
}

/// Returns `true` if `text` matches the shape "<X> in <Y>" — i.e. "in"
/// appears as its own word, with at least one word before and after it.
/// This is how a spoken "open <site> in <browser>" command tends to land
/// verbatim in a single parameter when the LLM fails to split it.
fn contains_x_in_y_phrase(text: &str) -> bool {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() < 3 {
        return false;
    }
    words
        .iter()
        .enumerate()
        .any(|(i, w)| *w == "in" && i > 0 && i < words.len() - 1)
}

/// Defense-in-depth validation for the `app` parameter (see module docs on
/// [`AppControlSkill`]). This never guesses a URL or knows about any
/// particular site — it only recognizes URL-shaped text or the "<X> in <Y>"
/// phrasing and asks the LLM to re-call the skill with `app`/`url` split
/// correctly.
///
/// Returns `Err(corrective message)` when `app` looks wrong; `Ok(())`
/// otherwise.
fn validate_app_param(app: &str, url: Option<&str>) -> Result<(), String> {
    let trimmed = app.trim();

    if trimmed.contains("://")
        || trimmed.to_lowercase().contains("www.")
        || contains_domain_like_token(trimmed)
    {
        return Err(format!(
            "Invalid 'app' parameter: \"{trimmed}\" looks like a website or URL, not an \
             application name. Put ONLY the application name in 'app' (e.g. \"Safari\") and the \
             website as a full URL in 'url' (e.g. \"https://example.com\"). Re-call app_open with \
             both parameters set correctly."
        ));
    }

    if url.is_none() && contains_x_in_y_phrase(trimmed) {
        return Err(format!(
            "Invalid 'app' parameter: \"{trimmed}\" looks like \"<website> in <browser>\". Split \
             it: put the browser/app name alone in 'app' and the website as a full URL in 'url'. \
             Example: to open Google in Safari, call app_open with app=\"Safari\", \
             url=\"https://google.com\" — NOT app=\"Google in Safari\". Re-call app_open with both \
             parameters set correctly."
        ));
    }

    Ok(())
}

/// Resolve common short names to their actual macOS app names.
fn resolve_app_name(name: &str) -> String {
    let name = name.trim().trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\'')
    });
    let lower = name.to_lowercase();
    match lower.as_str() {
        "chrome" | "google chrome" => "Google Chrome".into(),
        "firefox" | "mozilla firefox" => "Firefox".into(),
        "edge" | "microsoft edge" => "Microsoft Edge".into(),
        "code" | "vscode" | "vs code" => "Visual Studio Code".into(),
        "iterm" | "iterm2" => "iTerm".into(),
        "wechat" => "WeChat".into(),
        "telegram" => "Telegram".into(),
        "slack" => "Slack".into(),
        "notion" => "Notion".into(),
        "spotify" => "Spotify".into(),
        "safari" => "Safari".into(),
        "finder" => "Finder".into(),
        "terminal" => "Terminal".into(),
        "notes" => "Notes".into(),
        "music" | "apple music" => "Music".into(),
        "calendar" => "Calendar".into(),
        "mail" => "Mail".into(),
        "messages" => "Messages".into(),
        "photos" => "Photos".into(),
        "preview" => "Preview".into(),
        "activity monitor" => "Activity Monitor".into(),
        "system settings" | "system preferences" => "System Settings".into(),
        _ => name.to_string(),
    }
}

impl Skill for AppControlSkill {
    fn name(&self) -> &str {
        "app_open"
    }

    fn description(&self) -> &str {
        "Open apps, open URLs, or open a URL in a specific browser. `app` and `url` are SEPARATE \
         parameters: `app` is only ever an application name, `url` is only ever a website. \
         Example: user says \"Open Google in Safari\" → call app_open with app=\"Safari\", \
         url=\"https://google.com\" (never app=\"Google in Safari\"). Example: user says \"Open \
         Slack\" → app=\"Slack\" only, no url. Common app aliases like \"Chrome\" are resolved \
         automatically."
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![
            SkillParam {
                name: "app".into(),
                description: "Application NAME ONLY (e.g. \"Safari\", \"Terminal\", \"Visual Studio Code\"). \
                    Never put a website, URL, or a phrase like \"Google in Safari\" here — if the \
                    user wants a site opened in a browser, put the browser name here and the site \
                    in `url` instead (e.g. app=\"Safari\", url=\"https://google.com\"). Common \
                    aliases like \"Chrome\" are resolved automatically.".into(),
                required: false,
                default: None,
            },
            SkillParam {
                name: "url".into(),
                description: "Optional website to open, as a full URL (https://...). When the user \
                    asks to open a site in a browser (e.g. \"open Google in Safari\"), put the site \
                    here as a URL and the browser name in `app` — do not combine them into `app`. \
                    If `app` is also set, the URL opens in that app; otherwise it opens in the \
                    default browser.".into(),
                required: false,
                default: None,
            },
            SkillParam {
                name: "action".into(),
                description: "Action: \"open\" (default) or \"list\" running apps".into(),
                required: false,
                default: Some("open".into()),
            },
        ]
    }

    fn execute(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = fonos_core::Result<SkillOutput>> + Send + '_>>
    {
        let action = params["action"]
            .as_str()
            .unwrap_or("open")
            .to_string();
        let raw_app = params["app"].as_str().map(|s| s.to_string());
        let app = raw_app.as_deref().map(resolve_app_name);
        let url = params["url"].as_str().map(|s| s.to_string());

        Box::pin(async move {
            match action.as_str() {
                "list" => {
                    let script = r#"tell application "System Events" to get name of every process where background only is false"#;
                    let output = tokio::process::Command::new("osascript")
                        .arg("-e")
                        .arg(script)
                        .output()
                        .await
                        .map_err(|e| {
                            fonos_core::Error::Agent(format!("Failed to list apps: {e}"))
                        })?;

                    let stdout = String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string();

                    Ok(SkillOutput {
                        output: stdout,
                        structured: None,
                    })
                }
                "open" | _ => {
                    // Defense-in-depth: catch the LLM stuffing a URL or a
                    // "<site> in <browser>" phrase into `app` instead of using
                    // the dedicated `url` parameter. See `validate_app_param`.
                    if let Some(ref raw) = raw_app {
                        if let Err(msg) = validate_app_param(raw, url.as_deref()) {
                            return Err(fonos_core::Error::Agent(msg));
                        }
                    }

                    // Case 1: app + url → open URL in that specific app via AppleScript
                    if let (Some(ref app_name), Some(ref target_url)) = (&app, &url) {
                        let script = format!(
                            "tell application \"{}\" to open location \"{}\"",
                            app_name.replace('"', "\\\""),
                            target_url.replace('"', "\\\""),
                        );
                        let output = tokio::process::Command::new("osascript")
                            .arg("-e")
                            .arg(&format!("tell application \"{}\" to activate", app_name.replace('"', "\\\"")))
                            .arg("-e")
                            .arg(&script)
                            .output()
                            .await
                            .map_err(|e| {
                                fonos_core::Error::Agent(format!("Failed to open URL in {}: {e}", app_name))
                            })?;

                        if !output.status.success() {
                            // Fallback: try `open -a AppName url`
                            let fallback = tokio::process::Command::new("open")
                                .arg("-a")
                                .arg(app_name)
                                .arg(target_url)
                                .output()
                                .await
                                .map_err(|e| {
                                    fonos_core::Error::Agent(format!("Failed to open URL: {e}"))
                                })?;
                            if !fallback.status.success() {
                                let stderr = String::from_utf8_lossy(&fallback.stderr).trim_end().to_string();
                                return Err(fonos_core::Error::Agent(format!(
                                    "Could not open '{}' in {}: {stderr}", target_url, app_name
                                )));
                            }
                        }

                        Ok(SkillOutput {
                            output: format!("Opened {} in {}", target_url, app_name),
                            structured: None,
                        })
                    }
                    // Case 2: app only → open/switch to app
                    else if let Some(ref app_name) = app {
                        let output = tokio::process::Command::new("open")
                            .arg("-a")
                            .arg(app_name)
                            .output()
                            .await
                            .map_err(|e| {
                                fonos_core::Error::Agent(format!("Failed to open app: {e}"))
                            })?;

                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr)
                                .trim_end()
                                .to_string();
                            return Err(fonos_core::Error::Agent(format!(
                                "Could not open app '{}': {stderr}",
                                app_name
                            )));
                        }

                        Ok(SkillOutput {
                            output: format!("Opened {}", app_name),
                            structured: None,
                        })
                    }
                    // Case 3: url only → open in default browser
                    else if let Some(ref target_url) = url {
                        let output = tokio::process::Command::new("open")
                            .arg(target_url)
                            .output()
                            .await
                            .map_err(|e| {
                                fonos_core::Error::Agent(format!("Failed to open URL: {e}"))
                            })?;

                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr)
                                .trim_end()
                                .to_string();
                            return Err(fonos_core::Error::Agent(format!(
                                "Could not open URL '{}': {stderr}",
                                target_url
                            )));
                        }

                        Ok(SkillOutput {
                            output: format!("Opened {}", target_url),
                            structured: None,
                        })
                    } else {
                        Err(fonos_core::Error::Agent(
                            "app_open requires either 'app' or 'url' parameter for action 'open'"
                                .into(),
                        ))
                    }
                }
            }
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_desktop_skills {
    use super::*;

    #[test]
    fn test_resolve_app_names() {
        assert_eq!(resolve_app_name("chrome"), "Google Chrome");
        assert_eq!(resolve_app_name("Chrome"), "Google Chrome");
        assert_eq!(resolve_app_name("vscode"), "Visual Studio Code");
        assert_eq!(resolve_app_name("firefox"), "Firefox");
        assert_eq!(resolve_app_name(" Safari. "), "Safari");
        assert_eq!(resolve_app_name("MyCustomApp"), "MyCustomApp");
    }

    #[tokio::test]
    async fn test_app_control_list() {
        let skill = AppControlSkill;
        let result = skill
            .execute(serde_json::json!({"action": "list"}))
            .await
            .expect("app list should succeed");
        assert!(
            !result.output.trim().is_empty(),
            "Expected non-empty app list, got empty string"
        );
    }

    // ── validate_app_param ──────────────────────────────────────────────────

    #[test]
    fn test_validate_app_param_site_in_browser_without_url_is_corrective_error() {
        let err = validate_app_param("Google in Safari", None)
            .expect_err("should reject '<site> in <browser>' with no url");
        assert!(err.contains("app"), "error should mention app: {err}");
        assert!(
            err.to_lowercase().contains("url"),
            "error should point at url: {err}"
        );
    }

    #[test]
    fn test_validate_app_param_app_plus_url_is_ok() {
        assert!(validate_app_param("Safari", Some("https://google.com")).is_ok());
    }

    #[test]
    fn test_validate_app_param_multiword_app_name_is_ok() {
        assert!(validate_app_param("Visual Studio Code", None).is_ok());
    }

    #[test]
    fn test_validate_app_param_bare_domain_is_corrective_error() {
        let err = validate_app_param("github.com/foo/bar", None)
            .expect_err("should reject a bare domain/path as app");
        assert!(err.to_lowercase().contains("url"), "error was: {err}");

        let err2 =
            validate_app_param("google.com", None).expect_err("should reject bare domain");
        assert!(err2.to_lowercase().contains("url"), "error was: {err2}");
    }

    #[test]
    fn test_validate_app_param_url_scheme_is_corrective_error() {
        assert!(validate_app_param("https://example.com", None).is_err());
        assert!(validate_app_param("www.example.com", None).is_err());
    }

    #[test]
    fn test_validate_app_param_no_false_positive_on_legit_names() {
        // Names with digits/numbers but no dot or URL shape.
        assert!(validate_app_param("Things 3", None).is_ok());
        // Names with an internal dot but no recognizable TLD shape.
        assert!(validate_app_param("iTerm2", None).is_ok());
        assert!(validate_app_param("Safari", None).is_ok());
        assert!(validate_app_param("Terminal", None).is_ok());
    }

    #[test]
    fn test_validate_app_param_x_in_y_allowed_when_url_present() {
        // If the LLM already split things out (url is set), don't punish an
        // app string that happens to contain " in " for some other reason.
        assert!(validate_app_param("Google in Safari", Some("https://google.com")).is_ok());
    }

    #[tokio::test]
    async fn test_execute_rejects_site_stuffed_into_app_param() {
        let skill = AppControlSkill;
        let err = skill
            .execute(serde_json::json!({"app": "Google in Safari."}))
            .await
            .expect_err("execute should reject a site-in-app phrase");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("url"),
            "corrective error should mention url: {msg}"
        );
    }

    #[tokio::test]
    async fn test_execute_rejects_bare_url_as_app() {
        let skill = AppControlSkill;
        let err = skill
            .execute(serde_json::json!({"app": "github.com/foo/bar"}))
            .await
            .expect_err("execute should reject a bare url/domain as app");
        assert!(err.to_string().to_lowercase().contains("url"));
    }
}

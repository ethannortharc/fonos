/// AppControlSkill — open, switch, and manage applications on macOS.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that opens applications, opens URLs (optionally in a specific browser), or lists running apps.
pub struct AppControlSkill;

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
        "Open apps, open URLs, or open a URL in a specific browser. Use app+url together to open a URL in that app (e.g. app=\"Chrome\" url=\"https://google.com\"). Common names like \"Chrome\" are resolved automatically."
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![
            SkillParam {
                name: "app".into(),
                description: "Application name (e.g. \"Chrome\", \"Safari\", \"VS Code\", \"Finder\"). Common aliases resolved automatically.".into(),
                required: false,
                default: None,
            },
            SkillParam {
                name: "url".into(),
                description: "URL to open. If app is also set, opens the URL in that app. Otherwise opens in default browser.".into(),
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
        let app = params["app"].as_str().map(|s| resolve_app_name(s));
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
}

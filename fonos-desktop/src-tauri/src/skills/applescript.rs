/// AppleScriptSkill — run AppleScript for macOS automation via `osascript`.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that runs AppleScript snippets using `osascript`.
pub struct AppleScriptSkill;

impl AppleScriptSkill {
    /// Reject AppleScript that escapes into the shell or escalates privileges.
    ///
    /// AppleScript's `do shell script "..."` runs arbitrary shell commands,
    /// which would completely bypass the [`CommandSafetyFilter`] that guards the
    /// shell skill (e.g. `do shell script "rm -rf ~"`). `with administrator
    /// privileges` escalates to root. Both are blocked here so AppleScript can't
    /// be used as an unguarded backdoor around the shell safety rules.
    fn check_safe(script: &str) -> Result<(), String> {
        let lower = script.to_lowercase();
        for pattern in ["do shell script", "administrator privileges"] {
            if lower.contains(pattern) {
                return Err(format!(
                    "AppleScript containing '{}' is blocked because it can run shell commands \
                     outside the safety filter.",
                    pattern
                ));
            }
        }
        Ok(())
    }
}

impl Skill for AppleScriptSkill {
    fn name(&self) -> &str {
        "applescript"
    }

    fn description(&self) -> &str {
        "Run AppleScript for macOS automation"
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![SkillParam {
            name: "script".into(),
            description: "AppleScript code to execute".into(),
            required: true,
            default: None,
        }]
    }

    fn execute(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = fonos_core::Result<SkillOutput>> + Send + '_>>
    {
        let script = params["script"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            // Block the shell-escape / privilege-escalation vectors first.
            if let Err(reason) = Self::check_safe(&script) {
                return Err(fonos_core::Error::Agent(reason));
            }

            let output = tokio::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await
                .map_err(|e| {
                    fonos_core::Error::Agent(format!("Failed to run osascript: {e}"))
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string();
            let stderr = String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string();

            if !output.status.success() && !stderr.is_empty() {
                return Err(fonos_core::Error::Agent(format!(
                    "AppleScript error: {stderr}"
                )));
            }

            Ok(SkillOutput {
                output: stdout,
                structured: None,
            })
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_desktop_skills {
    use super::*;

    /// AppleScriptSkill: return 2 + 2 -> "4"
    #[tokio::test]
    async fn test_applescript_arithmetic() {
        let skill = AppleScriptSkill;
        let result = skill
            .execute(serde_json::json!({"script": "return 2 + 2"}))
            .await
            .expect("applescript arithmetic should succeed");
        assert_eq!(
            result.output.trim(),
            "4",
            "Expected '4', got: {:?}",
            result.output
        );
    }

    /// AppleScriptSkill: `do shell script` shell-escape is blocked.
    #[tokio::test]
    async fn test_applescript_shell_escape_blocked() {
        let skill = AppleScriptSkill;
        let err = skill
            .execute(serde_json::json!({"script": "do shell script \"echo pwned\""}))
            .await
            .expect_err("do shell script should be blocked");
        assert!(
            err.to_string().contains("blocked"),
            "Expected a blocked error, got: {err}"
        );
    }

    /// AppleScriptSkill: `with administrator privileges` escalation is blocked.
    #[tokio::test]
    async fn test_applescript_admin_privileges_blocked() {
        let skill = AppleScriptSkill;
        let err = skill
            .execute(serde_json::json!({
                "script": "do shell script \"id\" with administrator privileges"
            }))
            .await
            .expect_err("admin privileges should be blocked");
        assert!(err.to_string().contains("blocked"));
    }
}

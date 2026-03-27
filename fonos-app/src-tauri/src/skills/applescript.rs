/// AppleScriptSkill — run AppleScript for macOS automation via `osascript`.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that runs AppleScript snippets using `osascript`.
pub struct AppleScriptSkill;

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
}

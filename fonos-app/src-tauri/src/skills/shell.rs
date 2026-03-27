/// ShellSkill — execute shell commands on macOS with safety filtering and timeout.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use fonos_core::agent::safety::CommandSafetyFilter;
use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// Default command execution timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A skill that executes shell commands on macOS.
///
/// Before execution, each command is checked against the [`CommandSafetyFilter`]
/// to block dangerous operations. Commands that pass the check are run via
/// `tokio::process::Command` with a configurable timeout.
pub struct ShellSkill {
    safety: Arc<CommandSafetyFilter>,
    timeout_secs: u64,
}

impl ShellSkill {
    /// Create a new [`ShellSkill`] with the given safety filter and default timeout.
    pub fn new(safety: Arc<CommandSafetyFilter>) -> Self {
        Self {
            safety,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }

    /// Create a new [`ShellSkill`] with a custom timeout.
    pub fn with_timeout(safety: Arc<CommandSafetyFilter>, timeout_secs: u64) -> Self {
        Self { safety, timeout_secs }
    }
}

impl Skill for ShellSkill {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute shell commands on macOS"
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![SkillParam {
            name: "command".into(),
            description: "The shell command to execute".into(),
            required: true,
            default: None,
        }]
    }

    fn execute(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = fonos_core::Result<SkillOutput>> + Send + '_>>
    {
        let command = params["command"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let timeout_dur = Duration::from_secs(self.timeout_secs);
        let safety = Arc::clone(&self.safety);

        Box::pin(async move {
            // Safety check first — never execute before this passes.
            if let Err(blocked) = safety.check(&command) {
                return Err(fonos_core::Error::Agent(format!(
                    "Command blocked: {}. You can adjust safety rules in Agent Settings.",
                    blocked
                )));
            }

            // Run via shell so pipes, expansions, etc. work as expected.
            let exec = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output();

            let output = tokio::time::timeout(timeout_dur, exec)
                .await
                .map_err(|_| {
                    fonos_core::Error::Agent(format!(
                        "Command timed out after {} seconds: {}",
                        timeout_dur.as_secs(),
                        command
                    ))
                })?
                .map_err(|e| {
                    fonos_core::Error::Agent(format!("Failed to spawn command: {e}"))
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let combined = if stderr.is_empty() {
                stdout.trim_end().to_string()
            } else if stdout.is_empty() {
                stderr.trim_end().to_string()
            } else {
                format!("{}\n{}", stdout.trim_end(), stderr.trim_end())
            };

            Ok(SkillOutput {
                output: combined,
                structured: None,
            })
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_desktop_skills {
    use super::*;

    fn default_skill() -> ShellSkill {
        ShellSkill::new(Arc::new(CommandSafetyFilter::default()))
    }

    /// ShellSkill: echo hello -> output contains "hello"
    #[tokio::test]
    async fn test_shell_echo_hello() {
        let skill = default_skill();
        let result = skill
            .execute(serde_json::json!({"command": "echo hello"}))
            .await
            .expect("echo should succeed");
        assert!(
            result.output.contains("hello"),
            "Expected output to contain 'hello', got: {:?}",
            result.output
        );
    }

    /// ShellSkill: rm -rf / is blocked by the safety filter.
    #[tokio::test]
    async fn test_shell_rm_blocked() {
        let skill = default_skill();
        let err = skill
            .execute(serde_json::json!({"command": "rm -rf /"}))
            .await
            .expect_err("rm -rf / should be blocked");
        let msg = err.to_string();
        assert!(
            msg.contains("blocked") || msg.contains("Command blocked"),
            "Expected a blocked-command error, got: {msg}"
        );
    }

    /// ShellSkill: timeout fires when command takes too long.
    #[tokio::test]
    async fn test_shell_timeout() {
        // "sleep" isn't in the default allowlist, so build a custom one.
        use fonos_core::agent::safety::{CommandSafetyConfig, CommandSafetyFilter};
        let mut config = CommandSafetyConfig::empty();
        config.allowlist.push("sleep".to_string());
        let skill = ShellSkill::with_timeout(Arc::new(CommandSafetyFilter::new(config)), 1);
        let err = skill
            .execute(serde_json::json!({"command": "sleep 5"}))
            .await
            .expect_err("sleep 5 should time out");
        assert!(
            err.to_string().contains("timed out"),
            "Expected timeout error, got: {err}"
        );
    }
}

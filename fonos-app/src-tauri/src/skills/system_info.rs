/// SystemInfoSkill — query macOS system information.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that returns system information such as hostname, memory, disk usage,
/// OS version, uptime, and CPU details.
pub struct SystemInfoSkill;

impl Skill for SystemInfoSkill {
    fn name(&self) -> &str {
        "system_info"
    }

    fn description(&self) -> &str {
        "Query system information"
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![SkillParam {
            name: "query".into(),
            description: "What to query: \"hostname\", \"memory\", \"disk\", \"os\", \"uptime\", or \"cpu\"".into(),
            required: true,
            default: None,
        }]
    }

    fn execute(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = fonos_core::Result<SkillOutput>> + Send + '_>>
    {
        let query = params["query"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            let (program, args): (&str, Vec<&str>) = match query.as_str() {
                "hostname" => ("hostname", vec![]),
                "memory"   => ("vm_stat", vec![]),
                "disk"     => ("df", vec!["-h"]),
                "os"       => ("sw_vers", vec![]),
                "uptime"   => ("uptime", vec![]),
                "cpu"      => ("sysctl", vec!["-n", "machdep.cpu.brand_string"]),
                other => {
                    return Err(fonos_core::Error::Agent(format!(
                        "Unknown system_info query '{}'. Valid queries: hostname, memory, disk, os, uptime, cpu",
                        other
                    )));
                }
            };

            let output = tokio::process::Command::new(program)
                .args(&args)
                .output()
                .await
                .map_err(|e| {
                    fonos_core::Error::Agent(format!("Failed to run '{program}': {e}"))
                })?;

            let stdout = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string();
            let stderr = String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string();

            if !output.status.success() && !stderr.is_empty() {
                return Err(fonos_core::Error::Agent(format!(
                    "system_info '{}' failed: {stderr}",
                    query
                )));
            }

            let result = if stdout.is_empty() { stderr } else { stdout };

            Ok(SkillOutput {
                output: result,
                structured: None,
            })
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_desktop_skills {
    use super::*;

    /// SystemInfoSkill hostname -> returns non-empty string.
    #[tokio::test]
    async fn test_system_info_hostname() {
        let skill = SystemInfoSkill;
        let result = skill
            .execute(serde_json::json!({"query": "hostname"}))
            .await
            .expect("hostname query should succeed");
        assert!(
            !result.output.trim().is_empty(),
            "Expected non-empty hostname, got empty string"
        );
    }

    /// SystemInfoSkill disk -> returns output containing "Filesystem" or "/"
    #[tokio::test]
    async fn test_system_info_disk() {
        let skill = SystemInfoSkill;
        let result = skill
            .execute(serde_json::json!({"query": "disk"}))
            .await
            .expect("disk query should succeed");
        assert!(
            !result.output.trim().is_empty(),
            "Expected non-empty disk info"
        );
    }

    /// SystemInfoSkill os -> returns output containing product version info.
    #[tokio::test]
    async fn test_system_info_os() {
        let skill = SystemInfoSkill;
        let result = skill
            .execute(serde_json::json!({"query": "os"}))
            .await
            .expect("os query should succeed");
        assert!(
            !result.output.trim().is_empty(),
            "Expected non-empty OS info"
        );
    }

    /// SystemInfoSkill cpu -> returns processor info.
    #[tokio::test]
    async fn test_system_info_cpu() {
        let skill = SystemInfoSkill;
        let result = skill
            .execute(serde_json::json!({"query": "cpu"}))
            .await
            .expect("cpu query should succeed");
        assert!(
            !result.output.trim().is_empty(),
            "Expected non-empty CPU info"
        );
    }

    /// SystemInfoSkill with unknown query -> returns error.
    #[tokio::test]
    async fn test_system_info_unknown_query() {
        let skill = SystemInfoSkill;
        let err = skill
            .execute(serde_json::json!({"query": "blorp"}))
            .await
            .expect_err("unknown query should fail");
        assert!(
            err.to_string().contains("blorp"),
            "Expected error to mention unknown query, got: {err}"
        );
    }
}

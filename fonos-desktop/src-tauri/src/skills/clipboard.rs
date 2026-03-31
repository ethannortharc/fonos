/// ClipboardSkill — read and write clipboard contents using pbpaste/pbcopy.

use std::pin::Pin;

use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};

/// A skill that reads from and writes to the macOS clipboard.
pub struct ClipboardSkill;

impl Skill for ClipboardSkill {
    fn name(&self) -> &str {
        "clipboard"
    }

    fn description(&self) -> &str {
        "Read and write clipboard contents"
    }

    fn parameters(&self) -> Vec<SkillParam> {
        vec![
            SkillParam {
                name: "action".into(),
                description: "\"read\" to get clipboard contents, \"write\" to set them".into(),
                required: true,
                default: None,
            },
            SkillParam {
                name: "text".into(),
                description: "Text to write to clipboard (required when action is \"write\")".into(),
                required: false,
                default: None,
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
            .unwrap_or("read")
            .to_string();
        let text = params["text"].as_str().map(|s| s.to_string());

        Box::pin(async move {
            match action.as_str() {
                "read" => {
                    let output = tokio::process::Command::new("pbpaste")
                        .output()
                        .await
                        .map_err(|e| {
                            fonos_core::Error::Agent(format!("Failed to read clipboard: {e}"))
                        })?;

                    let content = String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string();

                    Ok(SkillOutput {
                        output: content,
                        structured: None,
                    })
                }
                "write" => {
                    let content = text.ok_or_else(|| {
                        fonos_core::Error::Agent(
                            "clipboard write requires 'text' parameter".into(),
                        )
                    })?;

                    // Pipe the text into pbcopy.
                    let mut child = tokio::process::Command::new("pbcopy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                        .map_err(|e| {
                            fonos_core::Error::Agent(format!("Failed to run pbcopy: {e}"))
                        })?;

                    // Write content to stdin.
                    if let Some(stdin) = child.stdin.take() {
                        use tokio::io::AsyncWriteExt;
                        let mut stdin = stdin;
                        stdin
                            .write_all(content.as_bytes())
                            .await
                            .map_err(|e| {
                                fonos_core::Error::Agent(format!(
                                    "Failed to write to pbcopy stdin: {e}"
                                ))
                            })?;
                    }

                    child.wait().await.map_err(|e| {
                        fonos_core::Error::Agent(format!("pbcopy failed: {e}"))
                    })?;

                    Ok(SkillOutput {
                        output: "Clipboard updated".into(),
                        structured: None,
                    })
                }
                other => Err(fonos_core::Error::Agent(format!(
                    "Unknown clipboard action '{}'. Use 'read' or 'write'.",
                    other
                ))),
            }
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_desktop_skills {
    use super::*;

    /// ClipboardSkill read -> returns something (may be empty on CI, but must not error).
    #[tokio::test]
    async fn test_clipboard_read() {
        let skill = ClipboardSkill;
        // Reading the clipboard should always succeed on macOS (even if empty).
        let result = skill
            .execute(serde_json::json!({"action": "read"}))
            .await
            .expect("clipboard read should not error");
        // output can be empty if clipboard is empty — that's OK.
        let _ = result.output;
    }

    /// ClipboardSkill write then read -> round-trips correctly.
    #[tokio::test]
    async fn test_clipboard_write_read_roundtrip() {
        let skill = ClipboardSkill;
        let test_text = "fonos-test-clipboard-content-12345";

        skill
            .execute(serde_json::json!({"action": "write", "text": test_text}))
            .await
            .expect("clipboard write should succeed");

        let result = skill
            .execute(serde_json::json!({"action": "read"}))
            .await
            .expect("clipboard read should succeed");

        assert!(
            result.output.contains(test_text),
            "Expected clipboard to contain '{}', got: {:?}",
            test_text,
            result.output
        );
    }
}

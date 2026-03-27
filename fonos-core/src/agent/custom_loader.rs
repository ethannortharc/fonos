//! Custom JSON skill loader for the Fonos agent system.
//!
//! This module reads `*.json` files from a directory (typically
//! `~/Library/Application Support/com.fonos.app/skills/`) and turns each
//! valid file into a [`CustomSkill`] that implements the [`Skill`] trait.
//!
//! # File format
//!
//! Each JSON file must match the [`CustomSkillConfig`] schema:
//!
//! ```json
//! {
//!   "name": "my_skill",
//!   "description": "Does something useful.",
//!   "skill_type": "shell",
//!   "command": "echo {text}",
//!   "parameters": {
//!     "text": { "description": "Text to echo", "default": "hello" }
//!   },
//!   "response_template": "Result: {output}"
//! }
//! ```
//!
//! Invalid files are skipped with a warning printed to stderr via
//! [`eprintln!`]; the loader continues processing the remaining files.

use std::{
    collections::HashMap,
    fs,
    path::Path,
    pin::Pin,
};

use serde::{Deserialize, Serialize};

use super::skill::{Skill, SkillOutput, SkillParam};

// ─── Config structs ───────────────────────────────────────────────────────────

/// Definition of a single parameter accepted by a custom skill.
///
/// Stored inside [`CustomSkillConfig::parameters`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    /// Human-readable description of what the parameter represents.
    pub description: String,
    /// Optional default value used when the caller omits this parameter.
    pub default: Option<String>,
}

/// The deserialized contents of a custom skill JSON file.
///
/// Each field corresponds directly to a key in the JSON object; unrecognised
/// keys are silently ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSkillConfig {
    /// Unique machine-readable name (used as the skill identifier).
    pub name: String,
    /// One-sentence description shown to the agent/LLM.
    pub description: String,
    /// Optional emoji or short string used as a display icon.
    pub icon: Option<String>,
    /// Execution type: `"shell"`, `"http"`, or `"script"`.
    pub skill_type: String,
    /// Shell command template for `skill_type == "shell"`.
    ///
    /// Parameter values are substituted for `{param_name}` placeholders.
    pub command: Option<String>,
    /// URL template for `skill_type == "http"`.
    ///
    /// Parameter values are substituted for `{param_name}` placeholders.
    pub url: Option<String>,
    /// Path to an executable script for `skill_type == "script"`.
    ///
    /// Parameters are passed as environment variables.
    pub script: Option<String>,
    /// Map of parameter name to its [`ParamDef`].
    #[serde(default)]
    pub parameters: HashMap<String, ParamDef>,
    /// Optional response template.
    ///
    /// `{output}` is replaced with the raw skill output before returning to
    /// the agent.  When absent the raw output is returned unchanged.
    pub response_template: Option<String>,
}

// ─── CustomSkill ─────────────────────────────────────────────────────────────

/// A runtime skill built from a [`CustomSkillConfig`].
///
/// `CustomSkill` implements the [`Skill`] trait and is returned by
/// [`load_custom_skills`].  The three supported execution backends are:
///
/// - **shell** — substitutes parameters into a command template and runs it
///   via [`tokio::process::Command`].
/// - **http** — substitutes parameters into a URL template and performs a
///   `GET` request using [`reqwest`].
/// - **script** — runs an executable script file, passing parameters as
///   environment variables.
pub struct CustomSkill {
    /// The parsed configuration from the JSON file.
    pub config: CustomSkillConfig,
    /// Pre-built list of [`SkillParam`] derived from [`CustomSkillConfig::parameters`].
    params: Vec<SkillParam>,
}

impl CustomSkill {
    /// Create a new [`CustomSkill`] from a [`CustomSkillConfig`].
    pub fn new(config: CustomSkillConfig) -> Self {
        // Build the params list once at construction time so that `parameters()`
        // can return it cheaply on every call.
        let mut params: Vec<SkillParam> = config
            .parameters
            .iter()
            .map(|(name, def)| SkillParam {
                name: name.clone(),
                description: def.description.clone(),
                required: def.default.is_none(),
                default: def.default.clone(),
            })
            .collect();
        // Sort by name for stable ordering (HashMap iteration order is arbitrary).
        params.sort_by(|a, b| a.name.cmp(&b.name));

        Self { config, params }
    }

    /// Substitute `{key}` placeholders in `template` with values from `params`.
    ///
    /// Any `{key}` whose value is not present in `params` is left as-is.
    fn substitute(template: &str, params: &serde_json::Value) -> String {
        let mut result = template.to_string();
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                let placeholder = format!("{{{key}}}");
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                result = result.replace(&placeholder, &replacement);
            }
        }
        result
    }

    /// Fill in any missing parameters using their default values.
    fn apply_defaults(&self, params: serde_json::Value) -> serde_json::Value {
        let mut map = match params {
            serde_json::Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        for (name, def) in &self.config.parameters {
            if !map.contains_key(name) {
                if let Some(ref default) = def.default {
                    map.insert(name.clone(), serde_json::Value::String(default.clone()));
                }
            }
        }
        serde_json::Value::Object(map)
    }

    /// Apply the optional response template to raw `output`.
    fn format_output(&self, output: &str) -> String {
        match &self.config.response_template {
            Some(tmpl) => tmpl.replace("{output}", output),
            None => output.to_string(),
        }
    }

    /// Execute a `shell`-type skill.
    async fn execute_shell(&self, params: serde_json::Value) -> crate::Result<SkillOutput> {
        let command_template = self.config.command.as_deref().unwrap_or("");
        let command = Self::substitute(command_template, &params);

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output()
            .await
            .map_err(|e| crate::Error::Agent(format!("shell execution failed: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let raw = if stdout.is_empty() && !stderr.is_empty() {
            stderr
        } else {
            stdout
        };

        Ok(SkillOutput {
            output: self.format_output(raw.trim()),
            structured: None,
        })
    }

    /// Execute an `http`-type skill.
    async fn execute_http(&self, params: serde_json::Value) -> crate::Result<SkillOutput> {
        let url_template = self.config.url.as_deref().unwrap_or("");
        let url = Self::substitute(url_template, &params);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| crate::Error::Http(format!("HTTP request failed: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| crate::Error::Http(format!("failed to read HTTP response: {e}")))?;

        Ok(SkillOutput {
            output: self.format_output(body.trim()),
            structured: None,
        })
    }

    /// Execute a `script`-type skill.
    async fn execute_script(&self, params: serde_json::Value) -> crate::Result<SkillOutput> {
        let script_path = self.config.script.as_deref().unwrap_or("");

        let mut cmd = tokio::process::Command::new(script_path);

        // Pass all parameters as environment variables (uppercased key names).
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                let env_key = key.to_uppercase();
                let env_val = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                cmd.env(env_key, env_val);
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| crate::Error::Agent(format!("script execution failed: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let raw = if stdout.is_empty() && !stderr.is_empty() {
            stderr
        } else {
            stdout
        };

        Ok(SkillOutput {
            output: self.format_output(raw.trim()),
            structured: None,
        })
    }
}

impl Skill for CustomSkill {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn description(&self) -> &str {
        &self.config.description
    }

    fn parameters(&self) -> Vec<SkillParam> {
        self.params.clone()
    }

    fn execute(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn std::future::Future<Output = crate::Result<SkillOutput>> + Send + '_>> {
        let params = self.apply_defaults(params);
        Box::pin(async move {
            match self.config.skill_type.as_str() {
                "shell" => self.execute_shell(params).await,
                "http" => self.execute_http(params).await,
                "script" => self.execute_script(params).await,
                other => Err(crate::Error::Agent(format!(
                    "unknown skill_type: {other}"
                ))),
            }
        })
    }
}

// ─── Loader ───────────────────────────────────────────────────────────────────

/// Load all custom skills from the given directory.
///
/// Every `*.json` file in `dir` is read and parsed as a [`CustomSkillConfig`].
/// Valid files produce a [`CustomSkill`] that implements [`Skill`].  Files that
/// cannot be read or parsed are skipped; a warning is printed to stderr via
/// [`eprintln!`].
///
/// # Returns
///
/// A `Vec` containing one [`Box<dyn Skill + Send + Sync>`] per valid JSON
/// file.  Returns an empty `Vec` if:
///
/// - `dir` does not exist,
/// - `dir` cannot be read,
/// - `dir` exists but contains no `*.json` files, or
/// - every `*.json` file is invalid.
pub fn load_custom_skills(dir: &Path) -> Vec<Box<dyn Skill + Send + Sync>> {
    // If the directory does not exist or cannot be read, return empty.
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "[fonos] warning: could not read skills directory {}: {e}",
                    dir.display()
                );
            }
            return Vec::new();
        }
    };

    let mut skills: Vec<Box<dyn Skill + Send + Sync>> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[fonos] warning: error reading directory entry: {e}");
                continue;
            }
        };

        let path = entry.path();

        // Only process *.json files.
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        // Read file contents.
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[fonos] warning: could not read skill file {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        // Parse JSON.
        let config: CustomSkillConfig = match serde_json::from_str(&contents) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[fonos] warning: invalid skill JSON in {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        skills.push(Box::new(CustomSkill::new(config)));
    }

    skills
}

/// Load all custom skills and return typed [`CustomSkill`] objects.
///
/// Unlike [`load_custom_skills`], this function returns the concrete type so
/// callers can inspect [`CustomSkillConfig`] fields directly.  It is primarily
/// useful for testing and administrative tooling.
pub fn load_custom_skills_typed(dir: &Path) -> Vec<CustomSkill> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "[fonos] warning: could not read skills directory {}: {e}",
                    dir.display()
                );
            }
            return Vec::new();
        }
    };

    let mut skills: Vec<CustomSkill> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[fonos] warning: error reading directory entry: {e}");
                continue;
            }
        };

        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[fonos] warning: could not read skill file {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        let config: CustomSkillConfig = match serde_json::from_str(&contents) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[fonos] warning: invalid skill JSON in {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        skills.push(CustomSkill::new(config));
    }

    skills
}

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Write a JSON string to `<dir>/<name>.json` and return the path.
    fn write_skill(dir: &TempDir, name: &str, json: &str) {
        let path = dir.path().join(format!("{name}.json"));
        fs::write(&path, json).expect("failed to write test skill file");
    }

    /// Minimal shell-type skill JSON.
    fn shell_skill_json() -> &'static str {
        r#"{
            "name": "echo_test",
            "description": "Echoes a message via the shell.",
            "skill_type": "shell",
            "command": "echo {message}",
            "parameters": {
                "message": { "description": "Message to echo", "default": "hello" }
            },
            "response_template": "Output: {output}"
        }"#
    }

    /// Minimal http-type skill JSON.
    fn http_skill_json() -> &'static str {
        r#"{
            "name": "fetch_url",
            "description": "Fetches the content of a URL.",
            "skill_type": "http",
            "url": "https://example.com/{path}",
            "parameters": {
                "path": { "description": "URL path segment", "default": "index" }
            }
        }"#
    }

    /// Minimal script-type skill JSON.
    fn script_skill_json() -> &'static str {
        r#"{
            "name": "run_script",
            "description": "Runs a custom script.",
            "skill_type": "script",
            "script": "/usr/local/bin/my_script.sh",
            "parameters": {
                "input": { "description": "Input passed as env var INPUT" }
            }
        }"#
    }

    // ── INV-05 tests ──────────────────────────────────────────────────────────

    /// Empty directory returns empty vec.
    #[test]
    fn test_custom_skill_loader_empty_directory() {
        let dir = TempDir::new().expect("tempdir");
        let skills = load_custom_skills(dir.path());
        assert!(skills.is_empty(), "expected no skills from empty directory");
    }

    /// Non-existent directory returns empty vec (no panic).
    #[test]
    fn test_custom_skill_loader_nonexistent_directory() {
        let path = Path::new("/tmp/__fonos_nonexistent_dir_xyz__");
        let skills = load_custom_skills(path);
        assert!(skills.is_empty());
    }

    /// A valid shell-type skill file is loaded with the correct name,
    /// description, and parameters.
    #[test]
    fn test_custom_skill_loader_shell_type() {
        let dir = TempDir::new().expect("tempdir");
        write_skill(&dir, "echo_test", shell_skill_json());

        let skills = load_custom_skills_typed(dir.path());
        assert_eq!(skills.len(), 1, "expected 1 skill");

        let skill = &skills[0];
        assert_eq!(skill.name(), "echo_test");
        assert_eq!(skill.description(), "Echoes a message via the shell.");

        // Config-level checks.
        assert_eq!(skill.config.skill_type, "shell");
        assert_eq!(skill.config.command.as_deref(), Some("echo {message}"));

        // SkillParam list checks.
        let params = skill.parameters();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "message");
        assert_eq!(params[0].description, "Message to echo");
        assert_eq!(params[0].default.as_deref(), Some("hello"));
        // Has a default → not required.
        assert!(!params[0].required);
    }

    /// A valid http-type skill file is loaded with the correct URL template.
    #[test]
    fn test_custom_skill_loader_http_type() {
        let dir = TempDir::new().expect("tempdir");
        write_skill(&dir, "fetch_url", http_skill_json());

        let skills = load_custom_skills_typed(dir.path());
        assert_eq!(skills.len(), 1, "expected 1 skill");

        let skill = &skills[0];
        assert_eq!(skill.name(), "fetch_url");
        assert!(skill.description().contains("URL"));
        assert_eq!(skill.config.skill_type, "http");
        assert_eq!(
            skill.config.url.as_deref(),
            Some("https://example.com/{path}")
        );
    }

    /// A valid script-type skill file is loaded with the correct script path.
    #[test]
    fn test_custom_skill_loader_script_type() {
        let dir = TempDir::new().expect("tempdir");
        write_skill(&dir, "run_script", script_skill_json());

        let skills = load_custom_skills_typed(dir.path());
        assert_eq!(skills.len(), 1, "expected 1 skill");

        let skill = &skills[0];
        assert_eq!(skill.name(), "run_script");
        assert_eq!(skill.config.skill_type, "script");
        assert_eq!(
            skill.config.script.as_deref(),
            Some("/usr/local/bin/my_script.sh")
        );

        // The `input` param has no default → required = true.
        let params = skill.parameters();
        assert_eq!(params.len(), 1);
        assert!(params[0].required);
    }

    /// An invalid JSON file is skipped; other valid skills are still loaded.
    #[test]
    fn test_custom_skill_loader_invalid_json_skipped() {
        let dir = TempDir::new().expect("tempdir");
        // One valid skill.
        write_skill(&dir, "valid_skill", shell_skill_json());
        // One file with broken JSON.
        write_skill(&dir, "broken_skill", r#"{ this is not valid json }"#);

        let skills = load_custom_skills(dir.path());
        // Only the valid skill should be loaded.
        assert_eq!(skills.len(), 1, "broken file should be skipped");
        assert_eq!(skills[0].name(), "echo_test");
    }

    /// Multiple valid skill files all load correctly.
    #[test]
    fn test_custom_skill_loader_multiple_skills() {
        let dir = TempDir::new().expect("tempdir");
        write_skill(&dir, "skill_a", shell_skill_json());
        write_skill(&dir, "skill_b", http_skill_json());
        write_skill(&dir, "skill_c", script_skill_json());

        let skills = load_custom_skills(dir.path());
        assert_eq!(skills.len(), 3, "all three valid skills should load");

        let names: Vec<&str> = skills.iter().map(|s| s.name()).collect();
        assert!(names.contains(&"echo_test"));
        assert!(names.contains(&"fetch_url"));
        assert!(names.contains(&"run_script"));
    }

    /// Non-JSON files in the directory are ignored.
    #[test]
    fn test_custom_skill_loader_ignores_non_json_files() {
        let dir = TempDir::new().expect("tempdir");
        write_skill(&dir, "valid_skill", shell_skill_json());
        // Write a non-JSON file.
        fs::write(dir.path().join("notes.txt"), "this is not a skill")
            .expect("failed to write txt file");

        let skills = load_custom_skills(dir.path());
        assert_eq!(skills.len(), 1, "only JSON files should be loaded");
    }

    /// Shell skill execute() with a response_template wraps output correctly.
    #[tokio::test]
    async fn test_custom_skill_shell_execute_with_response_template() {
        let config: CustomSkillConfig =
            serde_json::from_str(shell_skill_json()).expect("valid json");
        let skill = CustomSkill::new(config);

        let output = skill
            .execute(serde_json::json!({"message": "world"}))
            .await
            .expect("execute should succeed");

        // The command is `echo world`; the response_template is `Output: {output}`.
        assert!(
            output.output.starts_with("Output:"),
            "response_template should be applied; got: {}",
            output.output
        );
        assert!(
            output.output.contains("world"),
            "output should contain echoed message; got: {}",
            output.output
        );
    }

    /// Default values are applied when a parameter is omitted from execute().
    #[tokio::test]
    async fn test_custom_skill_shell_execute_uses_defaults() {
        let config: CustomSkillConfig =
            serde_json::from_str(shell_skill_json()).expect("valid json");
        let skill = CustomSkill::new(config);

        // No params supplied → should use default "hello".
        let output = skill
            .execute(serde_json::json!({}))
            .await
            .expect("execute should succeed with defaults");

        assert!(
            output.output.contains("hello"),
            "default value should be used; got: {}",
            output.output
        );
    }

    /// Unknown skill_type returns a descriptive error.
    #[tokio::test]
    async fn test_custom_skill_unknown_type_returns_error() {
        let config: CustomSkillConfig = serde_json::from_str(
            r#"{
                "name": "bad",
                "description": "bad skill",
                "skill_type": "ftp",
                "parameters": {}
            }"#,
        )
        .expect("valid json");
        let skill = CustomSkill::new(config);

        let err = skill
            .execute(serde_json::json!({}))
            .await
            .expect_err("unknown type should return error");
        assert!(err.to_string().contains("unknown skill_type"));
    }

    /// Config round-trips through JSON serialisation without data loss.
    #[test]
    fn test_custom_skill_config_serialise_roundtrip() {
        let config: CustomSkillConfig =
            serde_json::from_str(shell_skill_json()).expect("deserialise");
        let json = serde_json::to_string(&config).expect("serialise");
        let config2: CustomSkillConfig =
            serde_json::from_str(&json).expect("deserialise again");
        assert_eq!(config2.name, config.name);
        assert_eq!(config2.skill_type, config.skill_type);
        assert_eq!(config2.command, config.command);
        assert_eq!(config2.response_template, config.response_template);
    }

    /// Skills with an icon field round-trip correctly.
    #[test]
    fn test_custom_skill_config_with_icon() {
        let json = r#"{
            "name": "icon_skill",
            "description": "Has an icon.",
            "icon": "🔧",
            "skill_type": "shell",
            "command": "echo hi",
            "parameters": {}
        }"#;
        let config: CustomSkillConfig = serde_json::from_str(json).expect("deserialise");
        assert_eq!(config.icon.as_deref(), Some("🔧"));
    }
}

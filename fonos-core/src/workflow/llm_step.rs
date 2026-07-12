//! The `llm` processor component: [`LlmProps`] (a widget's persisted
//! configuration) and [`run_llm_step`], the pure step function that turns
//! `props` + input text into processed text.
//!
//! This module has no dependency on [`crate::workflow::registry`] — it is a
//! plain async function operating on already-resolved inputs
//! ([`crate::llm::ServiceConfig`], a pre-built glossary string), so it can be
//! unit tested without a `Registry`/`RunCtx`. A later task's desktop-side
//! factory is responsible for parsing a widget's `props` JSON into
//! [`LlmProps`], resolving `model_profile` to a `ServiceConfig`, computing
//! the glossary block from `vocab_books`, and wrapping this function as a
//! [`crate::workflow::registry::Processor`] impl.

use serde::{Deserialize, Serialize};

/// Configuration for an `llm` processor widget.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProps {
    /// Optional system prompt.
    #[serde(default)]
    pub system: Option<String>,
    /// Optional user message template; `{text}` is substituted by
    /// [`crate::llm::process_text`]. Defaults to `Some("{text}")` so a
    /// from-scratch `llm` widget whose form left the template untouched still
    /// deserializes to a runnable template instead of `None` (which
    /// `process_text` rejects at run time).
    #[serde(default = "default_user_template")]
    pub user_template: Option<String>,
    /// Model profile id to resolve into a [`crate::llm::ServiceConfig`].
    /// Empty string means "use the global LLM profile" — resolved by the
    /// desktop factory, not this module.
    #[serde(default)]
    pub model_profile: String,
    /// LLM sampling temperature (0.0 = deterministic).
    #[serde(default = "default_temp")]
    pub temperature: f64,
    /// Maximum tokens to request from the LLM.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Desired output language; `"auto"` means preserve the input language.
    #[serde(default = "default_lang")]
    pub output_language: String,
    /// Vocab book ids mounted by this widget, in addition to the global
    /// books. Consumed by the desktop factory when computing the glossary
    /// block passed to [`run_llm_step`]; not read directly here.
    #[serde(default)]
    pub vocab_books: Vec<String>,
}

fn default_user_template() -> Option<String> {
    Some("{text}".to_string())
}
fn default_temp() -> f64 {
    0.1
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_lang() -> String {
    "auto".to_string()
}

/// Merge `glossary` into `props.system`, the same assembly
/// [`run_llm_step`] hands to [`crate::llm::process_text`]'s `system`
/// argument, using the same concatenation convention as the legacy
/// dictation path (`commands/llm.rs::process_with_llm`, deleted in Workbench
/// P2 Task 12): `glossary` is expected to already carry its own leading
/// blank line (see [`crate::vocab::build_glossary_block`]), so when
/// `props.system` is `None` the leading whitespace is trimmed instead of
/// leaving a dangling blank line at the start of the prompt.
fn merged_system(props: &LlmProps, glossary: Option<&str>) -> Option<String> {
    match (props.system.as_deref(), glossary) {
        (Some(sys), Some(block)) => Some(format!("{sys}{block}")),
        (None, Some(block)) => Some(block.trim_start().to_string()),
        (Some(sys), None) => Some(sys.to_string()),
        (None, None) => None,
    }
}

/// Run one LLM processor step: merge `glossary` into `props.system` (see
/// [`merged_system`]), send `text` through [`crate::llm::process_text`] with
/// the rest of `props`' fields passed straight through, and return the
/// trimmed response text.
///
/// Prompt assembly is delegated to [`merged_system`] (unit tested directly);
/// this function's own untested surface is the network call itself. Returns
/// `Err` if the LLM call fails or returns an empty response.
pub async fn run_llm_step(
    props: &LlmProps,
    text: &str,
    service: &crate::llm::ServiceConfig,
    translate_target: &str,
    glossary: Option<&str>,
) -> Result<String, String> {
    let system = merged_system(props, glossary);
    let resp = crate::llm::process_text(
        text,
        system.as_deref(),
        props.user_template.as_deref(),
        props.temperature,
        props.max_tokens,
        &props.output_language,
        service,
        None,
        translate_target,
    )
    .await
    .map_err(|e| e.to_string())?;
    if resp.text.is_empty() {
        return Err("llm step: empty response".to_string());
    }
    Ok(resp.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_props_serde_defaults_from_empty_object() {
        let props: LlmProps = serde_json::from_str("{}").unwrap();
        assert_eq!(props.system, None);
        assert_eq!(props.user_template.as_deref(), Some("{text}"));
        assert_eq!(props.model_profile, "");
        assert_eq!(props.temperature, 0.1);
        assert_eq!(props.max_tokens, 4096);
        assert_eq!(props.output_language, "auto");
        assert!(props.vocab_books.is_empty());
    }

    #[test]
    fn llm_props_without_user_template_defaults_to_text_placeholder() {
        // A from-scratch `llm` widget saved with an untouched template omits
        // `user_template` from its props JSON; it must still deserialize to a
        // runnable template rather than `None`.
        let json = r#"{"system":"You are helpful.","model_profile":""}"#;
        let props: LlmProps = serde_json::from_str(json).unwrap();
        assert_eq!(props.user_template.as_deref(), Some("{text}"));
    }

    fn base_props() -> LlmProps {
        LlmProps {
            system: Some("You are a polish assistant.".into()),
            user_template: Some("<<<\n{text}\n>>>".into()),
            model_profile: String::new(),
            temperature: 0.4,
            max_tokens: 1234,
            output_language: "English".into(),
            vocab_books: vec!["book1".into()],
        }
    }

    #[test]
    fn merged_system_without_glossary_passes_system_through() {
        let props = base_props();
        let system = merged_system(&props, None);
        assert_eq!(system.as_deref(), Some("You are a polish assistant."));
    }

    #[test]
    fn merged_system_appends_glossary_to_existing_system() {
        let props = base_props();
        let block = "\n\nDomain vocabulary: Kubernetes, gRPC.";
        let system = merged_system(&props, Some(block));
        assert_eq!(
            system.as_deref(),
            Some("You are a polish assistant.\n\nDomain vocabulary: Kubernetes, gRPC.")
        );
    }

    #[test]
    fn merged_system_glossary_with_no_system_trims_leading_blank_line() {
        let mut props = base_props();
        props.system = None;
        let block = "\n\nDomain vocabulary: Kubernetes, gRPC.";
        let system = merged_system(&props, Some(block));
        assert_eq!(
            system.as_deref(),
            Some("Domain vocabulary: Kubernetes, gRPC.")
        );
    }

    #[test]
    fn merged_system_no_system_no_glossary_is_none() {
        let mut props = base_props();
        props.system = None;
        let system = merged_system(&props, None);
        assert_eq!(system, None);
    }
}

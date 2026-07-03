//! Custom vocabulary — user-defined vocab books with terms and correction
//! rules (issue #3).
//!
//! A `VocabBook` bundles domain vocabulary (`terms`, correct spellings used
//! for recognition biasing and LLM glossaries) with deterministic
//! post-transcription correction `rules` (find → replace). Books are
//! standalone entities referenced by id from two places:
//!
//! * `AppConfig.global_vocab_books` — applied to every dictation
//! * `Mode.vocab_books` — extra books mounted by a specific mode
//!
//! The effective set for a dictation is `global ∪ mode`, order-preserving
//! and deduplicated. One book feeds three pipeline stages: STT prompt
//! biasing ([`build_stt_prompt`]), deterministic correction
//! ([`apply_rules`]), and LLM system-prompt glossaries
//! ([`build_glossary_block`]).

use serde::{Deserialize, Serialize};

/// How a correction rule's `from` pattern is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    /// `from` is matched literally (word-boundary-guarded for ASCII words).
    #[default]
    Literal,
    /// `from` is a regular expression.
    Regex,
}

/// A single find → replace correction applied to transcripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VocabRule {
    /// Text or pattern to find.
    pub from: String,
    /// Replacement text. For regex rules, capture groups (`$1`) work.
    pub to: String,
    /// Literal (default) or regex matching.
    pub kind: RuleKind,
    /// Case-insensitive matching (default true).
    pub case_insensitive: bool,
}

impl Default for VocabRule {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            kind: RuleKind::Literal,
            case_insensitive: true,
        }
    }
}

/// A named vocabulary: domain terms plus correction rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VocabBook {
    /// Stable identifier referenced from config / modes.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Disabled books are ignored everywhere without being deleted.
    pub enabled: bool,
    /// Correct spellings of domain terms (one entry per term).
    pub terms: Vec<String>,
    /// Deterministic post-transcription corrections, applied in order.
    pub rules: Vec<VocabRule>,
}

impl Default for VocabBook {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            terms: Vec::new(),
            rules: Vec::new(),
        }
    }
}

/// Resolve the effective books for a dictation: `global_ids` first, then
/// `mode_ids`, order-preserving, deduplicated, disabled books skipped.
pub fn effective_books<'a>(
    books: &'a [VocabBook],
    global_ids: &[String],
    mode_ids: &[String],
) -> Vec<&'a VocabBook> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for id in global_ids.iter().chain(mode_ids.iter()) {
        if seen.contains(&id.as_str()) {
            continue;
        }
        seen.push(id.as_str());
        if let Some(book) = books.iter().find(|b| b.id == *id && b.enabled) {
            out.push(book);
        }
    }
    out
}

/// All terms across `books`, deduplicated, order preserving.
pub fn collect_terms(books: &[&VocabBook]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for book in books {
        for term in &book.terms {
            let t = term.trim();
            if !t.is_empty() && !out.iter().any(|e| e == t) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// Apply every rule of every book to `text`, in book order then rule order.
///
/// A rule that fails to compile (bad user regex) is skipped — corrections
/// must never break transcription.
pub fn apply_rules(text: &str, books: &[&VocabBook]) -> String {
    let mut out = text.to_string();
    for book in books {
        for rule in &book.rules {
            if rule.from.is_empty() {
                continue;
            }
            match compile_rule(rule) {
                Some(re) => out = re.replace_all(&out, rule.to.as_str()).into_owned(),
                None => eprintln!(
                    "fonos: vocab rule '{}' in book '{}' failed to compile — skipped",
                    rule.from, book.name
                ),
            }
        }
    }
    out
}

/// Build the regex for one rule. Literal rules are escaped and, when the
/// pattern starts/ends with an ASCII word character, guarded with `\b` so
/// "art" doesn't rewrite "start". CJK text has no word boundaries, so
/// non-ASCII-word edges match as plain substrings.
fn compile_rule(rule: &VocabRule) -> Option<regex::Regex> {
    let pattern = match rule.kind {
        RuleKind::Regex => rule.from.clone(),
        RuleKind::Literal => {
            let escaped = regex::escape(&rule.from);
            let is_word = |c: Option<char>| c.map(|c| c.is_ascii_alphanumeric() || c == '_').unwrap_or(false);
            let lead = if is_word(rule.from.chars().next()) { r"\b" } else { "" };
            let trail = if is_word(rule.from.chars().last()) { r"\b" } else { "" };
            format!("{lead}{escaped}{trail}")
        }
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(rule.case_insensitive)
        .build()
        .ok()
}

/// Character budget for STT prompts. Whisper truncates its prompt to ~224
/// tokens; this keeps the merged prompt safely inside that window.
pub const STT_PROMPT_BUDGET_CHARS: usize = 600;

/// Merge vocabulary terms into an STT initial prompt.
///
/// Keeps the mode's own `base` prompt intact and appends a glossary sentence
/// with as many terms as fit in `budget_chars` (in effective-book order, so
/// global books get priority).
pub fn build_stt_prompt(base: &str, terms: &[String], budget_chars: usize) -> String {
    let base = base.trim();
    if terms.is_empty() {
        return base.to_string();
    }
    const GLOSSARY_PREFIX: &str = "Vocabulary: ";
    let mut remaining = budget_chars
        .saturating_sub(base.chars().count())
        .saturating_sub(GLOSSARY_PREFIX.len() + 2);
    let mut kept: Vec<&str> = Vec::new();
    for term in terms {
        let cost = term.chars().count() + 2; // ", " separator
        if cost > remaining {
            break;
        }
        remaining -= cost;
        kept.push(term);
    }
    if kept.is_empty() {
        return base.to_string();
    }
    let glossary = format!("{}{}.", GLOSSARY_PREFIX, kept.join(", "));
    if base.is_empty() {
        glossary
    } else {
        format!("{base} {glossary}")
    }
}

/// Glossary block appended to LLM system prompts (stage ③). Returns `None`
/// when there is nothing to add.
pub fn build_glossary_block(terms: &[String]) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    Some(format!(
        "\n\nDomain vocabulary: {}. The transcript may render these as sound-alike \
         mis-transcriptions (including English terms heard as similar-sounding words \
         in another language, e.g. a Chinese homophone standing in for an English \
         term). When context indicates one of these terms was meant, restore the \
         exact spelling listed here.",
        terms.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: &str, terms: &[&str], rules: Vec<VocabRule>) -> VocabBook {
        VocabBook {
            id: id.into(),
            name: id.into(),
            enabled: true,
            terms: terms.iter().map(|s| s.to_string()).collect(),
            rules,
        }
    }

    fn lit(from: &str, to: &str) -> VocabRule {
        VocabRule { from: from.into(), to: to.into(), ..Default::default() }
    }

    #[test]
    fn literal_rule_case_insensitive_word_boundary() {
        let b = book("t", &[], vec![lit("my sequel", "MySQL")]);
        let out = apply_rules("I use My Sequel and my sequel daily", &[&b]);
        assert_eq!(out, "I use MySQL and MySQL daily");
    }

    #[test]
    fn literal_rule_respects_word_boundaries() {
        let b = book("t", &[], vec![lit("art", "ART")]);
        assert_eq!(apply_rules("art starts here", &[&b]), "ART starts here");
    }

    #[test]
    fn literal_rule_case_sensitive_when_disabled() {
        let rule = VocabRule { case_insensitive: false, ..lit("k8s", "K8s") };
        let b = book("t", &[], vec![rule]);
        assert_eq!(apply_rules("k8s and K8S", &[&b]), "K8s and K8S");
    }

    #[test]
    fn cjk_rule_matches_as_substring() {
        let b = book("t", &[], vec![lit("库伯内提斯", "Kubernetes")]);
        assert_eq!(apply_rules("我们用库伯内提斯部署", &[&b]), "我们用Kubernetes部署");
    }

    #[test]
    fn regex_rule_with_capture_group() {
        let rule = VocabRule {
            from: r"k8\s+s".into(),
            to: "k8s".into(),
            kind: RuleKind::Regex,
            case_insensitive: true,
        };
        let b = book("t", &[], vec![rule]);
        assert_eq!(apply_rules("deploy to K8 s now", &[&b]), "deploy to k8s now");
    }

    #[test]
    fn invalid_regex_is_skipped_not_fatal() {
        let bad = VocabRule { from: "([".into(), to: "x".into(), kind: RuleKind::Regex, case_insensitive: true };
        let b = book("t", &[], vec![bad, lit("foo", "bar")]);
        assert_eq!(apply_rules("foo", &[&b]), "bar");
    }

    #[test]
    fn rules_apply_in_book_then_rule_order() {
        let b1 = book("a", &[], vec![lit("alpha", "beta")]);
        let b2 = book("b", &[], vec![lit("beta", "gamma")]);
        assert_eq!(apply_rules("alpha", &[&b1, &b2]), "gamma");
    }

    #[test]
    fn effective_books_dedup_and_skip_disabled() {
        let mut disabled = book("off", &["X"], vec![]);
        disabled.enabled = false;
        let books = vec![book("g", &["G"], vec![]), disabled, book("m", &["M"], vec![])];
        let eff = effective_books(
            &books,
            &["g".into(), "off".into()],
            &["m".into(), "g".into(), "missing".into()],
        );
        let ids: Vec<&str> = eff.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["g", "m"]);
    }

    #[test]
    fn collect_terms_dedups_preserving_order() {
        let b1 = book("a", &["Kubernetes", "fonos"], vec![]);
        let b2 = book("b", &["fonos", " Istio "], vec![]);
        assert_eq!(collect_terms(&[&b1, &b2]), vec!["Kubernetes", "fonos", "Istio"]);
    }

    #[test]
    fn stt_prompt_appends_glossary_within_budget() {
        let terms = vec!["Kubernetes".to_string(), "kubectl".to_string()];
        let out = build_stt_prompt("Tech dictation.", &terms, STT_PROMPT_BUDGET_CHARS);
        assert_eq!(out, "Tech dictation. Vocabulary: Kubernetes, kubectl.");
    }

    #[test]
    fn stt_prompt_truncates_terms_to_budget() {
        let terms: Vec<String> = (0..100).map(|i| format!("term{i:02}")).collect();
        let out = build_stt_prompt("", &terms, 60);
        assert!(out.chars().count() <= 60, "was {} chars: {out}", out.chars().count());
        assert!(out.starts_with("Vocabulary: term00"));
    }

    #[test]
    fn stt_prompt_unchanged_without_terms() {
        assert_eq!(build_stt_prompt("base", &[], 600), "base");
        assert_eq!(build_stt_prompt("", &[], 600), "");
    }

    #[test]
    fn glossary_block_none_when_empty() {
        assert!(build_glossary_block(&[]).is_none());
        let block = build_glossary_block(&["MySQL".to_string()]).unwrap();
        assert!(block.contains("MySQL"));
    }

    #[test]
    fn book_serde_defaults_are_backward_compatible() {
        let b: VocabBook = serde_json::from_str(r#"{"id":"x","name":"X"}"#).unwrap();
        assert!(b.enabled && b.terms.is_empty() && b.rules.is_empty());
        let r: VocabRule = serde_json::from_str(r#"{"from":"a","to":"b"}"#).unwrap();
        assert_eq!(r.kind, RuleKind::Literal);
        assert!(r.case_insensitive);
    }
}

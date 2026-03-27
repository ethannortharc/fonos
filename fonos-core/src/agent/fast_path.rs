//! Fast-path matcher — resolves common commands without an LLM round-trip.
//!
//! The [`FastPathMatcher`] checks user input against a small set of well-known
//! patterns. When a pattern matches, it returns the target skill name and
//! pre-built parameters directly, bypassing the planner entirely.  Inputs that
//! do not match return [`FastPathResult::None`] and are forwarded to the LLM.

use serde_json::{json, Value};

// ── Result type ──────────────────────────────────────────────────────────────

/// The outcome of a fast-path matching attempt.
#[derive(Debug, PartialEq)]
pub enum FastPathResult {
    /// The input matched a known pattern and maps to a specific skill.
    Skill {
        /// The name of the skill that should handle this request.
        skill_name: String,
        /// JSON parameters to pass to the skill.
        params: Value,
    },
    /// The input did not match any fast-path pattern; route to the LLM planner.
    None,
}

// ── Matcher ──────────────────────────────────────────────────────────────────

/// A pattern-based pre-filter that resolves simple, deterministic commands
/// without calling the LLM.
///
/// Each rule is checked in registration order; the first match wins.
/// All comparisons are case-insensitive.
pub struct FastPathMatcher {
    rules: Vec<Box<dyn PatternRule + Send + Sync>>,
}

impl FastPathMatcher {
    /// Create a new [`FastPathMatcher`] pre-loaded with all built-in rules.
    pub fn new() -> Self {
        let mut m = Self { rules: Vec::new() };
        m.register_builtin_rules();
        m
    }

    /// Register a custom rule at the end of the rule list.
    pub fn add_rule(&mut self, rule: impl PatternRule + Send + Sync + 'static) {
        self.rules.push(Box::new(rule));
    }

    /// Match `text` against all registered rules and return the first hit.
    ///
    /// Returns [`FastPathResult::None`] when no rule matches (the caller should
    /// forward the input to the LLM planner).
    pub fn match_input(&self, text: &str) -> FastPathResult {
        let lower = text.trim().to_lowercase();
        for rule in &self.rules {
            if let Some(result) = rule.try_match(&lower, text.trim()) {
                return result;
            }
        }
        FastPathResult::None
    }

    // ── built-in rule registration ────────────────────────────────────────

    fn register_builtin_rules(&mut self) {
        // "open <url>" — must come before generic app-open so URL wins
        self.add_rule(OpenUrlRule);
        // "open <app>"
        self.add_rule(OpenAppRule);
        // IP address queries
        self.add_rule(IpAddressRule);
        // Time queries
        self.add_rule(CurrentTimeRule);
        // Current user queries
        self.add_rule(CurrentUserRule);
    }
}

impl Default for FastPathMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ── Rule trait ────────────────────────────────────────────────────────────────

/// A single pattern rule evaluated by [`FastPathMatcher`].
///
/// `lower` is the already-lowercased, trimmed input; `original` is the
/// trimmed original (preserving capitalisation for captured values).
pub trait PatternRule {
    /// Try to match the input.  Return `Some(FastPathResult)` on a match,
    /// `None` to pass through to the next rule.
    fn try_match(&self, lower: &str, original: &str) -> Option<FastPathResult>;
}

// ── Built-in rules ────────────────────────────────────────────────────────────

/// Matches `open <url>` when the argument looks like a URL (contains `http`,
/// `https`, or a `www.` prefix).
struct OpenUrlRule;

impl PatternRule for OpenUrlRule {
    fn try_match(&self, lower: &str, original: &str) -> Option<FastPathResult> {
        let arg = strip_prefix(lower, "open ")?;
        // Only treat as URL if it has a URL-like shape.
        if arg.contains("http://")
            || arg.contains("https://")
            || arg.starts_with("www.")
        {
            // Preserve original capitalisation for the URL.
            let url = strip_prefix_preserve_case(original, "open ")?;
            Some(FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "url": url }),
            })
        } else {
            Option::None
        }
    }
}

/// Matches `open <AppName>` (non-URL variant).
struct OpenAppRule;

impl PatternRule for OpenAppRule {
    fn try_match(&self, lower: &str, original: &str) -> Option<FastPathResult> {
        let _ = strip_prefix(lower, "open ")?;
        // Capture the app name with original capitalisation.
        let app = strip_prefix_preserve_case(original, "open ")?;
        Some(FastPathResult::Skill {
            skill_name: "app_open".to_string(),
            params: json!({ "app": app }),
        })
    }
}

/// Matches IP-address queries ("what's my ip", "my ip address", etc.).
struct IpAddressRule;

impl PatternRule for IpAddressRule {
    fn try_match(&self, lower: &str, _original: &str) -> Option<FastPathResult> {
        let triggers = [
            "what's my ip",
            "what is my ip",
            "whats my ip",
            "my ip address",
            "what's my ip address",
            "what is my ip address",
            "show my ip",
            "show ip address",
        ];
        if triggers.iter().any(|t| lower.contains(t)) {
            Some(FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "ifconfig | grep 'inet '" }),
            })
        } else {
            Option::None
        }
    }
}

/// Matches time queries ("what time is it", "current time", etc.).
struct CurrentTimeRule;

impl PatternRule for CurrentTimeRule {
    fn try_match(&self, lower: &str, _original: &str) -> Option<FastPathResult> {
        let triggers = [
            "what time is it",
            "what's the time",
            "what is the time",
            "current time",
            "tell me the time",
        ];
        if triggers.iter().any(|t| lower.contains(t)) {
            Some(FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "date" }),
            })
        } else {
            Option::None
        }
    }
}

/// Matches current-user queries ("who am i", "current user", etc.).
struct CurrentUserRule;

impl PatternRule for CurrentUserRule {
    fn try_match(&self, lower: &str, _original: &str) -> Option<FastPathResult> {
        let triggers = [
            "who am i",
            "who am I",
            "current user",
            "what's my username",
            "what is my username",
            "show current user",
        ];
        if triggers.iter().any(|t| lower.contains(t)) {
            Some(FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "whoami" }),
            })
        } else {
            Option::None
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Strip a lowercase prefix from an already-lowercase string.
fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix).map(str::trim)
}

/// Strip a prefix (case-insensitively) from the original-case string.
fn strip_prefix_preserve_case<'a>(original: &'a str, prefix: &str) -> Option<&'a str> {
    if original.len() < prefix.len() {
        return Option::None;
    }
    // We already know the lowercase version starts with `prefix` (caller verified),
    // so we can safely slice by byte length.
    Some(original[prefix.len()..].trim())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_fast_path {
    use super::*;

    fn matcher() -> FastPathMatcher {
        FastPathMatcher::new()
    }

    // ── app_open — named application ──────────────────────────────────────

    #[test]
    fn open_safari() {
        let r = matcher().match_input("open Safari");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "app": "Safari" }),
            }
        );
    }

    #[test]
    fn open_terminal() {
        let r = matcher().match_input("open Terminal");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "app": "Terminal" }),
            }
        );
    }

    #[test]
    fn open_app_case_insensitive() {
        // The "open" keyword must be matched case-insensitively.
        let r = matcher().match_input("Open Finder");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "app": "Finder" }),
            }
        );
    }

    #[test]
    fn open_app_all_caps_trigger() {
        let r = matcher().match_input("OPEN Notes");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "app": "Notes" }),
            }
        );
    }

    // ── app_open — URL ────────────────────────────────────────────────────

    #[test]
    fn open_http_url() {
        let r = matcher().match_input("open http://example.com");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "url": "http://example.com" }),
            }
        );
    }

    #[test]
    fn open_https_url() {
        let r = matcher().match_input("open https://github.com");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "url": "https://github.com" }),
            }
        );
    }

    #[test]
    fn open_www_url() {
        let r = matcher().match_input("open www.google.com");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "app_open".to_string(),
                params: json!({ "url": "www.google.com" }),
            }
        );
    }

    // ── shell — IP address ────────────────────────────────────────────────

    #[test]
    fn whats_my_ip_apostrophe() {
        let r = matcher().match_input("what's my IP");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "ifconfig | grep 'inet '" }),
            }
        );
    }

    #[test]
    fn my_ip_address() {
        let r = matcher().match_input("my IP address");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "ifconfig | grep 'inet '" }),
            }
        );
    }

    #[test]
    fn what_is_my_ip_address() {
        let r = matcher().match_input("What is my IP address?");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "ifconfig | grep 'inet '" }),
            }
        );
    }

    // ── shell — current time ──────────────────────────────────────────────

    #[test]
    fn what_time_is_it() {
        let r = matcher().match_input("what time is it");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "date" }),
            }
        );
    }

    #[test]
    fn current_time() {
        let r = matcher().match_input("current time");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "date" }),
            }
        );
    }

    #[test]
    fn what_time_is_it_mixed_case() {
        let r = matcher().match_input("What Time Is It?");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "date" }),
            }
        );
    }

    // ── shell — current user ──────────────────────────────────────────────

    #[test]
    fn who_am_i() {
        let r = matcher().match_input("who am I");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "whoami" }),
            }
        );
    }

    #[test]
    fn current_user() {
        let r = matcher().match_input("current user");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "whoami" }),
            }
        );
    }

    #[test]
    fn who_am_i_lowercase() {
        let r = matcher().match_input("who am i");
        assert_eq!(
            r,
            FastPathResult::Skill {
                skill_name: "shell".to_string(),
                params: json!({ "command": "whoami" }),
            }
        );
    }

    // ── FastPathResult::None — complex / unmatched inputs ─────────────────

    #[test]
    fn translate_returns_none() {
        let r = matcher().match_input("translate hello to Chinese");
        assert_eq!(r, FastPathResult::None);
    }

    #[test]
    fn tell_me_a_joke_returns_none() {
        let r = matcher().match_input("tell me a joke");
        assert_eq!(r, FastPathResult::None);
    }

    #[test]
    fn complex_query_returns_none() {
        let r = matcher().match_input("summarise the last email I received");
        assert_eq!(r, FastPathResult::None);
    }

    #[test]
    fn empty_string_returns_none() {
        let r = matcher().match_input("");
        assert_eq!(r, FastPathResult::None);
    }

    #[test]
    fn whitespace_only_returns_none() {
        let r = matcher().match_input("   ");
        assert_eq!(r, FastPathResult::None);
    }
}

//! Setup Doctor — platform-independent configuration health checks (issue #30).
//!
//! This module holds the *config-lint* half of the Setup Doctor: every check
//! that can be answered from an already-loaded [`AppConfig`] plus the resolved
//! set of [`Mode`]s and vocab books, with **no network, disk, or OS access**.
//! Network / permission / RTF probes live in the desktop shell command layer
//! (`commands::doctor`), which merges its findings with [`lint_config`]'s.
//!
//! Findings speak in *message keys* ([`Finding::message_key`]) rather than
//! localized strings — the frontend owns translation (`doctor.*` namespace) and
//! substitutes [`Finding::message_params`] positionally (`{0}`, `{1}`, …). One-
//! click fixes are the typed [`FixAction`] enum; [`apply_config_fix`] applies
//! the config-only variants and is unit-tested here, while mode-level and OS
//! variants are handled by the shell.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::vocab;
use crate::workflow::engine::{effective_widgets, effective_workflows};
use crate::workflow::llm_step::LlmProps;
use crate::workflow::model::{WidgetDef, WorkflowDef};

/// How serious a [`Finding`] is. Drives the row's status circle in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The check passed — a reassuring green ✓ row.
    Pass,
    /// A silent functional failure the user almost certainly did not intend
    /// (amber `!`): something is configured but will not take effect.
    Warn,
    /// An experience suggestion (red-ish `↯`): things work, but could be better.
    Advise,
}

/// A one-click remediation for a [`Finding`].
///
/// Config-only variants are applied by [`apply_config_fix`]; `OpenSettingsPane`
/// is an OS deep-link (handled by the frontend / permissions command) rather
/// than a config mutation.
///
/// `ResetListenMode` and `PointModeModelToDefault` were retired in Workbench
/// P2 Task 11 along with the checks that produced them (`listen_mode_unknown`,
/// `mode_model_missing`) — both were mode-system checks, and `lint_config` no
/// longer takes a `modes` map at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixAction {
    /// Add a vocab book id to `AppConfig.global_vocab_books` so it applies everywhere.
    AttachBookGlobal {
        /// Id of the vocab book to attach globally.
        book_id: String,
    },
    /// Clear a dangling top-level profile reference (set the field to `""`, which
    /// makes it fall back to the default profile). `field` is one of
    /// `listen_voice_profile`, `sts_llm_profile` (`sts_voice_profile`'s
    /// dangling-ref check was retired in Workbench P2 Task 14 — see
    /// `check_workflow_refs_over`'s doc comment).
    ClearProfileRef {
        /// The `AppConfig` field name to clear.
        field: String,
    },
    /// Switch a TTS model profile to a faster model discovered on the server.
    SwitchTtsModel {
        /// Id of the model profile to update.
        profile_id: String,
        /// The new model id to set on that profile.
        model: String,
    },
    /// Open an OS System Settings privacy pane (permissions can't be granted in-app).
    OpenSettingsPane {
        /// Pane key understood by the permissions command
        /// (`microphone`, `accessibility`, …).
        pane: String,
    },
}

/// A single Setup Doctor result: one row in the resident health card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Stable, machine-readable id (also used by the frontend to key rows and to
    /// route permission findings to the right settings pane).
    pub id: String,
    /// Severity — [`Severity::Pass`] rows are the green "all good" rows.
    pub severity: Severity,
    /// `doctor.*` translation key; the frontend renders it in the active locale.
    pub message_key: String,
    /// Positional substitutions for `message_key` (`{0}`, `{1}`, …).
    pub message_params: Vec<String>,
    /// Optional one-click fix; `None` renders a plain informational row.
    pub fix: Option<FixAction>,
}

impl Finding {
    /// Construct a passing finding (no fix).
    fn pass(id: &str, message_key: &str) -> Self {
        Finding {
            id: id.to_string(),
            severity: Severity::Pass,
            message_key: message_key.to_string(),
            message_params: Vec::new(),
            fix: None,
        }
    }
}

/// Run every pure config-lint check over `config`.
///
/// Workbench P2 Task 11: every check now speaks the workflow-engine world
/// (`config.workflows` / `config.widgets`, resolved through
/// [`effective_workflows`] / [`effective_widgets`]) instead of the legacy
/// `modes.json` map — the engine superseded modes back in Workflow P1, and
/// Task 12 has since deleted `modes.rs` entirely, so `lint_config` no longer
/// takes a `modes` parameter at all.
///
/// Returns findings in display order: one [`Severity::Pass`] finding per
/// healthy category, or one-or-more problem findings when a category has
/// issues.
pub fn lint_config(config: &AppConfig) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(check_hotkeys(config));
    out.extend(check_vocab(config));
    out.extend(check_workflow_refs(config));
    out.extend(check_llm_configured(config));
    out
}

// ── hotkey conflicts ────────────────────────────────────────────────────────
//
// Which keys the OS actually *grabs* is NOT the legacy `config.hotkey_*` fields.
// The workflow migration (`workflow::migrate`) moved every legacy value onto a
// workflow Hotkey trigger chip and `std::mem::take`-cleared the source field, so
// on every migrated machine (i.e. every real one) those fields are empty and a
// check that reads them can never find a conflict — "No hotkey conflicts" comes
// back vacuously true. The live binding set is the one the desktop's
// `main.rs::hotkey_bindings` registers: one binding per Hotkey chip on every
// effective workflow, plus the floating pill's own key. fonos-core can't depend
// on the desktop crate, but it owns every input needed to re-derive that same
// set, which is what this check now inspects.

/// The live hotkey binding set as `(label, combo)`, mirroring what
/// `main.rs::hotkey_bindings` registers but labeled for humans: every non-empty
/// `Trigger::Hotkey` chip on every [`effective_workflows`] entry (tagged with the
/// workflow's display name, suffixed with the chip's trigger index only when a
/// workflow carries more than one hotkey chip), followed by the floating pill's
/// own `config.pill_hotkey`. This is the exact set the OS grabs, so it is the set
/// the conflict checks must reason about — not the migration-emptied legacy
/// fields.
fn hotkey_bindings(config: &AppConfig) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for wf in effective_workflows(config) {
        // Disambiguate with the chip index only when a workflow has several
        // hotkey chips, so the common single-chip case reads as a bare name.
        let multi = wf.hotkey_triggers().filter(|(_, c, _)| !c.is_empty()).count() > 1;
        for (idx, combo, _capture) in wf.hotkey_triggers() {
            if combo.is_empty() {
                continue;
            }
            let label = if multi { format!("{} #{idx}", wf.name) } else { wf.name.clone() };
            out.push((label, combo.to_string()));
        }
    }
    // The floating pill owns a global key separate from any recipe's chips
    // (Workbench P1, spec §3c), registered under the desktop's `"pill"` arm.
    if !config.pill_hotkey.is_empty() {
        out.push(("pill".to_string(), config.pill_hotkey.clone()));
    }
    out
}

/// Canonicalize a combo so spelling / case / alias variants compare equal:
/// `"Cmd+Shift+A"`, `"command+shift+a"`, and `"Shift+Command+A"` all collapse to
/// one string. Split on `+`; the final token is the key, earlier tokens are
/// modifiers alias-folded to canonical names (`command`→`cmd`, `control`→`ctrl`,
/// `option`/`opt`→`alt`), then sorted + deduped so modifier order is irrelevant.
/// Returns `None` for an empty (unbound) combo.
///
/// When `fold_cmd_ctrl` is set, `cmd` is additionally mapped to `ctrl` *before*
/// sorting: on Linux the desktop layer sends `cmd` through `CommandOrControl`,
/// which X11 resolves to Control (see `to_plugin_shortcut`'s doc comment in
/// `main.rs`), so `cmd` and `ctrl` become the same physical grab. Pure and
/// `cfg!`-free, so both the plain and folded forms are unit-testable on macOS.
fn canonical_combo(combo: &str, fold_cmd_ctrl: bool) -> Option<String> {
    let combo = combo.trim();
    if combo.is_empty() {
        return None;
    }
    let parts: Vec<&str> = combo.split('+').filter(|p| !p.trim().is_empty()).collect();
    let (key_tok, mod_toks) = parts.split_last()?; // parts non-empty ⇒ Some
    let key = key_tok.trim().to_lowercase();
    let mut mods: Vec<&'static str> = mod_toks
        .iter()
        .map(|m| match m.trim().to_lowercase().as_str() {
            "command" | "cmd" => "cmd",
            "option" | "opt" | "alt" => "alt",
            "control" | "ctrl" => "ctrl",
            "shift" => "shift",
            _ => "?",
        })
        .map(|m| if fold_cmd_ctrl && m == "cmd" { "ctrl" } else { m })
        .collect();
    mods.sort_unstable();
    mods.dedup();
    Some(format!("{}+{}", mods.join("+"), key))
}

/// Group `bindings` by canonical combo and flag any combo claimed by more than
/// one binding — the same physical key bound to two actions, where the OS
/// registers whichever comes first and silently drops the loser (logged only).
///
/// Pure. The finding shape is unchanged from the pre-fix check (Warn, id
/// `hotkey_dup:{canonical}`, message `doctor.duplicate_hotkey` carrying the raw
/// combo) — only its *input* moved, from the dead legacy fields to the live
/// binding set.
fn collect_hotkey_conflicts(bindings: &[(String, String)]) -> Vec<Finding> {
    let mut groups: BTreeMap<String, Vec<&(String, String)>> = BTreeMap::new();
    for b in bindings {
        if let Some(canon) = canonical_combo(&b.1, false) {
            groups.entry(canon).or_default().push(b);
        }
    }
    groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(canon, members)| Finding {
            id: format!("hotkey_dup:{canon}"),
            severity: Severity::Warn,
            message_key: "doctor.duplicate_hotkey".to_string(),
            message_params: vec![members[0].1.trim().to_string()],
            fix: None,
        })
        .collect()
}

/// The Linux-only overlay: pairs of bindings that do NOT collide as typed but DO
/// collide once `cmd` folds into `ctrl` at the X11 grab layer. Reported
/// separately from [`collect_hotkey_conflicts`] because the failure is
/// platform-specific and needs its own explanation — on macOS `cmd+…` and
/// `ctrl+…` are two distinct keys and both work; only on Linux does the second
/// registration silently fail (first-registered wins, loser only logged). Pairs
/// that already collide *pre*-collapse are left to the regular check, never
/// double-reported here.
///
/// Pure and `cfg!`-free so the macOS test suite exercises it; only the *call
/// site* in [`check_hotkeys`] is `cfg!(target_os = "linux")`-gated — this repo's
/// convention for platform-split logic (cf. `main.rs::non_mic_fires_on_key_up`).
fn collect_linux_collapse_conflicts(bindings: &[(String, String)]) -> Vec<Finding> {
    // Bucket by the cmd→ctrl-folded canonical form.
    let mut groups: BTreeMap<String, Vec<&(String, String)>> = BTreeMap::new();
    for b in bindings {
        if let Some(folded) = canonical_combo(&b.1, true) {
            groups.entry(folded).or_default().push(b);
        }
    }
    let mut out = Vec::new();
    for (folded, members) in groups {
        // Keep one representative binding per *distinct un-folded* form. Fewer
        // than two means every member already shares an un-folded form — a plain
        // duplicate the regular check owns — so there is no collapse-only clash.
        let mut reps: BTreeMap<String, &(String, String)> = BTreeMap::new();
        for m in members {
            if let Some(plain) = canonical_combo(&m.1, false) {
                reps.entry(plain).or_insert(m);
            }
        }
        if reps.len() < 2 {
            continue;
        }
        let picks: Vec<&(String, String)> = reps.into_values().collect();
        let (a, b) = (picks[0], picks[1]);
        out.push(Finding {
            id: format!("hotkey_linux_collapse:{folded}"),
            severity: Severity::Warn,
            message_key: "doctor.hotkey_linux_collapse".to_string(),
            message_params: vec![
                a.0.clone(),
                a.1.trim().to_string(),
                b.0.clone(),
                b.1.trim().to_string(),
            ],
            fix: None,
        });
    }
    out
}

/// Flag any live hotkey bound to more than one action. The input is
/// [`hotkey_bindings`] — the keys the OS actually grabs (workflow chips + pill) —
/// NOT the legacy `config.hotkey_*` fields, which the workflow migration empties
/// and no runtime path reads.
fn check_hotkeys(config: &AppConfig) -> Vec<Finding> {
    let bindings = hotkey_bindings(config);
    let mut problems = collect_hotkey_conflicts(&bindings);
    // Only Linux collapses `cmd` into `ctrl` at the X11 grab, so only there can
    // two macOS-distinct combos fight over a single registration.
    if cfg!(target_os = "linux") {
        problems.extend(collect_linux_collapse_conflicts(&bindings));
    }
    if problems.is_empty() {
        vec![Finding::pass("hotkeys_ok", "doctor.hotkeys_ok")]
    } else {
        problems.sort_by(|a, b| a.id.cmp(&b.id));
        problems
    }
}

// ── vocabulary attachment + budget ──────────────────────────────────────────

/// Every book id referenced by any `llm.*` widget's `vocab_books` prop — the
/// engine-world equivalent of the retired per-mode `vocab_books` (Workbench
/// P2 Task 11). Widgets whose `props` don't deserialize as [`LlmProps`] are
/// skipped rather than treated as an error; a malformed widget is its own,
/// separate problem.
fn widget_book_ids(config: &AppConfig) -> Vec<String> {
    let mut ids = Vec::new();
    for w in effective_widgets(config) {
        if w.type_tag != "llm" {
            continue;
        }
        let Ok(props) = serde_json::from_value::<LlmProps>(w.props) else { continue };
        for id in props.vocab_books {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// Vocab books enabled but attached nowhere, plus the STT prompt-budget check.
fn check_vocab(config: &AppConfig) -> Vec<Finding> {
    let enabled: Vec<&vocab::VocabBook> =
        config.vocab_books.iter().filter(|b| b.enabled).collect();
    if enabled.is_empty() {
        return Vec::new();
    }

    let widget_ids = widget_book_ids(config);
    let is_attached = |id: &str| {
        config.global_vocab_books.iter().any(|g| g == id) || widget_ids.iter().any(|m| m == id)
    };

    let mut problems: Vec<Finding> = Vec::new();
    for book in &enabled {
        if !is_attached(&book.id) {
            let name = if book.name.trim().is_empty() { book.id.clone() } else { book.name.clone() };
            problems.push(Finding {
                id: format!("vocab_unattached:{}", book.id),
                severity: Severity::Warn,
                message_key: "doctor.vocab_unattached".to_string(),
                message_params: vec![name],
                fix: Some(FixAction::AttachBookGlobal { book_id: book.id.clone() }),
            });
        }
    }

    // Budget: the effective set of attached, enabled books whose terms feed the
    // STT prompt. Over budget ⇒ trailing terms are silently dropped.
    let effective = vocab::effective_books(&config.vocab_books, &config.global_vocab_books, &widget_ids);
    let terms = vocab::collect_terms(&effective);
    let total_chars: usize = terms.join(", ").chars().count();
    if total_chars > vocab::STT_PROMPT_BUDGET_CHARS {
        problems.push(Finding {
            id: "vocab_budget".to_string(),
            severity: Severity::Warn,
            message_key: "doctor.vocab_budget".to_string(),
            message_params: vec![total_chars.to_string(), vocab::STT_PROMPT_BUDGET_CHARS.to_string()],
            fix: None,
        });
    }

    if problems.is_empty() {
        vec![Finding::pass("vocab_ok", "doctor.vocab_ok")]
    } else {
        problems
    }
}

// ── dangling model / profile / widget references ────────────────────────────

/// Ids of all configured model profiles.
fn profile_ids(config: &AppConfig) -> Vec<String> {
    config
        .model_profiles
        .iter()
        .filter_map(|p| p["id"].as_str().map(|s| s.to_string()))
        .collect()
}

/// The engine-world equivalents of the retired per-mode reference checks
/// (Workbench P2 Task 11):
///
/// * `llm.listen` must resolve — Listen always reads this widget directly by
///   id ([`crate::workflow::engine::effective_widgets`], Workbench P2 Task
///   10), replacing the old `listen_mode`-must-resolve check now that
///   `listen_mode` has no reader left anywhere.
/// * Dangling profile references — `listen_voice_profile` / `sts_llm_profile`
///   are still genuinely live-read (Listen synthesis and the `call.default`
///   composite's fallback chain, respectively) so these two checks are kept.
///   `sts_voice_profile`'s twin check was retired in Workbench P2 Task 14:
///   `commands::doctor::check_conversation_rtf`'s RTF probe — its only
///   remaining live reader — was repointed at `call.default`'s own
///   `CallProps` instead, so a dangling `sts_voice_profile` no longer breaks
///   anything a doctor check should warn about.
/// * Dangling widget refs in workflows — every effective workflow's source /
///   processor / output ids must resolve to an effective widget, the
///   structural equivalent of a mode pointing at a missing model profile.
/// * Dangling widget-level refs — an `llm.*` widget's own `model_profile` or
///   `vocab_books` pointing at something that no longer exists, the
///   per-widget equivalent of the old per-mode `mode_model_missing` /
///   `mode_vocab_missing` checks.
fn check_workflow_refs(config: &AppConfig) -> Vec<Finding> {
    check_workflow_refs_over(config, &effective_widgets(config), &effective_workflows(config))
}

/// [`check_workflow_refs`]'s body, taking the effective widget/workflow lists
/// as parameters so tests can exercise cases the real built-ins can never
/// produce (e.g. `llm.listen` absent — every real `effective_widgets` output
/// includes it, since built-ins are the overlay's base and are never removed
/// by it).
fn check_workflow_refs_over(
    config: &AppConfig,
    widgets: &[WidgetDef],
    workflows: &[WorkflowDef],
) -> Vec<Finding> {
    let profile_ids = profile_ids(config);
    let is_profile = |id: &str| profile_ids.iter().any(|p| p == id);
    let is_widget = |id: &str| widgets.iter().any(|w| w.id == id);
    let mut problems: Vec<Finding> = Vec::new();

    // llm.listen must resolve.
    if !is_widget("llm.listen") {
        problems.push(Finding {
            id: "llm_listen_missing".to_string(),
            severity: Severity::Warn,
            message_key: "doctor.llm_listen_missing".to_string(),
            message_params: Vec::new(),
            fix: None,
        });
    }

    // Dangling profile references. Empty = intentional fallback, so only flag
    // non-empty ids that don't resolve.
    for (field, value, key) in [
        ("listen_voice_profile", &config.listen_voice_profile, "doctor.dangling_listen_voice"),
        ("sts_llm_profile", &config.sts_llm_profile, "doctor.dangling_sts_llm"),
    ] {
        if !value.trim().is_empty() && !is_profile(value.trim()) {
            problems.push(Finding {
                id: format!("dangling_ref:{field}"),
                severity: Severity::Warn,
                message_key: key.to_string(),
                message_params: Vec::new(),
                fix: Some(FixAction::ClearProfileRef { field: field.to_string() }),
            });
        }
    }

    // Dangling widget refs: every workflow's source/processors/outputs id
    // must resolve to an effective widget.
    for wf in workflows {
        let mut refs: Vec<&str> = vec![wf.source.as_str()];
        refs.extend(wf.processors.iter().map(String::as_str));
        refs.extend(wf.outputs.iter().map(String::as_str));
        for id in refs {
            if !is_widget(id) {
                problems.push(Finding {
                    id: format!("dangling_widget_ref:{}:{}", wf.id, id),
                    severity: Severity::Warn,
                    message_key: "doctor.dangling_widget_ref".to_string(),
                    message_params: vec![wf.name.clone(), id.to_string()],
                    fix: None,
                });
            }
        }
    }

    // Per-widget references (llm widgets only).
    for w in widgets {
        if w.type_tag != "llm" {
            continue;
        }
        let Ok(props) = serde_json::from_value::<LlmProps>(w.props.clone()) else { continue };
        if !props.model_profile.trim().is_empty() && !is_profile(props.model_profile.trim()) {
            problems.push(Finding {
                id: format!("widget_model_missing:{}", w.id),
                severity: Severity::Warn,
                message_key: "doctor.widget_model_missing".to_string(),
                message_params: vec![w.name.clone(), props.model_profile.clone()],
                fix: None,
            });
        }
        for book_id in &props.vocab_books {
            if !config.vocab_books.iter().any(|b| &b.id == book_id) {
                problems.push(Finding {
                    id: format!("widget_vocab_missing:{}:{book_id}", w.id),
                    severity: Severity::Warn,
                    message_key: "doctor.widget_vocab_missing".to_string(),
                    message_params: vec![w.name.clone(), book_id.clone()],
                    fix: None,
                });
            }
        }
    }

    if problems.is_empty() {
        vec![Finding::pass("refs_ok", "doctor.refs_ok")]
    } else {
        problems
    }
}

// ── LLM configured for prompted widgets ─────────────────────────────────────

/// Warn when LLM-prompted `llm.*` widgets exist but no LLM profile can serve
/// them — the engine-world equivalent of the old prompted-modes check
/// (Workbench P2 Task 11).
fn check_llm_configured(config: &AppConfig) -> Vec<Finding> {
    let llm_props: Vec<LlmProps> = effective_widgets(config)
        .into_iter()
        .filter(|w| w.type_tag == "llm")
        .filter_map(|w| serde_json::from_value(w.props).ok())
        .collect();
    let has_prompted = llm_props.iter().any(|p| p.system.is_some() || p.user_template.is_some());
    if !has_prompted {
        return Vec::new();
    }

    // A prompted widget works if the global LLM profile is set, or the widget
    // carries its own model_profile override. Problem only when neither holds.
    let global_ok = !config.llm_profile.trim().is_empty();
    let stranded = !global_ok
        && llm_props.iter().any(|p| {
            (p.system.is_some() || p.user_template.is_some()) && p.model_profile.trim().is_empty()
        });

    if stranded {
        vec![Finding {
            id: "llm_unconfigured".to_string(),
            severity: Severity::Warn,
            message_key: "doctor.llm_unconfigured".to_string(),
            message_params: Vec::new(),
            fix: None,
        }]
    } else {
        vec![Finding::pass("llm_ok", "doctor.llm_ok")]
    }
}

// ── fix application (config-only variants) ──────────────────────────────────

/// Apply a config-only [`FixAction`] to `config` in place.
///
/// Handles [`FixAction::AttachBookGlobal`], [`FixAction::ClearProfileRef`],
/// and [`FixAction::SwitchTtsModel`]. Returns `Err` for
/// [`FixAction::OpenSettingsPane`], which the shell must handle, so callers
/// fail loudly rather than silently no-op.
pub fn apply_config_fix(config: &mut AppConfig, fix: &FixAction) -> Result<(), String> {
    match fix {
        FixAction::AttachBookGlobal { book_id } => {
            if !config.global_vocab_books.iter().any(|b| b == book_id) {
                config.global_vocab_books.push(book_id.clone());
            }
            Ok(())
        }
        FixAction::ClearProfileRef { field } => match field.as_str() {
            "listen_voice_profile" => {
                config.listen_voice_profile.clear();
                Ok(())
            }
            "sts_llm_profile" => {
                config.sts_llm_profile.clear();
                Ok(())
            }
            other => Err(format!("ClearProfileRef: unknown field '{other}'")),
        },
        FixAction::SwitchTtsModel { profile_id, model } => {
            let target = config
                .model_profiles
                .iter_mut()
                .find(|p| p["id"].as_str() == Some(profile_id.as_str()));
            match target {
                Some(p) => {
                    p["model"] = serde_json::Value::String(model.clone());
                    Ok(())
                }
                None => Err(format!("SwitchTtsModel: no profile '{profile_id}'")),
            }
        }
        FixAction::OpenSettingsPane { .. } => {
            Err("fix is not a config mutation — handled by the shell".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::VocabBook;
    use crate::workflow::model::{Trigger, WidgetRole};
    use serde_json::json;

    fn cfg() -> AppConfig {
        // A clean slate. The legacy `config.hotkey_*` fields are no longer read
        // by any doctor check (the workflow migration empties them and moves
        // their values onto workflow Hotkey chips), so we no longer bother
        // clearing them here — a test that wants a hotkey binding sets it on
        // `config.workflows` (a Hotkey chip) or `config.pill_hotkey`.
        AppConfig::default()
    }

    fn book(id: &str, enabled: bool) -> VocabBook {
        VocabBook { id: id.into(), name: id.into(), enabled, terms: vec![], rules: vec![] }
    }

    fn profile(id: &str) -> serde_json::Value {
        json!({ "id": id, "name": id, "provider": "omlx", "model": "m", "capabilities": ["tts"] })
    }

    fn find<'a>(v: &'a [Finding], id: &str) -> Option<&'a Finding> {
        v.iter().find(|f| f.id == id)
    }

    /// A custom `llm.*` processor widget with the given props.
    fn llm_widget(id: &str, props: serde_json::Value) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props,
            builtin: false,
        }
    }

    /// A minimal, valid workflow: `source` → `outputs`, no processors.
    fn workflow(id: &str, source: &str, outputs: &[&str]) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            name: id.to_string(),
            icon: String::new(),
            hotkey: String::new(),
            triggers: Vec::new(),
            source: source.to_string(),
            processors: Vec::new(),
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
            builtin: false,
        }
    }

    /// A workflow carrying one `Trigger::Hotkey` chip on `combo`, named `name`.
    /// A new id (not a built-in), so `effective_workflows` appends it — the only
    /// hotkey chip in the effective set (built-ins carry only PillSlot chips).
    fn hk_workflow(id: &str, name: &str, combo: &str) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            name: name.to_string(),
            icon: String::new(),
            hotkey: String::new(),
            triggers: vec![Trigger::Hotkey { combo: combo.to_string(), capture: None }],
            source: "src.selection".to_string(),
            processors: Vec::new(),
            outputs: vec!["out.panel".to_string()],
            builtin: false,
        }
    }

    #[test]
    fn duplicate_hotkey_across_workflows_flagged_normalized() {
        // Two workflow chips on the same combo, spelled differently — the fix's
        // whole point: the conflict lives on Hotkey chips, not legacy fields.
        let mut c = cfg();
        c.workflows = vec![
            hk_workflow("wf.a", "Alpha", "Cmd+Shift+E"),
            hk_workflow("wf.b", "Bravo", "command+shift+e"),
        ];
        let f = check_hotkeys(&c);
        let dup = f.iter().find(|f| f.id.starts_with("hotkey_dup:")).expect("dup finding");
        assert_eq!(dup.severity, Severity::Warn);
        assert_eq!(dup.message_key, "doctor.duplicate_hotkey");
        assert!(f.iter().all(|f| f.severity != Severity::Pass));
    }

    #[test]
    fn workflow_chip_colliding_with_pill_flagged() {
        // A recipe's Hotkey chip and the floating pill's own key are the same
        // combo (case-insensitively) — a real conflict the old field-based check
        // could never see (the pill key isn't a legacy field).
        let mut c = cfg();
        c.workflows = vec![hk_workflow("wf.a", "Alpha", "cmd+shift+space")];
        c.pill_hotkey = "Command+Shift+Space".into();
        let f = check_hotkeys(&c);
        let dup = f.iter().find(|f| f.id.starts_with("hotkey_dup:")).expect("dup finding");
        assert_eq!(dup.severity, Severity::Warn);
    }

    #[test]
    fn distinct_hotkeys_pass() {
        let mut c = cfg();
        c.workflows = vec![
            hk_workflow("wf.a", "Alpha", "cmd+shift+space"),
            hk_workflow("wf.b", "Bravo", "option+l"),
        ];
        c.pill_hotkey = "ctrl+alt+p".into();
        let f = check_hotkeys(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "hotkeys_ok");
        assert_eq!(f[0].severity, Severity::Pass);
    }

    #[test]
    fn legacy_hotkey_fields_are_not_consulted() {
        // Post-migration every legacy field is empty; even a populated one (a
        // config predating the migration, or a hand-edit) can't be a conflict
        // because no doctor check reads it any more. A single legacy value alone
        // therefore produces the passing finding, never a `hotkey_dup:` warning.
        let mut c = cfg();
        c.hotkey_dictation = "cmd+shift+space".into();
        let f = check_hotkeys(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "hotkeys_ok");
        assert!(f.iter().all(|f| !f.id.starts_with("hotkey_dup:")));
    }

    #[test]
    fn canonical_combo_folds_cmd_into_ctrl_only_when_asked() {
        // The pure collapse-comparison primitive: cmd+shift+a and ctrl+shift+a
        // are distinct as typed (macOS reality) but the same X11 grab once
        // folded (Linux reality).
        assert_ne!(
            canonical_combo("cmd+shift+a", false),
            canonical_combo("ctrl+shift+a", false),
            "distinct pre-collapse"
        );
        assert_eq!(
            canonical_combo("cmd+shift+a", true),
            canonical_combo("ctrl+shift+a", true),
            "same grab post-collapse"
        );
        assert!(canonical_combo("cmd+shift+a", true).is_some());
    }

    #[test]
    fn linux_collapse_collector_reports_post_collapse_only_pairs() {
        // A macOS-distinct pair that folds to one X11 grab: the regular check
        // sees nothing, but the (pure, cfg!-free) collapse collector flags it.
        let collide = vec![
            ("Voice".to_string(), "cmd+shift+a".to_string()),
            ("Agent".to_string(), "ctrl+shift+a".to_string()),
        ];
        assert!(
            collect_hotkey_conflicts(&collide).is_empty(),
            "pre-collapse these are two different keys"
        );
        let collapse = collect_linux_collapse_conflicts(&collide);
        assert_eq!(collapse.len(), 1);
        assert_eq!(collapse[0].message_key, "doctor.hotkey_linux_collapse");
        assert_eq!(collapse[0].severity, Severity::Warn);

        // A genuine pre-collapse duplicate is the regular check's job and must
        // NOT be double-reported by the collapse collector.
        let plain_dup = vec![
            ("Voice".to_string(), "cmd+shift+a".to_string()),
            ("Agent".to_string(), "Command+Shift+A".to_string()),
        ];
        assert_eq!(collect_hotkey_conflicts(&plain_dup).len(), 1);
        assert!(
            collect_linux_collapse_conflicts(&plain_dup).is_empty(),
            "identical pre-collapse forms aren't a collapse-only clash"
        );
    }

    #[test]
    fn vocab_book_unattached_warns_with_attach_fix() {
        let mut c = cfg();
        c.vocab_books = vec![book("coding", true)];
        let f = check_vocab(&c);
        let w = find(&f, "vocab_unattached:coding").expect("unattached finding");
        assert_eq!(w.severity, Severity::Warn);
        assert_eq!(w.fix, Some(FixAction::AttachBookGlobal { book_id: "coding".into() }));
    }

    #[test]
    fn vocab_book_attached_globally_passes() {
        let mut c = cfg();
        c.vocab_books = vec![book("coding", true)];
        c.global_vocab_books = vec!["coding".into()];
        let f = check_vocab(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "vocab_ok");
    }

    #[test]
    fn vocab_attached_via_widget_is_not_unattached() {
        let mut c = cfg();
        c.vocab_books = vec![book("coding", true)];
        c.widgets = vec![llm_widget("llm.custom", json!({ "vocab_books": ["coding"] }))];
        let f = check_vocab(&c);
        assert!(find(&f, "vocab_unattached:coding").is_none());
    }

    #[test]
    fn vocab_over_budget_warns() {
        let mut c = cfg();
        let big: Vec<String> = (0..200).map(|i| format!("terminology{i:03}")).collect();
        c.vocab_books = vec![VocabBook {
            id: "big".into(), name: "Big".into(), enabled: true, terms: big, rules: vec![],
        }];
        c.global_vocab_books = vec!["big".into()];
        let f = check_vocab(&c);
        let b = find(&f, "vocab_budget").expect("budget finding");
        assert_eq!(b.severity, Severity::Warn);
    }

    #[test]
    fn dangling_sts_llm_profile_warns_with_clear_fix() {
        let mut c = cfg();
        c.model_profiles = vec![profile("real")];
        c.sts_llm_profile = "ghost".into();
        let f = check_workflow_refs(&c);
        let w = find(&f, "dangling_ref:sts_llm_profile").expect("dangling finding");
        assert_eq!(w.severity, Severity::Warn);
        assert_eq!(w.fix, Some(FixAction::ClearProfileRef { field: "sts_llm_profile".into() }));
    }

    #[test]
    fn dangling_sts_voice_profile_no_longer_warns() {
        // Workbench P2 Task 14: check_conversation_rtf (this check's last
        // live reader — see Task 11's now-superseded comment on the old
        // version of this test) was repointed at call.default's own
        // CallProps, so a dangling sts_voice_profile no longer surfaces here.
        let mut c = cfg();
        c.model_profiles = vec![profile("real")];
        c.sts_voice_profile = "ghost".into();
        let f = check_workflow_refs(&c);
        assert!(find(&f, "dangling_ref:sts_voice_profile").is_none());
    }

    #[test]
    fn llm_listen_missing_warns_when_widget_absent() {
        // Every real `effective_widgets` output always includes llm.listen
        // (a built-in the overlay can replace but never remove), so this
        // exercises the check via the injectable `_over` entry point.
        let c = cfg();
        let widgets = vec![llm_widget("llm.polish", json!({}))];
        let workflows = effective_workflows(&c);
        let f = check_workflow_refs_over(&c, &widgets, &workflows);
        let w = find(&f, "llm_listen_missing").expect("llm.listen missing finding");
        assert_eq!(w.severity, Severity::Warn);
    }

    #[test]
    fn dangling_widget_ref_in_workflow_warns() {
        let c = cfg();
        let widgets = effective_widgets(&c); // has llm.listen, so that check passes
        let workflows = vec![workflow("wf.broken", "src.selection", &["out.nonexistent"])];
        let f = check_workflow_refs_over(&c, &widgets, &workflows);
        let w = find(&f, "dangling_widget_ref:wf.broken:out.nonexistent").expect("dangling widget ref finding");
        assert_eq!(w.severity, Severity::Warn);
        assert_eq!(w.message_params, vec!["wf.broken".to_string(), "out.nonexistent".to_string()]);
    }

    #[test]
    fn widget_model_missing_warns() {
        let mut c = cfg();
        c.model_profiles = vec![profile("real")];
        c.widgets = vec![llm_widget("llm.custom", json!({ "model_profile": "ghost" }))];
        let f = check_workflow_refs(&c);
        find(&f, "widget_model_missing:llm.custom").expect("widget model finding");
    }

    #[test]
    fn widget_vocab_missing_warns() {
        let mut c = cfg();
        c.widgets = vec![llm_widget("llm.custom", json!({ "vocab_books": ["ghost-book"] }))];
        let f = check_workflow_refs(&c);
        find(&f, "widget_vocab_missing:llm.custom:ghost-book").expect("widget vocab finding");
    }

    #[test]
    fn intact_refs_pass() {
        let mut c = cfg();
        c.model_profiles = vec![profile("real")];
        c.sts_llm_profile = "real".into();
        let f = check_workflow_refs(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].id, "refs_ok");
    }

    #[test]
    fn llm_unconfigured_warns_when_prompted_widgets_stranded() {
        let mut c = cfg();
        c.llm_profile = String::new(); // built-in llm.* widgets (polish/formal/…) are always prompted
        let f = check_llm_configured(&c);
        let w = find(&f, "llm_unconfigured").expect("llm finding");
        assert_eq!(w.severity, Severity::Warn);
    }

    #[test]
    fn llm_ok_when_profile_set() {
        let mut c = cfg();
        c.llm_profile = "real".into();
        let f = check_llm_configured(&c);
        assert_eq!(f[0].id, "llm_ok");
        assert_eq!(f[0].severity, Severity::Pass);
    }

    #[test]
    fn apply_attach_book_global_is_idempotent() {
        let mut c = cfg();
        let fix = FixAction::AttachBookGlobal { book_id: "coding".into() };
        apply_config_fix(&mut c, &fix).unwrap();
        apply_config_fix(&mut c, &fix).unwrap();
        assert_eq!(c.global_vocab_books, vec!["coding".to_string()]);
    }

    #[test]
    fn apply_clear_profile_ref() {
        let mut c = cfg();
        c.sts_llm_profile = "ghost".into();
        apply_config_fix(&mut c, &FixAction::ClearProfileRef { field: "sts_llm_profile".into() }).unwrap();
        assert!(c.sts_llm_profile.is_empty());
    }

    #[test]
    fn apply_clear_profile_ref_rejects_retired_sts_voice_profile_field() {
        // Task 14 retired sts_voice_profile's dangling-ref check, so no
        // Finding constructs this anymore — but the match arm removal itself
        // is worth a regression guard: an unrecognized field name is a hard
        // error, not a silent no-op.
        let mut c = cfg();
        c.sts_voice_profile = "ghost".into();
        let err = apply_config_fix(&mut c, &FixAction::ClearProfileRef { field: "sts_voice_profile".into() })
            .unwrap_err();
        assert!(err.contains("sts_voice_profile"));
        assert_eq!(c.sts_voice_profile, "ghost", "rejected fix must not mutate config");
    }

    #[test]
    fn apply_switch_tts_model_updates_profile() {
        let mut c = cfg();
        c.model_profiles = vec![profile("tts1")];
        apply_config_fix(&mut c, &FixAction::SwitchTtsModel { profile_id: "tts1".into(), model: "kokoro-82m".into() }).unwrap();
        assert_eq!(c.model_profiles[0]["model"].as_str(), Some("kokoro-82m"));
    }

    #[test]
    fn apply_rejects_non_config_fixes() {
        let mut c = cfg();
        assert!(apply_config_fix(&mut c, &FixAction::OpenSettingsPane { pane: "microphone".into() }).is_err());
    }

    #[test]
    fn lint_config_healthy_setup_is_all_pass() {
        let mut c = cfg();
        c.pill_hotkey = "cmd+shift+space".into(); // one live binding, no conflict
        c.model_profiles = vec![profile("p")];
        c.llm_profile = "p".into();
        let findings = lint_config(&c);
        assert!(findings.iter().all(|f| f.severity == Severity::Pass), "unexpected: {findings:?}");
        // hotkeys_ok, refs_ok, llm_ok (no vocab books ⇒ no vocab finding).
        assert!(find(&findings, "hotkeys_ok").is_some());
        assert!(find(&findings, "refs_ok").is_some());
        assert!(find(&findings, "llm_ok").is_some());
    }

    #[test]
    fn fix_action_variants_round_trip_through_serde() {
        // Mirrors the frontend → Tauri deserialization path.
        let cases = vec![
            FixAction::AttachBookGlobal { book_id: "coding".into() },
            FixAction::ClearProfileRef { field: "sts_llm_profile".into() },
            FixAction::SwitchTtsModel { profile_id: "p".into(), model: "kokoro-82m".into() },
            FixAction::OpenSettingsPane { pane: "microphone".into() },
        ];
        for fix in cases {
            let v = serde_json::to_value(&fix).unwrap();
            assert!(v["kind"].is_string(), "missing tag for {fix:?}");
            let back: FixAction = serde_json::from_value(v).unwrap();
            assert_eq!(back, fix);
        }
    }

    #[test]
    fn finding_serializes_with_tagged_fix() {
        let f = Finding {
            id: "vocab_unattached:coding".into(),
            severity: Severity::Warn,
            message_key: "doctor.vocab_unattached".into(),
            message_params: vec!["Coding".into()],
            fix: Some(FixAction::AttachBookGlobal { book_id: "coding".into() }),
        };
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["severity"], "warn");
        assert_eq!(v["fix"]["kind"], "attach_book_global");
        assert_eq!(v["fix"]["book_id"], "coding");
        // Round-trips back into a FixAction for apply_doctor_fix.
        let back: FixAction = serde_json::from_value(v["fix"].clone()).unwrap();
        assert_eq!(back, FixAction::AttachBookGlobal { book_id: "coding".into() });
    }
}

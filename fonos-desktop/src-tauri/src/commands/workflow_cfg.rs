//! Settings-facing CRUD for workflow components: list / save / delete of both
//! [`WidgetDef`]s and [`WorkflowDef`]s, persisted into `AppConfig.widgets` /
//! `AppConfig.workflows`.
//!
//! Invariants enforced here (backend, not just UI):
//! * a widget's `type_tag` must be registered for its declared `role` before it
//!   can be saved (checked against the shared [`Registry`]);
//! * a composite widget's (`dialog`/`call`/`agent`/`meeting`) ref props must
//!   each be empty or point at an existing, non-composite widget of the
//!   expected capability type ([`validate_composite_refs`]);
//! * a workflow's full source → processors → outputs chain must pass
//!   [`engine::validate`] **before** anything is persisted;
//! * built-in widgets/workflows can never be deleted, and a widget still
//!   referenced by any effective workflow *or composite widget* can't be
//!   deleted either ([`engine::widget_referenced_by`] pierces into ref
//!   props, not just workflow source/processors/outputs).
//!
//! Saving or deleting a *workflow* emits `hotkey:reload` (its hotkey binding may
//! have changed); widgets carry no hotkey, so they don't. The registry lives in
//! [`AppState::registry`], built once in `main`'s `.setup()`.

use serde::Serialize;
use tauri::Emitter;

use fonos_core::workflow::builtin::{built_in_widgets, built_in_workflows};
use fonos_core::workflow::engine::{self, effective_widgets, effective_workflows};
use fonos_core::workflow::model::{is_composite, widget_ref_props, Trigger, WidgetDef, WorkflowDef};

use super::AppState;

/// Fold a legacy `hotkey` field into a `Trigger::Hotkey` chip (capture
/// `None`), clearing `hotkey` regardless. Skips adding the chip if an
/// equivalent combo is already present among `workflow.triggers`, so this is
/// safe to call unconditionally and repeatedly (matches the
/// `migrate_hotkeys_to_triggers` dedup behavior in fonos-core). Extracted
/// from [`save_workflow`] so the fold logic is testable without tauri
/// plumbing (a `tauri::State`/`tauri::AppHandle` command can't be
/// constructed in a plain unit test).
///
/// `capture: None` is deliberate here, not a placeholder: this bridge has no
/// widget-source lookup at its call site to derive a capture mode from, so
/// unlike `migrate_hotkeys_to_triggers` (which resolves the source widget and
/// carries its capture mode into the chip), this fold always leaves it unset.
pub(crate) fn fold_legacy_hotkey(workflow: &mut WorkflowDef) {
    if !workflow.hotkey.is_empty() {
        let combo = std::mem::take(&mut workflow.hotkey);
        let dup = workflow.hotkey_triggers().any(|(_, c, _)| c == combo.as_str());
        if !dup {
            workflow.triggers.push(Trigger::Hotkey { combo, capture: None });
        }
    }
}

/// The capability `type_tag` a composite's ref prop is expected to point at
/// (e.g. `stt_widget` must resolve to a `"stt"` widget). `None` for a prop
/// name not in this table — shouldn't arise, since every prop
/// [`widget_ref_props`] declares has an entry here; treated as "no type
/// constraint" rather than a panic if it ever does.
fn expected_ref_type(prop: &str) -> Option<&'static str> {
    match prop {
        "stt_widget" => Some("stt"),
        "llm_widget" => Some("llm"),
        _ => None,
    }
}

/// Composite ref-prop validation (Workbench P2 foundation for the
/// `dialog`/`call`/`agent`/`meeting` composites built in T4/T6-T9): for a
/// composite `widget`, every prop named by [`widget_ref_props`] must be
/// either empty (no override) or point at an existing, **non-composite**
/// widget whose `type_tag` matches the prop's expected capability
/// ([`expected_ref_type`]) — composites can never reference each other.
/// A non-composite `widget.type_tag` always passes: this is a no-op for
/// every widget type shipped so far. Extracted from [`save_widget`] so it's
/// unit-testable without tauri plumbing (a `tauri::State` can't be
/// constructed in a plain unit test) — same pattern as
/// [`fold_legacy_hotkey`].
pub(crate) fn validate_composite_refs(widget: &WidgetDef, widgets: &[WidgetDef]) -> Result<(), String> {
    if !is_composite(&widget.type_tag) {
        return Ok(());
    }
    for prop in widget_ref_props(&widget.type_tag) {
        let prop = *prop;
        let value = widget.props.get(prop).and_then(|v| v.as_str()).unwrap_or("");
        if value.is_empty() {
            continue;
        }
        let target = widgets
            .iter()
            .find(|w| w.id == value)
            .ok_or_else(|| format!("widget '{}': {prop} references unknown widget '{value}'", widget.id))?;
        if is_composite(&target.type_tag) {
            return Err(format!(
                "widget '{}': {prop} can't reference '{value}' — composite widgets can't reference each other",
                widget.id
            ));
        }
        if let Some(expected) = expected_ref_type(prop) {
            if target.type_tag != expected {
                return Err(format!(
                    "widget '{}': {prop} must reference a '{expected}' widget, but '{value}' is '{}'",
                    widget.id, target.type_tag
                ));
            }
        }
    }
    Ok(())
}

/// A workflow row for the settings list: the full [`WorkflowDef`] flattened,
/// plus the `type_tag` of its source widget resolved against the effective
/// widget set (`""` when the source id no longer resolves). Task 13's Dictation
/// drum filters microphone workflows on `source_type_tag` without re-resolving
/// widgets itself.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRow {
    /// The workflow definition, flattened into this row's own fields.
    #[serde(flatten)]
    pub def: WorkflowDef,
    /// `type_tag` of the workflow's source widget, or `""` if it's missing.
    pub source_type_tag: String,
}

/// Every effective widget: the built-ins overlaid by the user's config widgets.
#[tauri::command(rename_all = "snake_case")]
pub fn list_widgets(state: tauri::State<'_, AppState>) -> Vec<WidgetDef> {
    // A poisoned config lock still holds a readable config; recover it rather
    // than panic inside a non-`Result` command.
    let config = state.config.lock().unwrap_or_else(|e| e.into_inner());
    effective_widgets(&config)
}

/// Every effective workflow, each tagged with its source widget's `type_tag`.
#[tauri::command(rename_all = "snake_case")]
pub fn list_workflows(state: tauri::State<'_, AppState>) -> Vec<WorkflowRow> {
    let config = state.config.lock().unwrap_or_else(|e| e.into_inner());
    let widgets = effective_widgets(&config);
    effective_workflows(&config)
        .into_iter()
        .map(|def| {
            let source_type_tag = widgets
                .iter()
                .find(|w| w.id == def.source)
                .map(|w| w.type_tag.clone())
                .unwrap_or_default();
            WorkflowRow { def, source_type_tag }
        })
        .collect()
}

/// Persist a widget definition: validate its `type_tag` is registered for its
/// declared `role`, validate its ref props if it's a composite
/// ([`validate_composite_refs`]), then replace the same-id entry in
/// `config.widgets` (a built-in id MAY be overridden) or append it, and save.
///
/// `_app` is unused: widgets carry no hotkey, so saving one never triggers a
/// reload (unlike [`save_workflow`]).
#[tauri::command(rename_all = "snake_case")]
pub fn save_widget(
    state: tauri::State<'_, AppState>,
    _app: tauri::AppHandle,
    widget: WidgetDef,
) -> Result<(), String> {
    // `known_type_tags(role)` lists only the tags registered under that role, so
    // membership proves both "registered" and "role matches" in one check.
    if !state
        .registry
        .known_type_tags(widget.role)
        .contains(&widget.type_tag)
    {
        return Err(format!(
            "widget '{}': type_tag '{}' is not a registered {:?} widget",
            widget.id, widget.type_tag, widget.role
        ));
    }

    let mut config = state.config.lock().map_err(|e| e.to_string())?;

    // Composite ref-prop validation: a "dialog"/"call"/"agent"/"meeting"
    // widget's ref props must each be empty or point at an existing,
    // non-composite widget of the expected capability type. Checked against
    // the effective set (built-ins + config) so a ref to a built-in
    // capability widget resolves correctly.
    validate_composite_refs(&widget, &effective_widgets(&config))?;

    match config.widgets.iter_mut().find(|w| w.id == widget.id) {
        Some(slot) => *slot = widget,
        None => config.widgets.push(widget),
    }
    config.save().map_err(|e| format!("failed to save config: {e}"))
}

/// Persist a workflow definition. Validates the full source → processors →
/// outputs chain against the shared registry **first** (an invalid chain is
/// rejected with the validate error and nothing is written), then replaces the
/// same-id entry in `config.workflows` or appends it, saves, and emits
/// `hotkey:reload` so the possibly-changed hotkey binding re-registers.
#[tauri::command(rename_all = "snake_case")]
pub fn save_workflow(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    mut workflow: WorkflowDef,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;

    // Transition bridge: an older client or an imported scenario may still
    // write the legacy `hotkey` field — fold it into `triggers` so a saved
    // hotkey never silently stops registering.
    fold_legacy_hotkey(&mut workflow);

    // Validate BEFORE persisting. `effective_widgets` returns an owned Vec, so
    // the immutable borrow of `config` ends before the mutation below.
    let widgets = effective_widgets(&config);
    engine::validate(&state.registry, &workflow, &widgets)?;

    match config.workflows.iter_mut().find(|w| w.id == workflow.id) {
        Some(slot) => *slot = workflow,
        None => config.workflows.push(workflow),
    }
    config
        .save()
        .map_err(|e| format!("failed to save config: {e}"))?;
    drop(config);

    let _ = app.emit("hotkey:reload", ());
    Ok(())
}

/// Delete a custom widget by id. Refuses a built-in id, and refuses when any
/// effective workflow *or composite widget* still references the widget
/// (listing the referrers by name — see [`engine::widget_referenced_by`]).
/// Otherwise removes it from `config.widgets` and saves.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_widget(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    if built_in_widgets().iter().any(|w| w.id == id) {
        return Err(format!("widget '{id}' is built-in and cannot be deleted"));
    }

    let mut config = state.config.lock().map_err(|e| e.to_string())?;

    let referrers =
        engine::widget_referenced_by(&id, &effective_workflows(&config), &effective_widgets(&config));
    if !referrers.is_empty() {
        return Err(format!(
            "widget '{id}' is still used by: {}",
            referrers.join(", ")
        ));
    }

    let before = config.widgets.len();
    config.widgets.retain(|w| w.id != id);
    if config.widgets.len() == before {
        return Err(format!("widget '{id}' not found"));
    }
    config.save().map_err(|e| format!("failed to save config: {e}"))
}

/// Delete a custom workflow by id. Refuses a built-in id. Otherwise removes it
/// from `config.workflows`, saves, and emits `hotkey:reload` so the removed
/// workflow's hotkey unbinds.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_workflow(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    if built_in_workflows().iter().any(|w| w.id == id) {
        return Err(format!("workflow '{id}' is built-in and cannot be deleted"));
    }

    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let before = config.workflows.len();
    config.workflows.retain(|w| w.id != id);
    if config.workflows.len() == before {
        return Err(format!("workflow '{id}' not found"));
    }
    config
        .save()
        .map_err(|e| format!("failed to save config: {e}"))?;
    drop(config);

    let _ = app.emit("hotkey:reload", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::workflow::model::WidgetRole;

    fn wf(hotkey: &str, triggers: Vec<Trigger>) -> WorkflowDef {
        WorkflowDef {
            id: "wf.test".into(),
            name: "Test".into(),
            icon: String::new(),
            hotkey: hotkey.to_string(),
            triggers,
            source: "src.selection".into(),
            processors: vec![],
            outputs: vec!["out.panel".into()],
            builtin: false,
        }
    }

    /// A non-empty legacy `hotkey` becomes exactly one `Hotkey` chip with
    /// `capture: None`, and the legacy field is emptied.
    #[test]
    fn fold_legacy_hotkey_becomes_hotkey_chip_and_clears_field() {
        let mut w = wf("cmd+shift+z", vec![]);
        fold_legacy_hotkey(&mut w);

        assert!(w.hotkey.is_empty(), "legacy hotkey field cleared");
        let hks: Vec<_> = w.hotkey_triggers().collect();
        assert_eq!(hks.len(), 1, "exactly one hotkey chip");
        assert_eq!(hks[0].1, "cmd+shift+z");
        assert!(
            matches!(w.triggers[0], Trigger::Hotkey { capture: None, .. }),
            "capture is None (not carried over — no source widget context here)"
        );
    }

    /// A legacy hotkey matching an existing chip's combo does not duplicate
    /// the chip (still clears the legacy field, though).
    #[test]
    fn fold_legacy_hotkey_does_not_duplicate_existing_combo() {
        let mut w = wf(
            "cmd+shift+z",
            vec![Trigger::Hotkey { combo: "cmd+shift+z".into(), capture: None }],
        );
        fold_legacy_hotkey(&mut w);

        assert!(w.hotkey.is_empty(), "legacy hotkey field cleared");
        let matching: Vec<_> =
            w.hotkey_triggers().filter(|(_, c, _)| *c == "cmd+shift+z").collect();
        assert_eq!(matching.len(), 1, "no duplicate hotkey chip for the same combo");
    }

    // ── validate_composite_refs ─────────────────────────────────────────────

    fn widget(id: &str, type_tag: &str, props: serde_json::Value) -> WidgetDef {
        WidgetDef {
            id: id.into(),
            role: WidgetRole::Processor,
            type_tag: type_tag.into(),
            name: id.into(),
            icon: String::new(),
            props,
            builtin: false,
        }
    }

    /// A non-composite `type_tag` is always valid, regardless of props —
    /// `validate_composite_refs` is a no-op for every widget type shipped so
    /// far (dialog/call/agent/meeting don't exist as instantiable widgets yet).
    #[test]
    fn non_composite_widget_always_passes() {
        let w = widget("stt.custom", "stt", serde_json::json!({ "stt_widget": "anything" }));
        assert!(validate_composite_refs(&w, &[]).is_ok());
    }

    /// A composite's ref prop left empty (the default — "use whatever the
    /// composite falls back to") passes without needing any other widgets.
    #[test]
    fn composite_with_empty_ref_props_passes() {
        let w = widget("call.custom", "call", serde_json::json!({ "stt_widget": "", "llm_widget": "" }));
        assert!(validate_composite_refs(&w, &[]).is_ok());
    }

    /// A composite's ref prop pointing at an existing, correctly-typed,
    /// non-composite widget passes.
    #[test]
    fn composite_referencing_matching_capability_passes() {
        let stt = widget("stt.custom", "stt", serde_json::json!({}));
        let llm = widget("llm.custom", "llm", serde_json::json!({}));
        let w = widget(
            "call.custom",
            "call",
            serde_json::json!({ "stt_widget": "stt.custom", "llm_widget": "llm.custom" }),
        );
        assert!(validate_composite_refs(&w, &[stt, llm]).is_ok());
    }

    /// A ref prop naming an id that doesn't resolve to any widget is
    /// rejected, naming the missing id.
    #[test]
    fn composite_referencing_unknown_widget_is_rejected() {
        let w = widget("call.custom", "call", serde_json::json!({ "stt_widget": "stt.nope" }));
        let e = validate_composite_refs(&w, &[]).unwrap_err();
        assert!(e.contains("stt.nope"), "error should name the missing id, got: {e}");
    }

    /// A composite ref prop pointing at type-mismatched widget (an `llm`
    /// widget named as `stt_widget`) is rejected — the anti-cycle guard is
    /// really "does the target's type_tag match what this prop expects",
    /// which a wrong-but-non-composite type also fails.
    #[test]
    fn composite_referencing_wrong_capability_type_is_rejected() {
        let llm = widget("llm.custom", "llm", serde_json::json!({}));
        let w = widget("call.custom", "call", serde_json::json!({ "stt_widget": "llm.custom" }));
        let e = validate_composite_refs(&w, &[llm]).unwrap_err();
        assert!(e.contains("stt_widget"), "error should name the offending prop, got: {e}");
    }

    /// The core anti-cycle rule: a composite's ref prop can NEVER point at
    /// another composite widget, even one of a different composite type_tag.
    #[test]
    fn composite_referencing_another_composite_is_rejected() {
        let inner_call = widget("call.inner", "call", serde_json::json!({}));
        let w = widget("agent.custom", "agent", serde_json::json!({ "llm_widget": "call.inner" }));
        let e = validate_composite_refs(&w, &[inner_call]).unwrap_err();
        assert!(
            e.contains("call.inner"),
            "error should name the composite target, got: {e}"
        );
    }
}

//! Settings-facing CRUD for workflow components: list / save / delete of both
//! [`WidgetDef`]s and [`WorkflowDef`]s, persisted into `AppConfig.widgets` /
//! `AppConfig.workflows`.
//!
//! Invariants enforced here (backend, not just UI):
//! * a widget's `type_tag` must be registered for its declared `role` before it
//!   can be saved (checked against the shared [`Registry`]);
//! * a workflow's full source → processors → outputs chain must pass
//!   [`engine::validate`] **before** anything is persisted;
//! * built-in widgets/workflows can never be deleted, and a widget still
//!   referenced by any effective workflow can't be deleted either.
//!
//! Saving or deleting a *workflow* emits `hotkey:reload` (its hotkey binding may
//! have changed); widgets carry no hotkey, so they don't. The registry lives in
//! [`AppState::registry`], built once in `main`'s `.setup()`.

use serde::Serialize;
use tauri::Emitter;

use fonos_core::workflow::builtin::{built_in_widgets, built_in_workflows};
use fonos_core::workflow::engine::{self, effective_widgets, effective_workflows};
use fonos_core::workflow::model::{Trigger, WidgetDef, WorkflowDef};

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
/// declared `role`, then replace the same-id entry in `config.widgets` (a
/// built-in id MAY be overridden) or append it, and save.
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
/// effective workflow still references the widget (listing the referrers by
/// name). Otherwise removes it from `config.widgets` and saves.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_widget(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    if built_in_widgets().iter().any(|w| w.id == id) {
        return Err(format!("widget '{id}' is built-in and cannot be deleted"));
    }

    let mut config = state.config.lock().map_err(|e| e.to_string())?;

    let referrers = engine::widget_referenced_by(&id, &effective_workflows(&config));
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
}

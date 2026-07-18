//! Desktop workflow executor: the `run_workflow` entry point (in-flight
//! guarded) plus the [`DbRecorder`] that logs each run to the SQLite history.
//!
//! `run_workflow` resolves the target [`WorkflowDef`] and the effective widget
//! set from the live `AppConfig`, assembles a [`RunCtx`] (float-pill event sink,
//! empty meta, history recorder), and hands it to the platform-independent
//! [`engine::run`]. The engine owns every terminal pill event; `run_workflow`
//! only logs the raw cause of a returned error.
//!
//! Lock discipline mirrors `text_action.rs` / `adapters.rs`: the config lock is
//! read into owned values and dropped **before** `engine::run` is awaited — no
//! `std::sync` guard (config or DB) is ever held across an `.await`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::Manager;

use fonos_core::error_class::classify_error;
use fonos_core::pipeline::{EventSink, PipelineEvent};
use fonos_core::workflow::engine;
use fonos_core::workflow::model::{WidgetDef, WorkflowDef};
use fonos_core::workflow::registry::{RunCtx, RunRecorder};

use crate::adapters::PillEventSink;
use super::AppState;

/// True while a workflow run is in flight. Re-entrant triggers are dropped so
/// overlapping runs can't interleave the shared capture / panel state — the
/// same guard discipline `text_action.rs` uses for its own pipeline.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// RAII reset for [`IN_FLIGHT`]: clears the flag on scope exit (including early
/// returns and the `engine::run` await point), so a failed or empty run never
/// wedges the trigger.
///
/// `pub(crate)` so the bench commands (`commands::bench`) can share the same
/// re-entrancy flag — a bench run and a real hotkey-triggered run can't
/// overlap any more than two real runs could.
pub(crate) struct InFlightGuard;
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

impl InFlightGuard {
    /// Attempt to claim the in-flight guard: `None` if a run is already in
    /// progress. A `compare_exchange`-based alternative to `run_workflow`'s own
    /// `swap` + distinct re-entrant-trigger log message below — used by callers
    /// (the bench commands) that want a plain `Option`-shaped acquire rather
    /// than a fire-and-forget log-and-return.
    pub(crate) fn try_acquire() -> Option<InFlightGuard> {
        IN_FLIGHT
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| InFlightGuard)
    }
}

/// Execute the workflow identified by `workflow_id` end to end.
///
/// Guards against re-entrancy, resolves the workflow + effective widgets under
/// the config lock (dropped before any await), assembles the [`RunCtx`], and
/// runs the engine. Any engine `Err` has already surfaced its own terminal
/// pill event (Task 3's contract), so it is only logged here.
pub async fn run_workflow(handle: tauri::AppHandle, workflow_id: String) {
    // 1. In-flight guard — copy of the `text_action.rs` pattern. A re-entrant
    //    trigger is logged and dropped.
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        eprintln!("fonos: workflow already running — ignoring re-entrant trigger");
        // Review Fix Round 1 (Important item 3): the drop used to be silent
        // from the user's perspective (eprintln! only) — surface it on the
        // float pill, same emission path as the "workflow not found" case
        // just below.
        PillEventSink(handle.clone()).emit(PipelineEvent::Failed(classify_error(
            "Busy — previous run still in flight / 上一个任务还在进行中",
        )));
        return;
    }
    let _guard = InFlightGuard;

    // 2. Resolve everything the run needs from the live config, then drop the
    //    lock. `find` moves the matching def out of the effective set (owned),
    //    so nothing borrows the guard past this block.
    let (wf_opt, widgets, registry) = {
        let state: tauri::State<'_, AppState> = handle.state();
        // Clone the shared registry Arc out so it outlives the State borrow and
        // can be used across the `engine::run` await below.
        let registry = state.registry.clone();
        let config = match state.config.lock() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("fonos: run_workflow — config lock poisoned: {e}");
                return;
            }
        };
        let widgets = engine::effective_widgets(&config);
        let wf_opt = engine::effective_workflows(&config)
            .into_iter()
            .find(|w| w.id == workflow_id);
        (wf_opt, widgets, registry)
    };

    let Some(wf) = wf_opt else {
        // Not found → surface a pill error (engine never ran, so run_workflow
        // is the emitter here) and bail.
        let msg = format!("workflow '{workflow_id}' not found");
        eprintln!("fonos: run_workflow — {msg}");
        PillEventSink(handle.clone()).emit(PipelineEvent::Failed(classify_error(&msg)));
        return;
    };

    // 3. Assemble the per-run context: pill event sink, empty meta, and the
    //    history recorder.
    let ctx = RunCtx {
        events: Arc::new(PillEventSink(handle.clone())),
        meta: Mutex::new(serde_json::Map::new()),
        recorder: Some(Arc::new(DbRecorder {
            handle: handle.clone(),
        })),
        mock_text: None,
        dry_run: false,
    };

    // 4. Run against the shared registry, built once in `main`'s `.setup()`
    //    (Task 11) and cloned out of `AppState` above — no longer rebuilt per run.
    match engine::run(&registry, &wf, &widgets, &ctx).await {
        Err(e) => {
            // The engine already emitted the terminal Failed / NoSpeech event
            // for this error; only log the raw cause.
            eprintln!("fonos: run_workflow '{}' failed: {e}", wf.id);
        }
        Ok(outcome) => record_e2e_latency(&handle, &ctx, &wf, &widgets, &outcome),
    }
}

/// Record the end-to-end dictation latency stat (key release → text
/// delivered) for a completed run — the rows `get_dictation_latency`
/// aggregates for the Stats view.
///
/// Legacy-parity scope, enforced by early returns: mic runs only
/// (`META_CAPTURE_END_MS` is set exclusively by `MicSource`), no LLM
/// processor, and every output is the plain cursor-injection widget. The
/// output check is an ALLOWLIST on purpose: session/TTS outputs
/// (agent/dialog/speak/…) run generative work inside `deliver` — timing them
/// would record LLM thinking time as "dictation latency" (the builtin
/// `wf.agent-voice` is exactly mic→stt→agent) — and a blocklist would
/// silently start timing any future slow output. An unlisted output merely
/// skips the stat.
fn record_e2e_latency(
    handle: &tauri::AppHandle,
    ctx: &RunCtx,
    wf: &WorkflowDef,
    widgets: &[WidgetDef],
    outcome: &engine::RunOutcome,
) {
    use super::workflow_widgets::{read_meta_i64, read_meta_string, META_CAPTURE_END_MS, META_STT_MODEL};

    if outcome.final_text.is_empty() || engine::workflow_has_llm(wf, widgets) {
        return;
    }
    let inject_only = wf
        .outputs
        .iter()
        .all(|oid| widgets.iter().any(|w| &w.id == oid && w.type_tag == "insert"));
    if !inject_only {
        return;
    }
    let Some(capture_end) = read_meta_i64(ctx, META_CAPTURE_END_MS) else {
        return;
    };
    let elapsed = epoch_ms() - capture_end;
    if elapsed <= 0 {
        // Wall clock stepped backwards between capture end and delivery.
        return;
    }
    let stt_model = read_meta_string(ctx, META_STT_MODEL).unwrap_or_default();
    let state: tauri::State<'_, AppState> = handle.state();
    let db = state.db.lock();
    if let Ok(db) = &db {
        let _ = fonos_core::stats::record_dictation_latency(db, elapsed, &wf.id, &stt_model);
    }
}

/// Milliseconds since the Unix epoch — the wall-clock timestamps stashed in
/// `ctx.meta` (a serde_json map can't carry an `Instant`) for the e2e
/// dictation latency stat.
pub(crate) fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Frontend entry point: fire-and-forget a workflow run (same path as hotkeys).
///
/// Spawns [`run_workflow`] onto the async runtime so the `invoke` returns
/// immediately; run progress (and any terminal outcome) is reported entirely
/// through the `float:*` events the engine emits, so the caller never awaits
/// the run. The in-flight guard in [`run_workflow`] still drops a re-entrant
/// trigger, matching the hotkey behavior.
#[tauri::command(rename_all = "snake_case")]
pub async fn run_workflow_by_id(app: tauri::AppHandle, workflow_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn(run_workflow(app, workflow_id));
    Ok(())
}

/// Persists each completed workflow run to the SQLite history as a
/// [`fonos_core::storage::SourceType::Workflow`] entry, between processing and
/// delivery (the engine calls this via [`RunRecorder`]).
///
/// **Error policy (controller decision, from the Task 3 review).** The engine
/// treats a `RunRecorder` that returns `Err` as a hard failure of the whole run
/// (a single terminal `Failed`). History logging is *not* essential to a
/// workflow succeeding, so `DbRecorder` **absorbs its own write errors**: on a
/// poisoned DB lock or a SQLite insert failure it `eprintln!`s the cause and
/// returns `Ok(0)` — a degraded entry id. Downstream outputs already treat
/// `entry_id <= 0` gracefully (the notebook output rejects it; the speak output
/// skips the DB link), so a lost history row never breaks delivery. `record`
/// therefore **never returns `Err`**.
pub struct DbRecorder {
    /// Handle used to reach the shared history DB via `AppState`.
    pub handle: tauri::AppHandle,
}

/// Localize `wf.name` for history metadata through the builtin display map
/// (Workbench P2 Task 13). Pure — split out of [`DbRecorder::record`] so it's
/// unit-testable without a live `tauri::AppHandle`/`AppState` (this crate's
/// test harness cannot construct one — see `tests/uppercase_acceptance.rs`'s
/// doc comment). A builtin `id` resolves to its EN/ZH display name per
/// `ui_language` (`resolve_lang`'s `"auto"`/env fallback included); a custom
/// (non-builtin) `id` has no map entry and keeps `name` verbatim, so custom
/// recipes always keep their user-given names.
fn localized_workflow_name(id: &str, name: &str, ui_language: &str) -> String {
    let lang = fonos_core::workflow::builtin::resolve_lang(ui_language);
    fonos_core::workflow::builtin::builtin_display_name(id, lang)
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string())
}

impl RunRecorder for DbRecorder {
    fn record(&self, wf: &WorkflowDef, raw_text: &str, final_text: &str) -> Result<i64, String> {
        let state = self.handle.state::<AppState>();

        // Localize `workflow_name` through the same builtin map every
        // satellite panel reads at emission time (Workbench P2 Task 13), so a
        // Chinese-literal builtin name never leaks to an EN-language user's
        // History row/panel header. A poisoned config lock degrades to the
        // raw `wf.name` rather than failing the record (same error policy as
        // the DB lock below — this must never return `Err`).
        let (workflow_name, has_llm) = match state.config.lock() {
            Ok(config) => (
                localized_workflow_name(&wf.id, &wf.name, &config.ui_language),
                engine::workflow_has_llm(wf, &engine::effective_widgets(&config)),
            ),
            Err(e) => {
                eprintln!("fonos: DbRecorder — config lock poisoned, workflow_name not localized: {e}");
                (wf.name.clone(), false)
            }
        };

        let entry = fonos_core::storage::Entry {
            id: None,
            created_at: super::storage::now_iso8601(),
            source_type: fonos_core::storage::SourceType::Workflow,
            role: fonos_core::storage::EntryRole::User,
            mode: wf.id.clone(),
            raw_text: raw_text.to_string(),
            processed_text: Some(final_text.to_string()),
            container_id: None,
            audio_ref: None,
            metadata: serde_json::json!({
                "workflow_id": wf.id,
                "workflow_name": workflow_name,
            }),
        };

        // Grab the DB lock, insert, drop the lock — all synchronous (`record`
        // is a sync trait method; no await). Every failure is absorbed into
        // `Ok(0)` per the error policy above; this function must NEVER return
        // `Err`, or the engine would fail the whole run on a history-log miss.
        let db = match state.db.lock() {
            Ok(db) => db,
            Err(e) => {
                eprintln!("fonos: DbRecorder — history db lock poisoned, entry not recorded: {e}");
                return Ok(0);
            }
        };
        match fonos_core::storage::insert_entry(&db, &entry) {
            Ok(id) => {
                // Onboarding funnel (P2): the first successful LLM-processed
                // run is "first_command". Record-once semantics live in the
                // funnel table; failures are absorbed — same never-Err policy
                // as the rest of this recorder.
                if has_llm {
                    if let Ok(true) = fonos_core::funnel::record(&db, "first_command") {
                        eprintln!("fonos: funnel first_command recorded");
                    }
                }
                Ok(id)
            }
            Err(e) => {
                eprintln!("fonos: DbRecorder — history insert failed, entry not recorded: {e}");
                Ok(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `try_acquire` only ever hands out one guard at a time, and releases the
    /// flag when that guard drops — the mutual-exclusion contract the bench
    /// commands (`commands::bench`) rely on to share this flag with
    /// `run_workflow`.
    ///
    /// `IN_FLIGHT` is a single process-wide static, so this test claims and
    /// releases it deterministically (no `run_workflow`/bench call in the test
    /// suite touches it concurrently) rather than asserting on timing.
    #[test]
    fn try_acquire_is_mutually_exclusive_and_releases_on_drop() {
        // Defensive reset in case an earlier failed test left the flag set.
        IN_FLIGHT.store(false, Ordering::SeqCst);

        let first = InFlightGuard::try_acquire();
        assert!(first.is_some(), "first acquire should succeed while nothing is in flight");

        let second = InFlightGuard::try_acquire();
        assert!(second.is_none(), "second acquire must fail while the first guard is held");

        drop(first);
        let third = InFlightGuard::try_acquire();
        assert!(third.is_some(), "acquire should succeed again once the prior guard is dropped");
    }

    /// A builtin id resolves through `fonos_core::workflow::builtin`'s map,
    /// live-switching on `ui_language` — the localization
    /// `DbRecorder::record` writes into `workflow_name`.
    #[test]
    fn localized_workflow_name_resolves_builtin_by_lang() {
        assert_eq!(localized_workflow_name("wf.dictation", "听写", "en"), "Dictation");
        assert_eq!(localized_workflow_name("wf.dictation", "听写", "zh"), "听写");
        assert_eq!(localized_workflow_name("wf.explain", "选中解释", "en"), "Explain selection");
    }

    /// A custom (non-builtin) id has no map entry — the given `name` (the
    /// user's own recipe name) passes through unchanged, regardless of
    /// `ui_language`.
    #[test]
    fn localized_workflow_name_falls_back_to_given_name_for_custom_id() {
        assert_eq!(
            localized_workflow_name("wf.custom-1700000000000", "我的流程", "en"),
            "我的流程"
        );
        assert_eq!(
            localized_workflow_name("wf.custom-1700000000000", "我的流程", "zh"),
            "我的流程"
        );
    }
}

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
use fonos_core::workflow::model::WorkflowDef;
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
    if let Err(e) = engine::run(&registry, &wf, &widgets, &ctx).await {
        // The engine already emitted the terminal Failed / NoSpeech event for
        // this error; only log the raw cause.
        eprintln!("fonos: run_workflow '{}' failed: {e}", wf.id);
    }
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

impl RunRecorder for DbRecorder {
    fn record(&self, wf: &WorkflowDef, raw_text: &str, final_text: &str) -> Result<i64, String> {
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
                "workflow_name": wf.name,
            }),
        };

        // Grab the DB lock, insert, drop the lock — all synchronous (`record`
        // is a sync trait method; no await). Every failure is absorbed into
        // `Ok(0)` per the error policy above; this function must NEVER return
        // `Err`, or the engine would fail the whole run on a history-log miss.
        let state = self.handle.state::<AppState>();
        let db = match state.db.lock() {
            Ok(db) => db,
            Err(e) => {
                eprintln!("fonos: DbRecorder — history db lock poisoned, entry not recorded: {e}");
                return Ok(0);
            }
        };
        match fonos_core::storage::insert_entry(&db, &entry) {
            Ok(id) => Ok(id),
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
}

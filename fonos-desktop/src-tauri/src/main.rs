#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod adapters;
mod error_surface;
#[cfg(target_os = "macos")]
mod hotkey;
mod injection;
mod skills;
mod trigger_label;

use commands::AppState;
use fonos_core::config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};

/// Bring the main window to the front on the *currently active* Space.
///
/// Dock clicks (`RunEvent::Reopen`), the tray "Open Fonos" item and the float
/// pill all funnel through here. tao's `set_focus` relies on the deprecated
/// `activateIgnoringOtherApps:`; under macOS 14+ cooperative activation that
/// request can lose the race against the Dock-click activation already in
/// flight and hand focus back to the previously active app. The window also
/// re-opens on whichever Space it last lived on, which from another desktop
/// reads as "nothing happened" (the always-visible float pill suppresses the
/// system's own Space switch, since the app already "has visible windows").
fn raise_main_window(app: &tauri::AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };

    #[cfg(target_os = "macos")]
    {
        let win = w;
        // NSWindow calls must run on the main thread; Reopen and tray events
        // already do, the float-pill event listener may not.
        let _ = win.clone().run_on_main_thread(move || {
            use objc2::runtime::AnyObject;
            let Ok(ptr) = win.ns_window() else { return };
            if ptr.is_null() {
                return;
            }
            let ns_window = ptr as *mut AnyObject;
            let nil = std::ptr::null_mut::<AnyObject>();
            // SAFETY: `ptr` is the live NSWindow backing `win` (the captured
            // handle keeps it alive) and we are on the main thread — same
            // contract as `commands::refresh_ns_window`.
            unsafe {
                // NSWindowCollectionBehaviorMoveToActiveSpace (1 << 1): when
                // ordered in, the window joins the Space the user is on. Must
                // be set before the window is ordered front.
                let behavior: usize = objc2::msg_send![ns_window, collectionBehavior];
                let _: () =
                    objc2::msg_send![ns_window, setCollectionBehavior: behavior | (1_usize << 1)];
                // MoveToActiveSpace only applies while ordering in — a window
                // still visible on another Space must be ordered out first.
                let visible: bool = objc2::msg_send![ns_window, isVisible];
                let on_active: bool = objc2::msg_send![ns_window, isOnActiveSpace];
                if visible && !on_active {
                    let _: () = objc2::msg_send![ns_window, orderOut: nil];
                }
            }
            let _ = win.show();
            let _ = win.unminimize();
            unsafe {
                // Cooperative, non-deprecated activation — a no-op when the
                // Dock click already activated us, so it cannot bounce focus
                // back to the previously active app the way the deprecated
                // call inside tao's `set_focus` can.
                let ns_app: *mut AnyObject =
                    objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
                let responds: bool =
                    objc2::msg_send![ns_app, respondsToSelector: objc2::sel!(activate)];
                if responds {
                    let _: () = objc2::msg_send![ns_app, activate];
                } else {
                    // macOS < 14 never had cooperative activation.
                    let _: () = objc2::msg_send![ns_app, activateIgnoringOtherApps: true];
                }
                let _: () = objc2::msg_send![ns_window, makeKeyAndOrderFront: nil];
                // Raise even if the activation request is declined.
                let _: () = objc2::msg_send![ns_window, orderFrontRegardless];
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Position the note-panel window centered horizontally near the cursor,
/// slightly below the macOS menu bar — mirrors agent panel placement.
///
/// Retained for the P2 note-panel rebuild: the note-panel window is kept, but
/// the P1 `wf.note` workflow saves with no panel, so nothing positions it after
/// the legacy note dispatch arm was removed in Task 10.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn move_note_panel_to_cursor(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(panel) = app.get_webview_window("note-panel") else { return };
    let Some((target, _cursor)) = commands::monitor_under_cursor(&panel) else { return };

    let scale = target.scale_factor();
    let panel_w = 320.0_f64;

    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;

    let x = mon_x + (mon_w - panel_w) / 2.0;
    let y = mon_y + 32.0; // Just below the macOS menu bar

    let _ = panel.set_position(tauri::PhysicalPosition::new(
        (x * scale) as i32,
        (y * scale) as i32,
    ));
}

/// Stop dictation, run LLM if needed, inject text at cursor, emit float:stop.
/// Re-centers the float pill after completion.
///
/// Only invoked from the Linux global-shortcut path; macOS drives this through
/// the CGEventTap hotkey handler instead.
#[cfg(target_os = "linux")]
async fn stop_and_process_dictation(handle: tauri::AppHandle) {
    use tauri::Emitter;
    let state: tauri::State<'_, commands::AppState> = handle.state();
    let state2: tauri::State<'_, commands::AppState> = handle.state();
    let dictation_t0 = std::time::Instant::now();
    match commands::dictation::stop_recording(handle.clone(), state, None).await {
        Ok(result) => {
            if !result.text.is_empty() {
                let (mode, llm_props) = {
                    let cfg = state2.config.lock().unwrap();
                    let mode = cfg.dictation_mode.clone();
                    let widgets = fonos_core::workflow::engine::effective_widgets(&cfg);
                    let props = commands::dictation::dictation_mode_llm_props(&widgets, &mode);
                    (mode, props)
                };
                if let Some(props) = llm_props {
                    // stop_recording already left the pill in the processing
                    // state for LLM modes; just run the LLM and emit the final
                    // float:stop/float:error below.
                    {
                        // Shared post-LLM flow (fonos-core pipeline, issue #21): deliver the
                        // processed text, emit exactly one terminal pill event, classify errors.
                        let stage = commands::llm::run_dictation_llm_step(&state2, result.text, &props).await;
                        let (events, text_sink) = {
                            let s: tauri::State<AppState> = handle.state();
                            (
                                crate::adapters::PillEventSink(handle.clone()),
                                crate::adapters::InjectionTextSink(s.config.clone()),
                            )
                        };
                        if fonos_core::pipeline::deliver_llm_result(stage, &events, &text_sink).await
                            == fonos_core::pipeline::DeliveryOutcome::Delivered
                        {
                            // End-to-end dictation latency (key release → delivered), issue #4.
                            let db_arc = {
                                let s: tauri::State<AppState> = handle.state();
                                s.db.clone()
                            };
                            let guard = db_arc.lock();
                            if let Ok(db) = guard {
                                let _ = fonos_core::stats::record_dictation_latency(
                                    &db, dictation_t0.elapsed().as_millis() as i64, &mode, &result.stt_model,
                                );
                            }
                        }
                    }
                }
                // Raw mode — stop_recording already injected and emitted
                // float:stop (success) or float:error (injection failure).
                // Re-emitting float:stop here would repaint the pill green
                // over a just-shown injection error.
            } else {
                let _ = handle.emit("float:stop", "");
            }
        }
        Err(e) => {
            if e.contains("not recording") {
                // Harmless start/stop race — keep the silent idle revert.
                let _ = handle.emit("float:stop", "");
            } else {
                crate::error_surface::emit_float_error(&handle, &e);
            }
        }
    }
}

/// Build all hotkey configs from the current app config.
#[cfg(target_os = "macos")]
fn build_hotkey_configs(config: &AppConfig) -> Vec<hotkey::HotkeyConfig> {
    let mut configs = Vec::new();
    let mut try_add = |combo: &str, label: &str| {
        if combo.is_empty() { return; }
        match hotkey::HotkeyManager::parse_hotkey(combo, label) {
            Ok(hk) => configs.push(hk),
            Err(e) => eprintln!("fonos: could not parse {} hotkey '{}': {}", label, combo, e),
        }
    };
    // Non-workflow triggers keep their dedicated labels + dispatch arms.
    // (Agent's former standalone "agent"/"agent-panel" labels are gone —
    // Workbench P2 Task 6 folded them into wf.agent-voice/wf.agent's own
    // Hotkey chips, handled by the `starts_with("workflow-")` arm below.
    // Meeting's former standalone "meeting" label is gone the same way —
    // Workbench P2 Task 7 folded hotkey_meeting into wf.meeting's own Hotkey
    // chip; the STS walkie's "sts" label likewise — Task 9 folded hotkey_sts
    // into wf.call's own Hotkey chip, retiring the hold-to-talk arm.)
    // Dictation / note / listen / text-actions are unified onto the workflow
    // engine (Workflow P1): every Hotkey chip on a workflow registers its own
    // `workflow-{id}@{trigger_idx}` label (Workbench P1), handled by the
    // `starts_with("workflow-")` arm.
    for wf in fonos_core::workflow::engine::effective_workflows(config) {
        for (idx, combo, _capture) in wf.hotkey_triggers() {
            try_add(combo, &crate::trigger_label::hotkey_label(&wf.id, idx));
        }
    }
    // Pill-owned hotkey (Workbench P1, spec §3c): the floating pill holds its
    // own global key, separate from any recipe's Hotkey chips, dispatched by
    // the `"pill"` arm below.
    try_add(&config.pill_hotkey, "pill");
    configs
}

/// Debounces a fast physical double-press of a toggle-capture hotkey so it
/// doesn't re-trigger the same mic workflow twice in quick succession.
/// Key-repeat is already suppressed by the hotkey layer, so this only guards
/// against an actual double tap. Shared by every mic-sourced trigger — the
/// `workflow-{id}` and `pill` hotkey arms alike (see
/// [`dispatch_workflow_trigger`]) — since the guard is about a fast physical
/// gesture, not about which specific hotkey fired.
#[cfg(target_os = "macos")]
static TOGGLE_DEBOUNCE_LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
#[cfg(target_os = "macos")]
const TOGGLE_DELAY_MS: u64 = 500;

/// The shared mic hold/toggle dispatch dance for every workflow-triggering
/// hotkey arm (`workflow-{id}` and `pill` alike): given the resolved target
/// workflow id, whether its source is a microphone widget, this trigger's
/// capture mode ("hold" or "toggle"), and the key event, either runs the
/// workflow — mic sources on the correct hold/toggle edge (toggle key-downs
/// debounced via [`TOGGLE_DEBOUNCE_LAST`]), non-mic sources once on
/// key-down — or finishes an in-flight capture.
#[cfg(target_os = "macos")]
async fn dispatch_workflow_trigger(
    handle: tauri::AppHandle,
    wf_id: String,
    is_mic: bool,
    capture: &str,
    is_down: bool,
) {
    if is_mic {
        if capture == "toggle" && is_down {
            {
                let mut last = TOGGLE_DEBOUNCE_LAST.lock().unwrap();
                let debounced = last
                    .map(|t| t.elapsed().as_millis() < TOGGLE_DELAY_MS as u128)
                    .unwrap_or(false);
                if debounced {
                    eprintln!("fonos: workflow toggle debounce — too soon since last action");
                    return;
                }
                *last = Some(std::time::Instant::now());
            }
        }
        // MicSource starts on key-down and blocks (no timeout) until
        // finish_active_capture fires. is_recording() here + run_workflow's
        // own IN_FLIGHT guard together prevent a second capture starting
        // while one is live, so the single global CAPTURE_DONE signal always
        // targets the run the trigger owns.
        match (capture, is_down, commands::dictation::is_recording()) {
            // hold: key-down starts, key-up finishes.
            ("hold", true, false) => {
                commands::workflow_exec::run_workflow(handle.clone(), wf_id).await;
            }
            // Finish is unconditional on key-up — NOT gated on
            // is_recording() — because a fast tap can release the key before
            // start_recording flips IS_RECORDING (set only after mic warm-up
            // completes). If finish were gated here, that key-up would no-op
            // and MicSource::acquire would be left awaiting CAPTURE_DONE
            // forever (no timeout), hanging the mic live until the next
            // gesture. This is safe because MicSource::acquire registers its
            // CAPTURE_DONE waiter before calling start_recording, so an
            // "early" finish still latches via notify_waiters(); and when
            // nothing is capturing, finishing is a harmless no-op
            // (notify_waiters with no registered waiter is simply dropped).
            ("hold", false, _) => {
                commands::workflow_widgets::finish_active_capture();
            }
            // toggle: 1st press starts, 2nd press finishes.
            ("toggle", true, false) => {
                commands::workflow_exec::run_workflow(handle.clone(), wf_id).await;
            }
            ("toggle", true, true) => {
                commands::workflow_widgets::finish_active_capture();
            }
            _ => {}
        }
    } else if is_down {
        // Non-mic sources (e.g. selection) fire once on key-down.
        commands::workflow_exec::run_workflow(handle.clone(), wf_id).await;
    }
}

fn main() {
    let mut config = AppConfig::load();
    // One-time migration: legacy quick-transform hotkey → text_actions binding.
    if fonos_core::config::migrate_transform_to_text_actions(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated quick-transform hotkey to text_actions"),
            Err(e) => eprintln!("fonos: config migration save failed: {e}"),
        }
    }
    // One-time migration: legacy dictation / note / listen / text-action config
    // → workflow engine (Workflow P1). Runs after the transform migration above
    // so the text_actions it produces are folded into `wf.ta-*` workflows here,
    // and before build_hotkey_configs reads the (now workflow-shaped) config.
    if fonos_core::workflow::migrate::migrate_to_workflows_from_disk(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated dictation/note/listen/text-actions to workflows"),
            Err(e) => eprintln!("fonos: workflow migration save failed: {e}"),
        }
    }
    // One-time migration: formerly-global settings (STT language, insert
    // strategy, translate target) → widget props (Workflow P2). Runs after
    // migrate_to_workflows so it operates on the now-workflow-shaped config.
    if fonos_core::workflow::migrate::migrate_settings_into_flow(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated stt-language/insert-strategy/translate-target into widget props"),
            Err(e) => eprintln!("fonos: settings migration save failed: {e}"),
        }
    }
    // Idempotent (sentinel-free) remap of built-in ids renamed after they
    // shipped — currently out.dialog-explain → out.dialog — so configs from
    // earlier P2 builds keep resolving. A no-op once no stale ids remain.
    if fonos_core::workflow::migrate::remap_renamed_builtins(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: remapped renamed built-in ids (out.dialog-explain → out.dialog)"),
            Err(e) => eprintln!("fonos: builtin remap save failed: {e}"),
        }
    }
    // One-time migration: legacy per-workflow `hotkey` strings → `triggers`
    // (Workbench P1). Runs after the migrations above so it sees the
    // fully-migrated workflow shape, and before the pill-hotkey migration
    // below (which consumes the Hotkey chip this one creates on wf.dictation).
    if fonos_core::workflow::migrate::migrate_hotkeys_to_triggers(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated workflow hotkeys into triggers"),
            Err(e) => eprintln!("fonos: hotkey-to-triggers migration save failed: {e}"),
        }
    }
    // One-time migration: wf.dictation's primary Hotkey chip → the pill's own
    // `pill_hotkey`/`pill_hotkey_capture` fields (Workbench P1, spec §3c).
    // Runs LAST, after `migrate_hotkeys_to_triggers`, since it consumes the
    // chip that migration creates.
    if fonos_core::workflow::migrate::migrate_primary_hotkey_to_pill(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated primary dictation hotkey to the pill"),
            Err(e) => eprintln!("fonos: pill-hotkey migration save failed: {e}"),
        }
    }
    // One-time migration: the legacy standalone Agent hotkeys
    // (hotkey_agent/hotkey_agent_panel) → Trigger::Hotkey chips on the new
    // agent composite recipes (Workbench P2 Task 6). Runs after the pill
    // migration above (order doesn't matter functionally — see the function
    // doc — but keeps "newest migration last").
    if fonos_core::workflow::migrate::migrate_legacy_agent_triggers(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated legacy agent hotkeys to recipe triggers"),
            Err(e) => eprintln!("fonos: agent-triggers migration save failed: {e}"),
        }
    }
    // One-time migration: the legacy standalone `hotkey_meeting` toggle →
    // a Trigger::Hotkey chip on the new `wf.meeting` composite recipe, plus a
    // non-empty legacy `meeting_summary_prompt` → the `meeting.default`
    // widget's `props.summary_prompt` (Workbench P2 Task 7). Runs after the
    // agent-triggers migration above (order doesn't matter functionally —
    // see the function doc — but keeps "newest migration last").
    if fonos_core::workflow::migrate::migrate_legacy_meeting_triggers(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated legacy meeting hotkey to recipe trigger"),
            Err(e) => eprintln!("fonos: meeting-triggers migration save failed: {e}"),
        }
    }
    // One-time migration: the legacy STS walkie config → the `wf.call`
    // composite recipe (hotkey_sts → Hotkey chip, sts_persona → minted
    // llm.call-persona widget, sts_voice*/sts_max_turns/call_* →
    // call.default props; Workbench P2 Task 9). Runs after the
    // meeting-triggers migration above ("newest migration runs last").
    if fonos_core::workflow::migrate::migrate_legacy_call_triggers(&mut config) {
        match config.save() {
            Ok(()) => eprintln!("fonos: migrated legacy STS/call config to the call recipe"),
            Err(e) => eprintln!("fonos: call-triggers migration save failed: {e}"),
        }
    }
    let config = config;

    // Initialize SQLite database for stats & history
    let db_path = fonos_core::stats::db_path();
    let _ = std::fs::create_dir_all(db_path.parent().unwrap());
    let db_conn = rusqlite::Connection::open(&db_path)
        .expect("failed to open fonos.db");
    fonos_core::stats::init_db(&db_conn);

    // Initialize v2 storage tables (entries, containers, FTS5) — idempotent.
    fonos_core::storage::init_storage_db(&db_conn);
    // Migrate legacy events table to v2 entries/containers schema (idempotent).
    if let Err(e) = fonos_core::storage::migrate_from_history(&db_conn) {
        eprintln!("fonos: storage migration warning: {e}");
    }
    // Ensure "Quick Note" notebook exists (the default notebook for notes without a specific target).
    {
        let has_quick: bool = db_conn.query_row(
            "SELECT COUNT(*) FROM containers WHERE container_type='notebook' AND title='Quick Note'",
            [], |r| r.get::<_, i64>(0)
        ).unwrap_or(0) > 0;
        if !has_quick {
            let now = commands::storage::now_iso8601();
            let _ = db_conn.execute(
                "INSERT INTO containers (container_type, title, created_at, updated_at, metadata) VALUES ('notebook', 'Quick Note', ?1, ?1, '{}')",
                rusqlite::params![now],
            );
            eprintln!("fonos: created default 'Quick Note' notebook");
        }
    }

    // ── Agent state initialization ─────────────────────────────────────────
    let agent_state = {
        use fonos_core::agent::registry::SkillRegistry;
        use fonos_core::agent::context::ConversationContext;
        use fonos_core::agent::fast_path::FastPathMatcher;
        use fonos_core::agent::safety::{CommandSafetyConfig, CommandSafetyFilter};
        use fonos_core::agent::custom_loader::load_custom_skills_with_safety;
        use commands::agent::AgentState;

        // Build the safety filter from config (merge defaults with user customizations).
        let mut safety_config = CommandSafetyConfig::empty();
        safety_config.allowlist.extend(config.agent_safety_allowlist.clone());
        safety_config.blocklist.extend(config.agent_safety_blocklist.clone());
        let safety = Arc::new(CommandSafetyFilter::new_with_defaults(safety_config));

        // Create skill registry and register built-in desktop skills.
        let mut registry = SkillRegistry::new();
        skills::register_desktop_skills(&mut registry, Arc::clone(&safety));

        // Collect built-in skill names before loading custom skills.
        let builtin_skill_names: Vec<String> = registry
            .list()
            .iter()
            .map(|si| si.name.clone())
            .collect();

        // Load custom skills from the app data directory. The safety filter is
        // attached so custom `shell` skills are vetted just like the built-in one.
        let skills_dir = AppConfig::config_dir().join("skills");
        if skills_dir.exists() {
            let custom_skills =
                load_custom_skills_with_safety(&skills_dir, Some(Arc::clone(&safety)));
            for skill in custom_skills {
                registry.register(skill);
            }
        }

        let context = ConversationContext::new(config.agent_max_turns);
        let fast_path = FastPathMatcher::new();
        let timeout_secs = config.agent_timeout_secs;

        // No `config.agent_system_prompt` read here anymore (final review
        // wave, I1): that startup-cached copy used to back `AgentState`'s now
        // -removed `system_prompt` field, which `agent_process` alone
        // consulted — it now resolves the persona fresh on every call via
        // `commands::agent_widget::resolve_agent_default_persona`, the same
        // way the voice/widget path already did. See `AgentState`'s doc
        // comment and `config.agent_system_prompt`'s.
        AgentState::new(
            registry,
            context,
            fast_path,
            timeout_secs,
            builtin_skill_names,
            Arc::clone(&safety),
        )
    };

    let meeting_state = commands::meeting::MeetingState::new();

    // `AppState` construction is deferred into `.setup()` below: its `registry`
    // field is built by `build_registry(handle)`, and the `AppHandle` is only
    // available inside setup. The owned pieces built above (config, db_conn,
    // agent_state, meeting_state) are moved into the setup closure, which
    // assembles AppState and `manage`s it before anything else in setup reads it.

    // `mut` is only exercised on Linux (the global-shortcut plugin block below);
    // on macOS the binding is never reassigned.
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // In-app auto-update (macOS + Linux AppImage): the updater plugin checks
    // the GitHub Releases `latest.json` endpoint and installs signed
    // artifacts; the process plugin exposes relaunch() so the UI can restart
    // into the new version. On Linux this only works for AppImage installs —
    // see `commands::update::update_supports_self_install`, which the
    // frontend uses to hide the in-place install button for deb/rpm.
    builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    // Register global-shortcut plugin on Linux
    #[cfg(target_os = "linux")]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            // Config commands
            commands::config::get_config,
            commands::config::save_config,
            // Dictation commands
            commands::dictation::has_microphone,
            commands::dictation::list_audio_inputs,
            commands::dictation::start_recording,
            commands::dictation::stop_recording,
            commands::dictation::test_stt,
            commands::dictation::transcribe_file,

            commands::doctor::run_doctor,
            commands::doctor::apply_doctor_fix,
            // Scenario setup commands (issue #29)
            commands::scenarios::scan_models,
            commands::scenarios::scenario_probe,
            commands::scenarios::save_scenario,
            commands::scenarios::apply_saved_scenario,
            commands::scenarios::delete_saved_scenario,
            commands::scenarios::export_scenario,
            commands::scenarios::import_scenario,
            commands::scenarios::import_scenario_json,
            // The STS walkie/Talk-page commands (sts_page_start/stop,
            // get_sts_history, reset_sts_session) and call_start are gone
            // (Workbench P2 Task 9): calls start via the `call` composite
            // widget; call-panel.html only invokes call_stop/hide_call_panel.
            commands::call::call_stop,
            commands::call::hide_call_panel,
            commands::tts::list_model_voices,
            commands::permissions::check_accessibility,
            commands::permissions::open_settings_pane,
            commands::update::update_supports_self_install,
            commands::update::open_releases_page,
            // TTS commands
            commands::tts::synthesize_speech,
            commands::tts::generate_and_play,
            commands::tts::play_audio_file,
            commands::tts::play_speech,
            commands::tts::stop_playback,
            commands::tts::pause_playback,
            commands::tts::resume_playback,
            // Voice commands
            commands::voices::list_voices,
            commands::voices::clone_voice,
            commands::voices::delete_voice,
            commands::voices::preview_voice,
            commands::voices::pick_audio_file,
            commands::voices::record_voice_sample,
            // Window commands
            commands::resize_float,
            commands::refresh_float_window,
            commands::resize_agent_panel,
            commands::hide_agent_panel,
            commands::hide_note_panel,
            commands::text_action::hide_text_action_panel,
            commands::text_action::text_action_copy,
            commands::text_action::text_action_insert,
            commands::text_action::text_action_save_notebook,
            commands::dialog::dialog_send,
            commands::dialog::hide_dialog_panel,
            commands::dialog::dialog_save_notebook,
            commands::set_note_notebook,
            // LLM commands
            commands::llm::probe_model,
            commands::llm::list_provider_models,
            // Stats & History commands
            commands::stats::record_event,
            commands::stats::delete_event,
            commands::stats::get_stats,
            commands::stats::get_history,
            commands::stats::get_today,
            commands::stats::get_dictation_latency,
            // Agent commands
            commands::agent::agent_process,
            commands::agent::agent_reset,
            commands::agent::list_skills,
            commands::agent::toggle_skill,
            commands::agent::save_custom_skill,
            commands::agent::get_custom_skill,
            commands::agent::delete_custom_skill,
            commands::agent::test_skill,
            // Selection commands
            commands::selection::grab_selection,
            commands::selection::replace_selection,
            // Storage commands (v2)
            commands::storage::list_entries,
            commands::storage::get_entry,
            commands::storage::update_entry,
            commands::storage::update_entry_text,
            commands::storage::delete_entry,
            commands::storage::search_entries,
            commands::storage::create_container,
            commands::storage::list_containers,
            commands::storage::get_container_entries,
            commands::storage::delete_container,
            commands::storage::update_container_metadata,
            commands::storage::export_notebook_md,
            commands::storage::export_notebook_json,
            // Meeting commands
            commands::meeting::start_meeting,
            commands::meeting::stop_meeting,
            commands::meeting::get_meetings,
            commands::meeting::get_meeting_detail,
            commands::meeting::hide_meeting_panel,
            commands::meeting::export_meeting_md,
            commands::meeting::export_meeting_json,
            commands::diarize::diarize_check,
            commands::diarize::diarize_download_models,
            // Workflow / widget CRUD commands (settings pages)
            commands::workflow_cfg::list_widgets,
            commands::workflow_cfg::list_workflows,
            commands::workflow_cfg::save_widget,
            commands::workflow_cfg::save_workflow,
            commands::workflow_cfg::delete_widget,
            commands::workflow_cfg::delete_workflow,
            // Workflow execution (frontend entry points to the engine)
            commands::workflow_exec::run_workflow_by_id,
            commands::workflow_widgets::finish_capture,
            // Test Run bench (settings-only; step-traced, intercepted runs)
            commands::bench::bench_run_workflow,
            commands::bench::bench_run_widget,
        ])
        .setup(move |app| {
            // Assemble and manage the shared AppState first — the rest of setup
            // (and every command) reaches it via `app.state::<AppState>()`. The
            // workflow registry is built exactly once here, from this handle, and
            // shared by `run_workflow` and the settings CRUD commands (rather than
            // rebuilt per run). Managing here (vs. on the builder) is equivalent:
            // no command runs before setup completes.
            let app_state = AppState {
                audio_capture: Arc::new(Mutex::new(None)),
                audio_playback: Arc::new(Mutex::new(None)),
                config: Arc::new(Mutex::new(config)),
                db: Arc::new(Mutex::new(db_conn)),
                agent: Arc::new(tokio::sync::Mutex::new(agent_state)),
                meeting: Arc::new(tokio::sync::Mutex::new(meeting_state)),
                note_target: Arc::new(Mutex::new(None)),
                sts_session: Arc::new(tokio::sync::Mutex::new(fonos_core::sts::StsSession::default())),
                dialog_session: Arc::new(tokio::sync::Mutex::new(None)),
                call_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                registry: Arc::new(commands::workflow_widgets::build_registry(app.handle().clone())),
            };
            app.manage(app_state);

            // 0. Make agent-panel window fully transparent:
            //    - Clear webview background so only #drop div is visible
            //    - Disable window shadow so macOS doesn't draw a rectangular outline
            //      around the transparent window (which causes the "two-layer" effect)
            {
                use tauri::Manager;
                use tauri::webview::Color;
                if let Some(panel) = app.get_webview_window("agent-panel") {
                    let _ = panel.set_background_color(Some(Color(0, 0, 0, 0)));
                    let _ = panel.set_shadow(false);
                }
                if let Some(panel) = app.get_webview_window("note-panel") {
                    let _ = panel.set_background_color(Some(Color(0, 0, 0, 0)));
                    let _ = panel.set_shadow(false);
                }
                if let Some(panel) = app.get_webview_window("meeting-panel") {
                    let _ = panel.set_background_color(Some(Color(0, 0, 0, 0)));
                    let _ = panel.set_shadow(false);
                }
                // Main window starts hidden — user opens it via tray icon or dock click.
                // First run: show + focus the main window immediately so the
                // onboarding wizard is visible (mirrors the tray "Open Fonos" flow).
                let first_run = {
                    let state = app.state::<AppState>();
                    let config = state.config.lock().unwrap();
                    !config.has_completed_onboarding
                };
                if first_run {
                    raise_main_window(app.handle());
                }
            }

            // 0. SIGUSR2 handler — toggle dictation from external scripts / window managers.
            #[cfg(unix)]
            {
                use tauri::Emitter;
                let sig_handle = app.handle().clone();
                std::thread::spawn(move || {
                    use signal_hook::iterator::Signals;
                    let mut signals = Signals::new(&[signal_hook::consts::SIGUSR2])
                        .expect("failed to register SIGUSR2 handler");
                    for _ in signals.forever() {
                        eprintln!("fonos: SIGUSR2 received — toggling dictation");
                        let _ = sig_handle.emit("signal:toggle-dictation", ());
                    }
                });
            }

            // 1. Global hotkeys (macOS uses CGEventTap; Linux TODO: use global-shortcut plugin).
            #[cfg(target_os = "macos")]
            {
            let state = app.state::<AppState>();
            // Snapshot the config so registration goes through the same
            // build_hotkey_configs() path the hotkey:reload listener uses.
            let cfg = state.config.lock().unwrap().clone();

            let mut hm = hotkey::HotkeyManager::new();
            let mut any_hotkey = false;
            for hk in build_hotkey_configs(&cfg) {
                hm.register(hk);
                any_hotkey = true;
            }

            if any_hotkey {
                let app_handle = app.handle().clone();
                // Toggle debounce state lives in the shared `TOGGLE_DEBOUNCE_LAST`
                // static (see above) so it's usable from the free-standing
                // `dispatch_workflow_trigger` helper, not just this closure.
                hm.set_callback(move |label, is_down| {
                    let handle = app_handle.clone();
                    let label = label.to_string();
                    tauri::async_runtime::spawn(async move {
                        match label.as_str() {
                            // Meeting's former standalone "meeting" toggle arm is
                            // gone (Workbench P2 Task 7) — folded into
                            // `wf.meeting`'s own Hotkey chip, handled by the
                            // `starts_with("workflow-")` arm below;
                            // `commands::meeting_widget::MeetingOutput` reads the
                            // live recording state itself to decide start vs stop,
                            // the same toggle dance this arm used to do inline.
                            // The STS walkie's former "sts" hold-to-talk arm is gone
                            // the same way (Workbench P2 Task 9) — hotkey_sts is now
                            // a Hotkey chip on `wf.call`, whose
                            // `commands::call_widget::CallOutput` toggles a
                            // hands-free call on each key-down.

                            l if l.starts_with("workflow-") => {
                                // Resolve the trigger target and derive
                                // `is_mic`/`capture` under the config lock,
                                // then drop it before any await. The label
                                // carries the fired trigger chip as
                                // `workflow-{id}@{trigger_idx}`
                                // (Workbench P1 — one binding per Hotkey
                                // chip). Every `workflow-{id}` label triggers
                                // its own id directly (Workbench P1, spec
                                // §3c: the former `workflow-wf.dictation` →
                                // `active_voice_workflow` redirect is gone —
                                // that behavior now belongs to the pill's own
                                // hotkey, see the `"pill"` arm below).
                                // `is_mic` gates the two-phase mic dance and
                                // comes from the target workflow's source
                                // widget; `capture` ("hold"|"toggle") comes
                                // from the fired workflow's
                                // `triggers[trigger_idx]`. A missing/dangling
                                // workflow is logged and the trigger dropped.
                                let resolved = {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    let config = match state.config.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("fonos: workflow hotkey — config lock poisoned: {e}");
                                            return;
                                        }
                                    };
                                    let (base_label, trigger_idx) =
                                        crate::trigger_label::parse_hotkey_label(l);
                                    let wf_id =
                                        fonos_core::workflow::engine::resolve_trigger_target(base_label);
                                    let widgets =
                                        fonos_core::workflow::engine::effective_widgets(&config);
                                    let workflows =
                                        fonos_core::workflow::engine::effective_workflows(&config);
                                    workflows
                                        .iter()
                                        .find(|w| w.id == wf_id)
                                        .map(|wf| {
                                            let src = widgets.iter().find(|w| w.id == wf.source);
                                            let is_mic = src
                                                .map(|w| w.type_tag == "microphone")
                                                .unwrap_or(false);
                                            let capture = wf
                                                .hotkey_triggers()
                                                .find(|(i, _, _)| *i == trigger_idx)
                                                .map(|(_, _, cap)| cap.to_string())
                                                .unwrap_or_else(|| "hold".to_string());
                                            (wf.id.clone(), is_mic, capture)
                                        })
                                };
                                let Some((wf_id, is_mic, capture)) = resolved else {
                                    eprintln!(
                                        "fonos: workflow trigger '{l}' resolved to no definition — ignoring hotkey"
                                    );
                                    return;
                                };
                                dispatch_workflow_trigger(handle.clone(), wf_id, is_mic, &capture, is_down)
                                    .await;
                            }

                            "pill" => {
                                // Pill-owned hotkey (Workbench P1, spec §3c):
                                // the floating pill holds its own global key,
                                // separate from any recipe's Hotkey chips.
                                // Pressing it runs whichever workflow the
                                // pill roller currently has selected
                                // (`active_voice_workflow`), falling back to
                                // the built-in wf.dictation — this is the
                                // exact "run whatever's selected" behavior
                                // the old `workflow-wf.dictation` redirect
                                // used to provide, now read directly rather
                                // than through `resolve_trigger_target`.
                                // `is_mic` is derived the same way as the
                                // `workflow-` arm above; `capture` comes from
                                // `config.pill_hotkey_capture` (not a
                                // per-workflow trigger chip, since the pill's
                                // key isn't one). Shares the mic dance with
                                // the `workflow-` arm via
                                // `dispatch_workflow_trigger`.
                                let resolved = {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    let config = match state.config.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("fonos: pill hotkey — config lock poisoned: {e}");
                                            return;
                                        }
                                    };
                                    let workflows =
                                        fonos_core::workflow::engine::effective_workflows(&config);
                                    let active = &config.active_voice_workflow;
                                    let wf_id = if !active.is_empty()
                                        && workflows.iter().any(|w| w.id == *active)
                                    {
                                        active.clone()
                                    } else {
                                        "wf.dictation".to_string()
                                    };
                                    let widgets =
                                        fonos_core::workflow::engine::effective_widgets(&config);
                                    let capture = if config.pill_hotkey_capture.is_empty() {
                                        "hold".to_string()
                                    } else {
                                        config.pill_hotkey_capture.clone()
                                    };
                                    workflows.iter().find(|w| w.id == wf_id).map(|wf| {
                                        let src = widgets.iter().find(|w| w.id == wf.source);
                                        let is_mic = src
                                            .map(|w| w.type_tag == "microphone")
                                            .unwrap_or(false);
                                        (wf.id.clone(), is_mic, capture)
                                    })
                                };
                                let Some((wf_id, is_mic, capture)) = resolved else {
                                    eprintln!(
                                        "fonos: pill hotkey resolved to no definition — ignoring"
                                    );
                                    return;
                                };
                                dispatch_workflow_trigger(handle.clone(), wf_id, is_mic, &capture, is_down)
                                    .await;
                            }

                            _ => {}
                        }
                    });
                });

                // Get a handle to the hotkeys for live reload
                let hotkeys_arc = hm.hotkeys_ref();

                if let Err(e) = hm.start() {
                    eprintln!("fonos: hotkey registration failed: {}", e);
                }

                // The CGEventTap that backs global hotkeys is installed on a
                // background thread and silently no-ops without the Accessibility
                // permission, so hm.start() can't report that failure directly.
                // Probe AXIsProcessTrusted() as a proxy and surface a clickable
                // error when it's missing. A short delay lets the float pill's
                // event listener come up before we emit.
                if !crate::injection::accessibility_trusted() {
                    let acc_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        crate::error_surface::emit_float_error(
                            &acc_handle,
                            "Accessibility permission not granted — global hotkeys won't work. \
                             Enable Fonos in System Settings > Privacy & Security > Accessibility.",
                        );
                    });
                }

                // Listen for hotkey config changes and reload bindings
                let reload_handle = app.handle().clone();
                let reload_hotkeys = hotkeys_arc;
                app.listen("hotkey:reload", move |_| {
                    let state: tauri::State<'_, AppState> = reload_handle.state();
                    let config = state.config.lock().unwrap();
                    let new_configs = build_hotkey_configs(&config);
                    let mut guard = reload_hotkeys.lock().unwrap();
                    guard.clear();
                    guard.extend(new_configs);
                    eprintln!("fonos: hotkeys hot-reloaded ({} bindings)", guard.len());
                });
            }

            } // end #[cfg(target_os = "macos")] hotkey block

            // 1b. Linux global shortcuts via tauri-plugin-global-shortcut.
            #[cfg(target_os = "linux")]
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

                let state = app.state::<AppState>();
                let config = state.config.lock().unwrap();
                let combos: Vec<(String, String)> = vec![
                    (config.hotkey_dictation.clone(), "dictation".into()),
                    (config.hotkey_dictation_toggle.clone(), "dictation-toggle".into()),
                ];
                drop(config);

                // Convert fonos hotkey format (cmd+shift+a) to Tauri shortcut format (CommandOrControl+Shift+A)
                fn to_tauri_shortcut(combo: &str) -> Option<String> {
                    if combo.is_empty() { return None; }
                    let parts: Vec<&str> = combo.split('+').collect();
                    let converted: Vec<String> = parts.iter().map(|p| {
                        match p.to_lowercase().as_str() {
                            "cmd" | "command" => "CommandOrControl".into(),
                            "ctrl" | "control" => "Control".into(),
                            "shift" => "Shift".into(),
                            "alt" | "opt" | "option" => "Alt".into(),
                            "space" => "Space".into(),
                            other => {
                                let mut c = other.chars();
                                match c.next() {
                                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                    None => other.into(),
                                }
                            }
                        }
                    }).collect();
                    Some(converted.join("+"))
                }

                let app_handle = app.handle().clone();
                for (combo, label) in combos {
                    if let Some(tauri_combo) = to_tauri_shortcut(&combo) {
                        match tauri_combo.parse::<Shortcut>() {
                            Ok(shortcut) => {
                                let lbl = label.clone();
                                let h = app_handle.clone();
                                if let Err(e) = app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                                    let handle = h.clone();
                                    let label = lbl.clone();
                                    let is_toggle = label == "dictation-toggle";
                                    let pressed = event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed;

                                    if is_toggle {
                                        // Toggle: only react to press
                                        if !pressed { return; }
                                        tauri::async_runtime::spawn(async move {
                                            eprintln!("fonos: linux toggle '{}'", label);
                                            if crate::commands::dictation::is_recording() {
                                                stop_and_process_dictation(handle).await;
                                            } else {
                                                let state: tauri::State<'_, AppState> = handle.state();
                                                if let Err(e) = commands::dictation::start_recording(handle.clone(), state, None).await {
                                                    crate::error_surface::emit_float_error(&handle, &e);
                                                }
                                            }
                                        });
                                    } else {
                                        // Hold-to-talk: press=start, release=stop
                                        tauri::async_runtime::spawn(async move {
                                            if pressed {
                                                eprintln!("fonos: linux hold '{}' down", label);
                                                if !crate::commands::dictation::is_recording() {
                                                    let state: tauri::State<'_, AppState> = handle.state();
                                                    if let Err(e) = commands::dictation::start_recording(handle.clone(), state, None).await {
                                                        crate::error_surface::emit_float_error(&handle, &e);
                                                    }
                                                }
                                            } else {
                                                eprintln!("fonos: linux hold '{}' up", label);
                                                if crate::commands::dictation::is_recording() {
                                                    stop_and_process_dictation(handle).await;
                                                }
                                            }
                                        });
                                    }
                                }) {
                                    eprintln!("fonos: failed to register linux shortcut '{}': {e}", combo);
                                } else {
                                    eprintln!("fonos: registered linux shortcut '{}' → {}", combo, label);
                                }
                            }
                            Err(e) => eprintln!("fonos: invalid shortcut '{}': {e}", tauri_combo),
                        }
                    }
                }

                // Hot-reload: unregister all + re-register with new config
                let reload_handle = app.handle().clone();
                app.listen("hotkey:reload", move |_| {
                    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

                    let h = reload_handle.clone();
                    eprintln!("fonos: linux hotkey reload — re-registering");
                    let _ = h.global_shortcut().unregister_all();

                    let state: tauri::State<'_, AppState> = h.state();
                    let config = state.config.lock().unwrap();
                    let combos: Vec<(String, String)> = vec![
                        (config.hotkey_dictation.clone(), "dictation".into()),
                        (config.hotkey_dictation_toggle.clone(), "dictation-toggle".into()),
                    ];
                    drop(config);

                    for (combo, label) in combos {
                        if let Some(tauri_combo) = to_tauri_shortcut(&combo) {
                            if let Ok(shortcut) = tauri_combo.parse::<Shortcut>() {
                                let lbl = label.clone();
                                let h2 = h.clone();
                                if let Err(e) = h.global_shortcut().on_shortcut(shortcut, move |_app, _sc, event| {
                                    let handle = h2.clone();
                                    let l = lbl.clone();
                                    let is_toggle = l == "dictation-toggle";
                                    let pressed = event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed;
                                    if is_toggle && !pressed { return; }
                                    tauri::async_runtime::spawn(async move {
                                        if is_toggle || !pressed {
                                            // toggle press or hold release → stop
                                            if crate::commands::dictation::is_recording() {
                                                stop_and_process_dictation(handle).await;
                                            }
                                        } else {
                                            // hold press → start
                                            if !crate::commands::dictation::is_recording() {
                                                let state: tauri::State<'_, AppState> = handle.state();
                                                if let Err(e) = commands::dictation::start_recording(handle.clone(), state, None).await {
                                                    crate::error_surface::emit_float_error(&handle, &e);
                                                }
                                            }
                                        }
                                    });
                                }) {
                                    eprintln!("fonos: reload shortcut '{}' failed: {e}", combo);
                                } else {
                                    eprintln!("fonos: reloaded linux shortcut '{}' → {}", combo, label);
                                }
                            }
                        }
                    }
                });
            }

            // 2. Position float window at bottom center of primary screen.
            commands::dictation::move_float_to_primary_pub(app.handle());

            // 2b. Re-place the float pill whenever the display configuration
            // changes (external monitor connected/disconnected, resolution or
            // arrangement change) so it never strands on coordinates computed
            // for a display that is gone. macOS-only; Tauri exposes no
            // cross-platform monitors-changed event.
            #[cfg(target_os = "macos")]
            commands::dictation::register_display_reconfig_callback(app.handle());

            // 3. Tray menu.
            use tauri::menu::PredefinedMenuItem;

            let show_item = MenuItem::with_id(app, "show_app", "Open Fonos", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Fonos", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[
                &show_item,
                &PredefinedMenuItem::separator(app)?,
                &quit_item,
            ])?;

            if let Some(tray) = app.tray_by_id("main") {
                tray.set_menu(Some(menu))?;
                tray.set_show_menu_on_left_click(true)?;

                // On Linux, use light icon for dark panel backgrounds
                #[cfg(target_os = "linux")]
                {
                    use tauri::image::Image;
                    if let Ok(icon) = Image::from_path("resources/tray_icon_light.png")
                        .or_else(|_| Image::from_path("/usr/lib/Fonos/resources/tray_icon_light.png"))
                    {
                        let _ = tray.set_icon(Some(icon));
                    }
                }

                let app_handle_menu = app.handle().clone();
                tray.on_menu_event(move |_tray, event| {
                    let id = event.id().0.as_str();
                    match id {
                        "show_app" => {
                            raise_main_window(&app_handle_menu);
                        }
                        "quit" => {
                            // Hide every window up front (most importantly the
                            // always-on-top `float` pill) so nothing is left
                            // visible on screen while the process winds down —
                            // on Linux, GTK/WebKit teardown can take a moment
                            // or hang outright, which previously stranded the
                            // pill frozen mid-dictation. RunEvent::Exit below
                            // is what actually guarantees the process dies.
                            for (_, window) in app_handle_menu.webview_windows() {
                                let _ = window.hide();
                            }
                            // Stop any live mic capture (e.g. quitting
                            // mid-dictation) via the same stop path the
                            // dictation commands use, so the cpal stream is
                            // dropped instead of left running past exit.
                            let state: tauri::State<'_, AppState> = app_handle_menu.state();
                            let _ = commands::dictation::stop_and_drain(state.inner());
                            app_handle_menu.exit(0);
                        }
                        _ => {}
                    }
                });
            }

            // Listen for show-main-window event from float pill
            use tauri::Listener;
            let app_handle_show = app.handle().clone();
            app.handle().listen("show-main-window", move |event| {
                use tauri::Emitter;
                raise_main_window(&app_handle_show);
                if let Some(w) = app_handle_show.get_webview_window("main") {
                    // Forward the payload (e.g. "settings") so the React app can navigate
                    let _ = w.emit("navigate-tab", event.payload());
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Fonos app")
        .run(|_app_handle, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                raise_main_window(_app_handle);
            }
            // Hard fallback: once the event loop reports Exit, force the
            // process down. Background threads we don't (and can't) join —
            // the detached SIGUSR2 signal-hook thread (main.rs ~698), any
            // lingering GTK/WebKit teardown on Linux — must not be able to
            // keep the process alive after the window/tray lifecycle is done.
            if let tauri::RunEvent::Exit = _event {
                std::process::exit(0);
            }
        });
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod adapters;
mod error_surface;
#[cfg(target_os = "macos")]
mod hotkey;
mod injection;
mod skills;
mod tray;
mod trigger_label;
mod window;

use commands::AppState;
use fonos_core::config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use window::raise_main_window;

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

/// Every (combo, dispatch-label) the app should bind, derived from live config:
/// one entry per workflow Hotkey chip (`workflow-{id}@{idx}`) plus the pill's
/// own key (`"pill"`). The single source of truth for *what* to register,
/// shared by the macOS CGEventTap registration ([`build_hotkey_configs`]) and
/// the Linux global-shortcut registration ([`register_workflow_shortcuts`]), so
/// both platforms bind the exact same set of hotkeys.
///
/// `cfg`-free so `cargo check`/`cargo test` exercise it on macOS.
fn hotkey_bindings(config: &AppConfig) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Dictation / note / listen / text-actions / selection recipes are all
    // unified onto the workflow engine (Workflow P1): every Hotkey chip on a
    // workflow registers its own `workflow-{id}@{trigger_idx}` label (Workbench
    // P1). Agent/meeting/STS's former standalone labels are gone the same way —
    // they are now Hotkey chips on wf.agent*/wf.meeting/wf.call.
    for wf in fonos_core::workflow::engine::effective_workflows(config) {
        for (idx, combo, _capture) in wf.hotkey_triggers() {
            if combo.is_empty() {
                continue;
            }
            out.push((combo.to_string(), crate::trigger_label::hotkey_label(&wf.id, idx)));
        }
    }
    // Pill-owned hotkey (Workbench P1, spec §3c): the floating pill holds its
    // own global key, separate from any recipe's Hotkey chips, dispatched by
    // the `"pill"` arm.
    if !config.pill_hotkey.is_empty() {
        out.push((config.pill_hotkey.clone(), "pill".to_string()));
    }
    out
}

/// Build all macOS CGEventTap hotkey configs from the current app config, by
/// parsing every combo [`hotkey_bindings`] resolves.
#[cfg(target_os = "macos")]
fn build_hotkey_configs(config: &AppConfig) -> Vec<hotkey::HotkeyConfig> {
    let mut configs = Vec::new();
    for (combo, label) in hotkey_bindings(config) {
        match hotkey::HotkeyManager::parse_hotkey(&combo, &label) {
            Ok(hk) => configs.push(hk),
            Err(e) => eprintln!("fonos: could not parse {} hotkey '{}': {}", label, combo, e),
        }
    }
    configs
}

/// Map a fonos combo string (`"cmd+shift+space"`, the same format
/// [`hotkey::HotkeyManager::parse_hotkey`] consumes on macOS) to the token
/// string the global-shortcut plugin's `Shortcut` parser accepts
/// (`"CommandOrControl+Shift+space"`).
///
/// The last `+`-separated token is the key, earlier tokens are modifiers — same
/// split as the macOS parser. `cmd`/`command` map to `CommandOrControl` so a
/// macOS-authored combo lands on Control (not the Super key) under X11. Only the
/// key names that differ between fonos's macOS key table
/// ([`hotkey`]`::key_name_to_code`) and the case-insensitive global-hotkey
/// parser need translation; every other token (letters, digits, raw
/// punctuation, `space`, `tab`, `escape`, arrows, F-keys) is accepted verbatim.
/// Returns `None` for an empty combo or an unknown modifier token, so the caller
/// skips (and logs) an unbindable combo. Final-key validity is confirmed by
/// `Shortcut::from_str` at registration time.
///
/// `cfg`-free (pure) so it is unit-tested on macOS; only *called* on Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn to_plugin_shortcut(combo: &str) -> Option<String> {
    if combo.is_empty() {
        return None;
    }
    let parts: Vec<&str> = combo.split('+').collect();
    let (key_tok, mod_toks) = parts.split_last()?; // parts is non-empty
    let mut out: Vec<String> = Vec::with_capacity(parts.len());
    for m in mod_toks {
        out.push(
            match m.trim().to_lowercase().as_str() {
                "cmd" | "command" => "CommandOrControl",
                "ctrl" | "control" => "Control",
                "shift" => "Shift",
                "alt" | "opt" | "option" => "Alt",
                _ => return None, // unknown modifier — unbindable
            }
            .to_string(),
        );
    }
    let key = key_tok.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    let key = match key.as_str() {
        "return" => "enter",          // plugin has no RETURN alias (ENTER only)
        "delete" | "backspace" => "backspace", // macOS `delete` IS the Backspace key
        "forwarddelete" => "delete",  // plugin DELETE == forward-delete
        "grave" => "backquote",       // plugin has no GRAVE alias (Backquote / `)
        other => other,
    };
    out.push(key.to_string());
    Some(out.join("+"))
}

/// Debounces a fast physical double-press of a toggle-capture hotkey so it
/// doesn't re-trigger the same mic workflow twice in quick succession.
/// Key-repeat is already suppressed by the hotkey layer, so this only guards
/// against an actual double tap. Shared by every mic-sourced trigger — the
/// `workflow-{id}` and `pill` hotkey arms alike (see
/// [`dispatch_workflow_trigger`]) — since the guard is about a fast physical
/// gesture, not about which specific hotkey fired.
/// `cfg`-free: shared by both the macOS CGEventTap callback and the Linux
/// global-shortcut callback (both drive [`dispatch_workflow_trigger`]).
static TOGGLE_DEBOUNCE_LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
const TOGGLE_DELAY_MS: u64 = 500;

/// The shared mic hold/toggle dispatch dance for every workflow-triggering
/// hotkey arm (`workflow-{id}` and `pill` alike): given the resolved target
/// workflow id, whether its source is a microphone widget, this trigger's
/// capture mode ("hold" or "toggle"), and the key event, either runs the
/// workflow — mic sources on the correct hold/toggle edge (toggle key-downs
/// debounced via [`TOGGLE_DEBOUNCE_LAST`]), non-mic sources once on
/// key-down — or finishes an in-flight capture.
///
/// `cfg`-free (platform-neutral): the macOS CGEventTap callback and the Linux
/// global-shortcut callback share it verbatim, so dictation and every recipe
/// hotkey take the exact same engine path on both platforms.
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

/// Resolve a `workflow-{id}@{idx}` dispatch label against the live config into
/// the `(workflow id, is_mic, capture)` [`dispatch_workflow_trigger`] needs.
///
/// The label carries the fired trigger chip as `workflow-{id}@{trigger_idx}`
/// (Workbench P1 — one binding per Hotkey chip). `is_mic` gates the two-phase
/// mic dance and comes from the target workflow's source widget; `capture`
/// ("hold"|"toggle") comes from that workflow's `triggers[trigger_idx]`. `None`
/// when the label resolves to no live workflow (a dangling/renamed trigger) — the
/// caller logs and drops the keystroke.
///
/// `cfg`-free: the macOS CGEventTap callback and the Linux global-shortcut
/// callback share it, so a rebind in Workbench takes effect identically on both.
fn resolve_workflow_trigger(config: &AppConfig, label: &str) -> Option<(String, bool, String)> {
    let (base_label, trigger_idx) = crate::trigger_label::parse_hotkey_label(label);
    let wf_id = fonos_core::workflow::engine::resolve_trigger_target(base_label);
    let widgets = fonos_core::workflow::engine::effective_widgets(config);
    let workflows = fonos_core::workflow::engine::effective_workflows(config);
    workflows.iter().find(|w| w.id == wf_id).map(|wf| {
        let src = widgets.iter().find(|w| w.id == wf.source);
        let is_mic = src.map(|w| w.type_tag == "microphone").unwrap_or(false);
        let capture = wf
            .hotkey_triggers()
            .find(|(i, _, _)| *i == trigger_idx)
            .map(|(_, _, cap)| cap.to_string())
            .unwrap_or_else(|| "hold".to_string());
        (wf.id.clone(), is_mic, capture)
    })
}

/// Resolve the pill-owned hotkey against the live config (Workbench P1, spec
/// §3c). Runs whichever workflow the pill roller currently has selected
/// (`active_voice_workflow`), falling back to the built-in `wf.dictation` — the
/// "run whatever's selected" behavior the old `workflow-wf.dictation` redirect
/// used to provide. `capture` comes from `config.pill_hotkey_capture` (the
/// pill's key isn't a per-workflow trigger chip). `None` when the resolved id
/// has no live workflow.
///
/// `cfg`-free: shared by the macOS + Linux callbacks. Resolving `active_voice_workflow`
/// *at keypress time* (not registration time) is what lets the pill key follow
/// the roller without a `hotkey:reload`, on both platforms.
fn resolve_pill_trigger(config: &AppConfig) -> Option<(String, bool, String)> {
    let workflows = fonos_core::workflow::engine::effective_workflows(config);
    let active = &config.active_voice_workflow;
    let wf_id = if !active.is_empty() && workflows.iter().any(|w| w.id == *active) {
        active.clone()
    } else {
        "wf.dictation".to_string()
    };
    let widgets = fonos_core::workflow::engine::effective_widgets(config);
    let capture = if config.pill_hotkey_capture.is_empty() {
        "hold".to_string()
    } else {
        config.pill_hotkey_capture.clone()
    };
    workflows.iter().find(|w| w.id == wf_id).map(|wf| {
        let src = widgets.iter().find(|w| w.id == wf.source);
        let is_mic = src.map(|w| w.type_tag == "microphone").unwrap_or(false);
        (wf.id.clone(), is_mic, capture)
    })
}

/// Register every workflow + pill hotkey with the Linux global-shortcut plugin.
///
/// Reads the live config through [`hotkey_bindings`] — the same source of truth
/// the macOS CGEventTap uses — maps each fonos combo to the plugin's `Shortcut`
/// format ([`to_plugin_shortcut`]), and installs a per-combo handler that mirrors
/// the macOS callback: it resolves the dispatch target from live config
/// ([`resolve_pill_trigger`] / [`resolve_workflow_trigger`]) on each keypress and
/// hands off to the shared [`dispatch_workflow_trigger`]. Called on startup and,
/// after an `unregister_all()`, on every `hotkey:reload`.
///
/// The plugin (via `global-hotkey`) always *consumes* a registered combo — the
/// grabbed key never reaches the focused window — so the workflow `capture`
/// flag ("hold"/"toggle") is honored, but there is no separate "don't capture"
/// mode to implement (X11 `XGrabKey` has none; this matches the macOS
/// CGEventTap, which likewise always drops a matched hotkey). Wayland is out of
/// scope: `global-hotkey` is X11-only.
#[cfg(target_os = "linux")]
fn register_workflow_shortcuts(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    let bindings = {
        let state: tauri::State<'_, AppState> = app.state();
        let config = match state.config.lock() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("fonos: linux hotkey registration — config lock poisoned: {e}");
                return;
            }
        };
        hotkey_bindings(&config)
    };

    let gs = app.global_shortcut();
    for (combo, label) in bindings {
        let Some(plugin_combo) = to_plugin_shortcut(&combo) else {
            eprintln!("fonos: skipping unbindable combo '{combo}' ({label})");
            continue;
        };
        let shortcut: Shortcut = match plugin_combo.parse() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("fonos: invalid shortcut '{plugin_combo}' ({label}): {e}");
                continue;
            }
        };
        let handle = app.clone();
        let lbl = label.clone();
        let res = gs.on_shortcut(shortcut, move |_app, _sc, event| {
            let handle = handle.clone();
            let label = lbl.clone();
            // The plugin delivers exactly one Pressed on physical key-down and
            // one Released on key-up: `global-hotkey` enables X11
            // DETECTABLE_AUTO_REPEAT and dedups by held-state, so auto-repeat is
            // suppressed just like the macOS CGEventTap's own held-set.
            let is_down = event.state == ShortcutState::Pressed;
            tauri::async_runtime::spawn(async move {
                let resolved = {
                    let state: tauri::State<'_, AppState> = handle.state();
                    let config = match state.config.lock() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("fonos: linux hotkey — config lock poisoned: {e}");
                            return;
                        }
                    };
                    if label == "pill" {
                        resolve_pill_trigger(&config)
                    } else {
                        resolve_workflow_trigger(&config, &label)
                    }
                };
                let Some((wf_id, is_mic, capture)) = resolved else {
                    eprintln!("fonos: linux hotkey '{label}' resolved to no workflow — ignoring");
                    return;
                };
                dispatch_workflow_trigger(handle.clone(), wf_id, is_mic, &capture, is_down).await;
            });
        });
        match res {
            Ok(()) => eprintln!("fonos: registered linux shortcut '{combo}' → {label}"),
            Err(e) => eprintln!("fonos: failed to register linux shortcut '{combo}' ({label}): {e}"),
        }
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
    // Onboarding funnel milestones (local-only, record-once) — idempotent.
    fonos_core::funnel::init_db(&db_conn);
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
            commands::permissions::request_accessibility,
            // Onboarding funnel (local-only)
            commands::funnel::record_onboarding_event,
            commands::funnel::get_onboarding_events,
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
                tray_menu: Arc::new(Mutex::new(None)),
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

            // 0. SIGUSR2 handler — toggle dictation from external scripts / window
            //    managers (the documented fallback where global key grabs don't
            //    exist, e.g. Wayland). Dispatches through the same engine path as
            //    the hotkeys; a signal has no key-up edge, so it always acts as a
            //    toggle press regardless of the pill's hold/toggle setting.
            #[cfg(unix)]
            {
                let sig_handle = app.handle().clone();
                std::thread::spawn(move || {
                    use signal_hook::iterator::Signals;
                    let mut signals = Signals::new(&[signal_hook::consts::SIGUSR2])
                        .expect("failed to register SIGUSR2 handler");
                    for _ in signals.forever() {
                        eprintln!("fonos: SIGUSR2 received — toggling dictation");
                        let handle = sig_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let resolved = {
                                let state: tauri::State<'_, AppState> = handle.state();
                                let config = match state.config.lock() {
                                    Ok(c) => c,
                                    Err(e) => {
                                        eprintln!("fonos: SIGUSR2 — config lock poisoned: {e}");
                                        return;
                                    }
                                };
                                resolve_pill_trigger(&config)
                            };
                            let Some((wf_id, is_mic, _capture)) = resolved else {
                                eprintln!("fonos: SIGUSR2 resolved to no workflow — ignoring");
                                return;
                            };
                            dispatch_workflow_trigger(handle.clone(), wf_id, is_mic, "toggle", true)
                                .await;
                        });
                    }
                });
            }

            // 1. Global hotkeys. macOS uses a hand-rolled CGEventTap; Linux uses
            //    the global-shortcut plugin (block 1b below). Both register the
            //    same set (`hotkey_bindings`) and dispatch through the same
            //    workflow engine (`dispatch_workflow_trigger`).
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
                                // Resolve the trigger target under the config
                                // lock, drop it before any await, then dispatch.
                                // Resolution logic is shared with the Linux
                                // global-shortcut callback via
                                // `resolve_workflow_trigger` — a missing/dangling
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
                                    resolve_workflow_trigger(&config, l)
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
                                // runs whichever workflow the pill roller
                                // currently has selected. Resolution is shared
                                // with the Linux callback via
                                // `resolve_pill_trigger`.
                                let resolved = {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    let config = match state.config.lock() {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("fonos: pill hotkey — config lock poisoned: {e}");
                                            return;
                                        }
                                    };
                                    resolve_pill_trigger(&config)
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
            // Registers EVERY workflow Hotkey chip plus the pill's own key
            // (`hotkey_bindings`) and routes each through the same workflow
            // engine the macOS CGEventTap uses — dictation (hold via the pill,
            // toggle via wf.dictation-toggle) and every selection recipe alike.
            // X11 only (global-shortcut can't grab under Wayland).
            #[cfg(target_os = "linux")]
            {
                register_workflow_shortcuts(app.handle());

                // Hot-reload: on a config change (e.g. a rebind in Workbench),
                // drop every OS grab and re-register from the fresh config.
                let reload_handle = app.handle().clone();
                app.listen("hotkey:reload", move |_| {
                    use tauri_plugin_global_shortcut::GlobalShortcutExt;
                    eprintln!("fonos: linux hotkey reload — re-registering all workflow shortcuts");
                    let _ = reload_handle.global_shortcut().unregister_all();
                    register_workflow_shortcuts(&reload_handle);
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

            // 3. Tray menu — health panel (onboarding P2), built in tray.rs.
            tray::setup_tray(app)?;

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

#[cfg(test)]
mod tests {
    use super::to_plugin_shortcut;
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    /// Every string `to_plugin_shortcut` emits must parse as a plugin
    /// `Shortcut` (the plugin uses global-hotkey's `from_str`), or the Linux
    /// registration would silently drop the binding.
    fn assert_parses(combo: &str) {
        let mapped = to_plugin_shortcut(combo)
            .unwrap_or_else(|| panic!("`{combo}` should map to Some"));
        Shortcut::from_str(&mapped)
            .unwrap_or_else(|e| panic!("`{combo}` → `{mapped}` failed to parse: {e}"));
    }

    #[test]
    fn maps_modifiers_and_letter() {
        // cmd → CommandOrControl (lands on Control under X11, not the Super key).
        assert_eq!(to_plugin_shortcut("cmd+shift+a").as_deref(), Some("CommandOrControl+Shift+a"));
        assert_eq!(to_plugin_shortcut("command+shift+a").as_deref(), Some("CommandOrControl+Shift+a"));
        assert_eq!(to_plugin_shortcut("ctrl+alt+z").as_deref(), Some("Control+Alt+z"));
        assert_eq!(to_plugin_shortcut("control+option+z").as_deref(), Some("Control+Alt+z"));
    }

    #[test]
    fn maps_named_and_special_keys() {
        assert_eq!(to_plugin_shortcut("cmd+shift+space").as_deref(), Some("CommandOrControl+Shift+space"));
        // Keys whose fonos name differs from the global-hotkey token get remapped.
        assert_eq!(to_plugin_shortcut("cmd+return").as_deref(), Some("CommandOrControl+enter"));
        assert_eq!(to_plugin_shortcut("cmd+delete").as_deref(), Some("CommandOrControl+backspace"));
        assert_eq!(to_plugin_shortcut("cmd+backspace").as_deref(), Some("CommandOrControl+backspace"));
        assert_eq!(to_plugin_shortcut("cmd+forwarddelete").as_deref(), Some("CommandOrControl+delete"));
        assert_eq!(to_plugin_shortcut("cmd+grave").as_deref(), Some("CommandOrControl+backquote"));
    }

    #[test]
    fn single_key_combo_has_no_modifiers() {
        assert_eq!(to_plugin_shortcut("f5").as_deref(), Some("f5"));
    }

    #[test]
    fn empty_and_unknown_modifier_reject() {
        assert_eq!(to_plugin_shortcut(""), None);
        // "hyper" is not a modifier fonos/macOS recognizes → unbindable.
        assert_eq!(to_plugin_shortcut("hyper+a"), None);
    }

    #[test]
    fn every_mapping_parses_as_a_plugin_shortcut() {
        // A representative spread across the key families the macOS parser
        // (`hotkey::key_name_to_code`) accepts: letters, digits, raw and named
        // punctuation, whitespace/control keys, arrows, and function keys.
        for combo in [
            "cmd+shift+a",
            "ctrl+alt+z",
            "cmd+9",
            "cmd+space",
            "cmd+return",
            "cmd+enter",
            "cmd+tab",
            "cmd+escape",
            "cmd+esc",
            "cmd+delete",
            "cmd+backspace",
            "cmd+forwarddelete",
            "cmd+up",
            "cmd+arrowdown",
            "cmd+left",
            "cmd+right",
            "cmd+f1",
            "cmd+f12",
            "cmd+f19",
            "cmd+minus",
            "cmd+-",
            "cmd+equal",
            "cmd+=",
            "cmd+bracketleft",
            "cmd+[",
            "cmd+semicolon",
            "cmd+;",
            "cmd+comma",
            "cmd+,",
            "cmd+slash",
            "cmd+/",
            "cmd+period",
            "cmd+.",
            "cmd+backslash",
            "cmd+quote",
            "cmd+grave",
            "cmd+backquote",
            "cmd+`",
        ] {
            assert_parses(combo);
        }
    }
}

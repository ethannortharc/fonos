#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod adapters;
mod error_surface;
#[cfg(target_os = "macos")]
mod hotkey;
mod injection;
mod skills;

use commands::AppState;
use fonos_core::config::AppConfig;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};

/// Position the agent-panel window centered horizontally near the cursor,
/// slightly above the vertical center of the screen.
#[cfg(target_os = "macos")]
fn move_agent_panel_to_cursor(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(panel) = app.get_webview_window("agent-panel") else { return };

    let monitors = match panel.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return,
    };

    // Find the monitor that contains the cursor
    let cursor = {
        let source = core_graphics::event_source::CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState
        ).expect("CGEventSource");
        let event = core_graphics::event::CGEvent::new(source).expect("CGEvent");
        event.location()
    };

    let target = monitors.iter().find(|m| {
        let scale = m.scale_factor();
        let lx = m.position().x as f64 / scale;
        let ly = m.position().y as f64 / scale;
        let lw = m.size().width as f64 / scale;
        let lh = m.size().height as f64 / scale;
        cursor.x >= lx && cursor.x < lx + lw && cursor.y >= ly && cursor.y < ly + lh
    }).unwrap_or_else(|| &monitors[0]);

    let scale = target.scale_factor();
    let panel_w = 340.0; // logical pixels — matches tauri.conf.json width

    // Convert monitor bounds to logical
    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;

    // Top-center: drops down from the menu bar area like a water drop
    let x = mon_x + (mon_w - panel_w) / 2.0;
    let y = mon_y + 32.0; // Just below the macOS menu bar (28pt)

    let _ = panel.set_position(tauri::PhysicalPosition::new(
        (x * scale) as i32,
        (y * scale) as i32,
    ));
}

/// Position the note-panel window centered horizontally near the cursor,
/// slightly below the macOS menu bar — mirrors agent panel placement.
#[cfg(target_os = "macos")]
fn move_note_panel_to_cursor(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(panel) = app.get_webview_window("note-panel") else { return };

    let monitors = match panel.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return,
    };

    let cursor = {
        let source = core_graphics::event_source::CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState
        ).expect("CGEventSource");
        let event = core_graphics::event::CGEvent::new(source).expect("CGEvent");
        event.location()
    };

    let target = monitors.iter().find(|m| {
        let scale = m.scale_factor();
        let lx = m.position().x as f64 / scale;
        let ly = m.position().y as f64 / scale;
        let lw = m.size().width as f64 / scale;
        let lh = m.size().height as f64 / scale;
        cursor.x >= lx && cursor.x < lx + lw && cursor.y >= ly && cursor.y < ly + lh
    }).unwrap_or_else(|| &monitors[0]);

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

/// Position the meeting-panel window in the bottom-right corner of the active monitor,
/// above the Dock — a fixed corner so it doesn't obscure the meeting app window.
#[cfg(target_os = "macos")]
fn move_meeting_panel_to_cursor(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(panel) = app.get_webview_window("meeting-panel") else { return };

    let monitors = match panel.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return,
    };

    let cursor = {
        let source = core_graphics::event_source::CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState
        ).expect("CGEventSource");
        let event = core_graphics::event::CGEvent::new(source).expect("CGEvent");
        event.location()
    };

    let target = monitors.iter().find(|m| {
        let scale = m.scale_factor();
        let lx = m.position().x as f64 / scale;
        let ly = m.position().y as f64 / scale;
        let lw = m.size().width as f64 / scale;
        let lh = m.size().height as f64 / scale;
        cursor.x >= lx && cursor.x < lx + lw && cursor.y >= ly && cursor.y < ly + lh
    }).unwrap_or_else(|| &monitors[0]);

    let scale = target.scale_factor();
    let panel_w = 520.0_f64;
    let top_margin = 80.0_f64;

    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;

    // Right edge of panel flush with right edge of screen, near the top
    let x = mon_x + mon_w - panel_w;
    let y = mon_y + top_margin;

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
                let mode = {
                    let cfg = state2.config.lock().unwrap();
                    cfg.dictation_mode.clone()
                };
                let has_llm = {
                    let all = fonos_core::modes::all_modes();
                    all.get(&mode).map_or(false, |m| m.system.is_some() || m.user_template.is_some())
                };
                if has_llm {
                    // stop_recording already left the pill in the processing
                    // state for LLM modes; just run the LLM and emit the final
                    // float:stop/float:error below.
                    {
                        // Shared post-LLM flow (fonos-core pipeline, issue #21): deliver the
                        // processed text, emit exactly one terminal pill event, classify errors.
                        let llm_res = commands::llm::process_with_llm(state2, result.text, mode.clone()).await;
                        let stage = llm_res.map(|l| fonos_core::pipeline::LlmStageOutput {
                            processed: l.processed,
                            auto_paste: l.auto_paste,
                            auto_press_enter: l.auto_press_enter,
                        });
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
    try_add(&config.hotkey_dictation, "dictation");
    try_add(&config.hotkey_dictation_toggle, "dictation-toggle");
    try_add(&config.hotkey_agent, "agent");
    try_add(&config.hotkey_agent_panel, "agent-panel");
    try_add(&config.hotkey_note, "note");
    try_add(&config.hotkey_meeting, "meeting");
    try_add(&config.hotkey_transform, "transform");
    try_add(&config.hotkey_listen, "listen");
    try_add(&config.hotkey_sts, "sts");
    if config.notebook_hotkey_1 > 0 { try_add(&config.hotkey_note_1, "note-1"); }
    if config.notebook_hotkey_2 > 0 { try_add(&config.hotkey_note_2, "note-2"); }
    if config.notebook_hotkey_3 > 0 { try_add(&config.hotkey_note_3, "note-3"); }
    configs
}

fn main() {
    let config = AppConfig::load();

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
        let system_prompt = config.agent_system_prompt.clone();
        let timeout_secs = config.agent_timeout_secs;

        AgentState::new(
            registry,
            context,
            fast_path,
            system_prompt,
            timeout_secs,
            builtin_skill_names,
            Arc::clone(&safety),
        )
    };

    let meeting_state = commands::meeting::MeetingState::new();

    let app_state = AppState {
        audio_capture: Arc::new(Mutex::new(None)),
        audio_playback: Arc::new(Mutex::new(None)),
        config: Arc::new(Mutex::new(config)),
        db: Arc::new(Mutex::new(db_conn)),
        agent: Arc::new(tokio::sync::Mutex::new(agent_state)),
        meeting: Arc::new(tokio::sync::Mutex::new(meeting_state)),
        note_target: Arc::new(Mutex::new(None)),
        agent_selection: Arc::new(tokio::sync::Mutex::new(None)),
        sts_session: Arc::new(tokio::sync::Mutex::new(fonos_core::sts::StsSession::default())),
    };

    // `mut` is only exercised on Linux (the global-shortcut plugin block below);
    // on macOS the binding is never reassigned.
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Register global-shortcut plugin on Linux
    #[cfg(target_os = "linux")]
    {
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    builder
        .manage(app_state)
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
            // Permission commands
            commands::listen::create_listen_from_text,
            commands::sts::reset_sts_session,
            commands::sts::sts_page_start,
            commands::sts::sts_page_stop,
            commands::sts::get_sts_history,
            commands::tts::list_model_voices,
            commands::permissions::check_accessibility,
            commands::permissions::open_settings_pane,
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
            commands::resize_agent_panel,
            commands::hide_agent_panel,
            commands::hide_note_panel,
            commands::set_note_notebook,
            // LLM commands
            commands::llm::process_with_llm,
            commands::llm::probe_model,
            commands::llm::list_provider_models,
            commands::llm::list_modes,
            commands::llm::save_custom_mode,
            commands::llm::delete_custom_mode,
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
        ])
        .setup(|app| {
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
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
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
            let (dictation_combo, dictation_toggle_combo,
                 agent_combo, agent_panel_combo, note_combo, meeting_combo,
                 transform_combo,
                    listen_combo,
                    sts_combo,
                 note1_combo, note2_combo, note3_combo,
                 note1_nb, note2_nb, note3_nb) = {
                let config = state.config.lock().unwrap();
                (
                    config.hotkey_dictation.clone(),
                    config.hotkey_dictation_toggle.clone(),
                    config.hotkey_agent.clone(),
                    config.hotkey_agent_panel.clone(),
                    config.hotkey_note.clone(),
                    config.hotkey_meeting.clone(),
                    config.hotkey_transform.clone(),
                    config.hotkey_listen.clone(),
                    config.hotkey_sts.clone(),
                    config.hotkey_note_1.clone(),
                    config.hotkey_note_2.clone(),
                    config.hotkey_note_3.clone(),
                    config.notebook_hotkey_1,
                    config.notebook_hotkey_2,
                    config.notebook_hotkey_3,
                )
            };

            let mut hm = hotkey::HotkeyManager::new();
            let mut any_hotkey = false;

            match hotkey::HotkeyManager::parse_hotkey(&dictation_combo, "dictation") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse dictation hotkey '{}': {}", dictation_combo, e),
            }
            if !dictation_toggle_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&dictation_toggle_combo, "dictation-toggle") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse dictation-toggle hotkey '{}': {}", dictation_toggle_combo, e),
                }
            }
            match hotkey::HotkeyManager::parse_hotkey(&agent_combo, "agent") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse agent hotkey '{}': {}", agent_combo, e),
            }
            match hotkey::HotkeyManager::parse_hotkey(&agent_panel_combo, "agent-panel") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse agent-panel hotkey '{}': {}", agent_panel_combo, e),
            }
            match hotkey::HotkeyManager::parse_hotkey(&note_combo, "note") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse note hotkey '{}': {}", note_combo, e),
            }
            match hotkey::HotkeyManager::parse_hotkey(&meeting_combo, "meeting") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse meeting hotkey '{}': {}", meeting_combo, e),
            }
            if !transform_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&transform_combo, "transform") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse transform hotkey '{}': {}", transform_combo, e),
                }
            }
            if !listen_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&listen_combo, "listen") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse listen hotkey '{}': {}", listen_combo, e),
                }
            }
            if !sts_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&sts_combo, "sts") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse sts hotkey '{}': {}", sts_combo, e),
                }
            }
            // Notebook-specific note shortcuts (only if a notebook is bound)
            eprintln!("fonos: note shortcuts: 1='{}' nb={}, 2='{}' nb={}, 3='{}' nb={}",
                note1_combo, note1_nb, note2_combo, note2_nb, note3_combo, note3_nb);
            if note1_nb > 0 && !note1_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&note1_combo, "note-1") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse note-1 hotkey '{}': {}", note1_combo, e),
                }
            }
            if note2_nb > 0 && !note2_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&note2_combo, "note-2") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse note-2 hotkey '{}': {}", note2_combo, e),
                }
            }
            if note3_nb > 0 && !note3_combo.is_empty() {
                match hotkey::HotkeyManager::parse_hotkey(&note3_combo, "note-3") {
                    Ok(hk) => { hm.register(hk); any_hotkey = true; }
                    Err(e) => eprintln!("fonos: could not parse note-3 hotkey '{}': {}", note3_combo, e),
                }
            }

            if any_hotkey {
                let app_handle = app.handle().clone();
                // Move notebook IDs into the closure so note-1/2/3 handlers can access them
                let nb1 = note1_nb;
                let nb2 = note2_nb;
                let nb3 = note3_nb;
                // Toggle state: generation counter + last action time for debounce
                let toggle_gen: Arc<std::sync::atomic::AtomicU64> = Arc::new(std::sync::atomic::AtomicU64::new(0));
                let toggle_last_action: Arc<Mutex<std::time::Instant>> = Arc::new(Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(10)));
                const TOGGLE_DELAY_MS: u64 = 500; // hold 500ms to trigger
                let tg_gen = Arc::clone(&toggle_gen);
                let tg_last = Arc::clone(&toggle_last_action);
                hm.set_callback(move |label, is_down| {
                    use tauri::Emitter;
                    let handle = app_handle.clone();
                    let label = label.to_string();
                    let note1_nb = nb1;
                    let note2_nb = nb2;
                    let note3_nb = nb3;
                    let gen = Arc::clone(&tg_gen);
                    let last_action = Arc::clone(&tg_last);
                    tauri::async_runtime::spawn(async move {
                        match label.as_str() {
                            "dictation" | "dictation-toggle" => {
                                let is_toggle = label == "dictation-toggle";

                                // Hold mode: key_down=start, key_up=stop
                                if !is_toggle {
                                    if is_down {
                                        let state: tauri::State<'_, AppState> = handle.state();
                                        if let Err(e) = commands::dictation::start_recording(
                                            handle.clone(), state, None
                                        ).await {
                                            crate::error_surface::emit_float_error(&handle, &e);
                                        }
                                        return;
                                    }
                                    // key_up → fall through to stop logic
                                } else {
                                    // Toggle mode: hold for 500ms to trigger (fires while held).
                                    // key_down → increment generation, spawn delayed check
                                    // key_up → increment generation (cancels pending)
                                    // Debounce: ignore if <500ms since last action
                                    use std::sync::atomic::Ordering;

                                    if !is_down {
                                        // Key released — cancel any pending delayed trigger
                                        gen.fetch_add(1, Ordering::SeqCst);
                                        return;
                                    }

                                    // Key down — check debounce
                                    {
                                        let last = last_action.lock().unwrap();
                                        if last.elapsed().as_millis() < TOGGLE_DELAY_MS as u128 {
                                            eprintln!("fonos: toggle debounce — too soon since last action");
                                            return;
                                        }
                                    }

                                    // Increment generation and spawn a delayed check
                                    let my_gen = gen.fetch_add(1, Ordering::SeqCst) + 1;
                                    let gen2 = Arc::clone(&gen);
                                    let last2 = Arc::clone(&last_action);
                                    let h2 = handle.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(TOGGLE_DELAY_MS)).await;

                                        // Check if generation still matches (key hasn't been released/re-pressed)
                                        if gen2.load(Ordering::SeqCst) != my_gen {
                                            eprintln!("fonos: toggle cancelled (key released before {}ms)", TOGGLE_DELAY_MS);
                                            return;
                                        }

                                        // Record action time for debounce
                                        *last2.lock().unwrap() = std::time::Instant::now();

                                        if crate::commands::dictation::is_recording() {
                                            eprintln!("fonos: toggle → stopping");
                                            let state: tauri::State<'_, AppState> = h2.state();
                                            let state2: tauri::State<'_, AppState> = h2.state();
                                            let dictation_t0 = std::time::Instant::now();
                                            match commands::dictation::stop_recording(h2.clone(), state, None).await {
                                                Ok(result) => {
                                                    if result.text.is_empty() {
                                                        let _ = h2.emit("float:stop", "");
                                                    } else {
                                                        let mode = {
                                                            let cfg = state2.config.lock().unwrap();
                                                            cfg.dictation_mode.clone()
                                                        };
                                                        let has_llm = {
                                                            let all = fonos_core::modes::all_modes();
                                                            all.get(&mode).map_or(false, |m| m.system.is_some() || m.user_template.is_some())
                                                        };
                                                        if has_llm {
                                                            {
                                                                // Shared post-LLM flow (fonos-core pipeline, issue #21): deliver the
                                                                // processed text, emit exactly one terminal pill event, classify errors.
                                                                let llm_res = commands::llm::process_with_llm(state2, result.text, mode.clone()).await;
                                                                let stage = llm_res.map(|l| fonos_core::pipeline::LlmStageOutput {
                                                                    processed: l.processed,
                                                                    auto_paste: l.auto_paste,
                                                                    auto_press_enter: l.auto_press_enter,
                                                                });
                                                                let (events, text_sink) = {
                                                                    let s: tauri::State<AppState> = h2.state();
                                                                    (
                                                                        crate::adapters::PillEventSink(h2.clone()),
                                                                        crate::adapters::InjectionTextSink(s.config.clone()),
                                                                    )
                                                                };
                                                                if fonos_core::pipeline::deliver_llm_result(stage, &events, &text_sink).await
                                                                    == fonos_core::pipeline::DeliveryOutcome::Delivered
                                                                {
                                                                    // End-to-end dictation latency (key release → delivered), issue #4.
                                                                    let db_arc = {
                                                                        let s: tauri::State<AppState> = h2.state();
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
                                                        // Raw mode — stop_recording already emitted
                                                        // float:stop/float:error; don't repaint over it.
                                                    }
                                                }
                                                Err(e) => {
                                                    if e.contains("not recording") {
                                                        // Harmless start/stop race — silent idle revert.
                                                        let _ = h2.emit("float:stop", "");
                                                    } else {
                                                        crate::error_surface::emit_float_error(&h2, &e);
                                                    }
                                                }
                                            }
                                        } else {
                                            eprintln!("fonos: toggle → starting");
                                            let state: tauri::State<'_, AppState> = h2.state();
                                            if let Err(e) = commands::dictation::start_recording(h2.clone(), state, None).await {
                                                crate::error_surface::emit_float_error(&h2, &e);
                                            }
                                        }
                                    });
                                    return;
                                }

                                // ── Stop + process (hold key-up OR toggle second press) ──
                                {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    let state2: tauri::State<'_, AppState> = handle.state();
                                    let dictation_t0 = std::time::Instant::now();
                                    match commands::dictation::stop_recording(
                                        handle.clone(), state, None
                                    ).await {
                                        Ok(result) => {
                                            if !result.text.is_empty() {
                                                let session_id = fonos_core::stats::new_session_id();
                                                let mode = {
                                                    let cfg = state2.config.lock().unwrap();
                                                    cfg.dictation_mode.clone()
                                                };
                                                // Mode processing: LLM if mode has prompts, else raw inject
                                                let has_llm = {
                                                    let all = fonos_core::modes::all_modes();
                                                    all.get(&mode).map_or(false, |m| m.system.is_some() || m.user_template.is_some())
                                                };
                                                if has_llm {
                                                    {
                                                        // Shared post-LLM flow (fonos-core pipeline, issue #21): deliver the
                                                        // processed text, emit exactly one terminal pill event, classify errors.
                                                        let llm_res = commands::llm::process_with_llm(state2.clone(), result.text.clone(), mode.clone()).await;
                                                        let stage = llm_res.map(|l| fonos_core::pipeline::LlmStageOutput {
                                                            processed: l.processed,
                                                            auto_paste: l.auto_paste,
                                                            auto_press_enter: l.auto_press_enter,
                                                        });
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
                                                // Note: raw mode injection is handled inside stop_recording() —
                                                // do NOT inject again here for !has_llm modes.
                                                if let Ok(db) = state2.db.lock() {
                                                    let n = {
                                                        let all = fonos_core::modes::all_modes();
                                                        if all.get(&mode).map_or(false, |m| m.system.is_some() || m.user_template.is_some()) { 2 } else { 1 }
                                                    };
                                                    let _ = fonos_core::stats::tag_session(&db, &session_id, n);
                                                }
                                            } else {
                                                // Empty transcript — still stop the pill
                                                let _ = handle.emit("float:stop", "");
                                            }
                                        }
                                        Err(e) => {
                                            if e.contains("not recording") {
                                                // Harmless start/stop race — silent idle revert.
                                                let _ = handle.emit("float:stop", "");
                                            } else {
                                                crate::error_surface::emit_float_error(&handle, &e);
                                            }
                                        }
                                    }
                                }
                            }

                            "agent" => {
                                // Press-and-hold: key down starts recording, key up stops and processes.
                                // Uses Tauri WebviewWindow::eval() to call JS functions directly in
                                // the panel — bypasses the event system which doesn't reliably
                                // reach webviews that were created as hidden.
                                use tauri::Manager;

                                // Helper: run JS in the agent-panel webview via Tauri's eval() API.
                                // Strings passed to recvXxx() functions are pre-escaped by callers.
                                fn agent_js(h: &tauri::AppHandle, js: &str) {
                                    if let Some(panel) = h.get_webview_window("agent-panel") {
                                        if let Err(e) = panel.eval(js) {
                                            eprintln!("fonos: agent panel JS: {e}");
                                        }
                                    }
                                }

                                if is_down {
                                    // ── Grab selected text BEFORE showing panel (original app still focused) ──
                                    let sel = commands::selection::grab_selection().await.ok();
                                    let sel_store = {
                                        let state: tauri::State<'_, AppState> = handle.state();
                                        Arc::clone(&state.agent_selection)
                                    };
                                    *sel_store.lock().await = sel;

                                    // Stop any TTS still playing from previous interaction
                                    {
                                        let state: tauri::State<'_, AppState> = handle.state();
                                        let _ = commands::tts::stop_playback(state);
                                    }

                                    move_agent_panel_to_cursor(&handle);
                                    if let Some(panel) = handle.get_webview_window("agent-panel") {
                                        let _ = panel.show();
                                        let _ = panel.set_focus();
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                                    // Reset persistent mode (hides header + mic footer if leftover from Hotkey 2)
                                    agent_js(&handle, "recvShow(false)");
                                    agent_js(&handle, "recvRecording()");

                                    let state: tauri::State<'_, AppState> = handle.state();
                                    if let Err(e) = commands::dictation::start_recording(handle.clone(), state, Some(true)).await {
                                        eprintln!("fonos: agent hotkey start error: {e}");
                                        agent_js(&handle, "recvDismiss()");
                                    }
                                } else {
                                    agent_js(&handle, "recvRecordingStop()");

                                    // Retrieve the selection context stashed on key-down
                                    let sel_load = {
                                        let state: tauri::State<'_, AppState> = handle.state();
                                        Arc::clone(&state.agent_selection)
                                    };
                                    let sel = sel_load.lock().await.take();

                                    let state: tauri::State<'_, AppState> = handle.state();
                                    match commands::dictation::stop_recording(handle.clone(), state, Some("agent".to_string())).await {
                                        Ok(result) => {
                                            let transcript = result.text;
                                            if transcript.is_empty() {
                                                agent_js(&handle, "recvDismiss()");
                                                return;
                                            }

                                            // Build agent prompt: prepend selection context if any
                                            let has_selection = sel.as_ref().map(|s| !s.text.is_empty()).unwrap_or(false);
                                            let agent_prompt = if let Some(ref s) = sel {
                                                if !s.text.is_empty() {
                                                    format!(
                                                        "[Selected text from {}]:\n\"\"\"\n{}\n\"\"\"\n\nUser instruction: {}",
                                                        s.app_name, s.text, transcript
                                                    )
                                                } else {
                                                    transcript.clone()
                                                }
                                            } else {
                                                transcript.clone()
                                            };

                                            // Show the user message (just the spoken part).
                                            // Use serde_json to produce safe JS string literals
                                            // (handles quotes, backslashes, newlines, unicode).
                                            eprintln!("fonos: agent user-message: {}", &transcript);
                                            if has_selection {
                                                let sel_ref = sel.as_ref().unwrap();
                                                let preview: String = sel_ref.text.chars().take(120).collect();
                                                let sel_j = serde_json::to_string(&preview).unwrap_or_default();
                                                let app_j = serde_json::to_string(&sel_ref.app_name).unwrap_or_default();
                                                agent_js(&handle, &format!(
                                                    "recvSelection({}, {})",
                                                    sel_j, app_j
                                                ));
                                            }
                                            let tx_j = serde_json::to_string(&transcript).unwrap_or_default();
                                            agent_js(&handle, &format!("recvUserMessage({})", tx_j));
                                            agent_js(&handle, "recvThinking()");

                                            let state2: tauri::State<'_, AppState> = handle.state();
                                            match commands::agent::agent_process(state2, agent_prompt).await {
                                                Ok(agent_result) => {
                                                    for exec in &agent_result.skill_executions {
                                                        let p_j = serde_json::to_string(&exec.params).unwrap_or("\"\"".into());
                                                        let n_j = serde_json::to_string(&exec.skill_name).unwrap_or_default();
                                                        agent_js(&handle, &format!(
                                                            "recvSkillExec({},{},{},{})",
                                                            n_j, p_j, exec.latency_ms, exec.blocked
                                                        ));
                                                    }
                                                    let r_j = serde_json::to_string(&agent_result.response_text).unwrap_or_default();
                                                    agent_js(&handle, &format!("recvResponse({})", r_j));

                                                    // Auto-replace: switch focus back to the original app
                                                    // and paste the result. Cmd+V silently fails if the
                                                    // target isn't an editable field.
                                                    if has_selection && !agent_result.response_text.is_empty() {
                                                        let replace_text = agent_result.response_text.clone();
                                                        let target_app = sel.as_ref().map(|s| s.app_name.clone());
                                                        let _ = commands::selection::replace_selection(replace_text, target_app).await;
                                                        eprintln!("fonos: auto-replaced selection in {:?}", sel.as_ref().map(|s| &s.app_name));
                                                    }

                                                    let (tts_enabled, tts_voice, tts_speed) = {
                                                        let state3: tauri::State<'_, AppState> = handle.state();
                                                        let cfg = state3.config.lock().unwrap();
                                                        (cfg.agent_tts_enabled, cfg.default_voice.clone(), cfg.tts_speed)
                                                    };
                                                    // Track audio duration so we dismiss AFTER playback finishes
                                                    let mut audio_dur = 0.0_f64;
                                                    if tts_enabled && !agent_result.response_text.is_empty() {
                                                        // Truncate to first 3 sentences for TTS — keep it brief
                                                        let tts_text = {
                                                            let mut count = 0;
                                                            let mut end = agent_result.response_text.len();
                                                            for (i, c) in agent_result.response_text.char_indices() {
                                                                if c == '.' || c == '!' || c == '?' || c == '。' || c == '！' || c == '？' {
                                                                    count += 1;
                                                                    if count >= 3 { end = i + c.len_utf8(); break; }
                                                                }
                                                            }
                                                            agent_result.response_text[..end].to_string()
                                                        };
                                                        let state3: tauri::State<'_, AppState> = handle.state();
                                                        if let Ok(tts_result) = commands::tts::generate_and_play(
                                                            state3, tts_text, tts_voice, tts_speed
                                                        ).await {
                                                            audio_dur = tts_result.duration_secs;
                                                        }
                                                    }

                                                    // Auto-dismiss: wait for audio to finish + 2s buffer
                                                    let handle2 = handle.clone();
                                                    tokio::spawn(async move {
                                                        let wait = audio_dur + 2.0;
                                                        tokio::time::sleep(std::time::Duration::from_secs_f64(wait)).await;
                                                        agent_js(&handle2, "recvDismiss()");
                                                    });
                                                }
                                                Err(e) => {
                                                    eprintln!("fonos: agent process error: {e}");
                                                    let esc = e.replace('\\', "\\\\").replace('\'', "\\'");
                                                    agent_js(&handle, &format!("recvError('{}')", esc));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("fonos: agent stop error: {e}");
                                            agent_js(&handle, "recvDismiss()");
                                        }
                                    }
                                }
                            }

                            "agent-panel" => {
                                // Toggle: key down only (not hold)
                                if !is_down { return; }
                                use tauri::Manager;
                                if let Some(panel) = handle.get_webview_window("agent-panel") {
                                    let visible = panel.is_visible().unwrap_or(false);
                                    if visible {
                                        let _ = panel.hide();
                                    } else {
                                        move_agent_panel_to_cursor(&handle);
                                        let _ = panel.show();
                                        let _ = panel.set_focus();
                                        let _ = panel.eval("recvShow(true)");
                                    }
                                }
                            }

                            "note" => {
                                // Hold-to-talk for notes:
                                // Key-down: show panel + start recording
                                //   (if panel visible & showing result → dismiss instead)
                                // Key-up: stop recording → show result → auto-dismiss after 2s
                                use tauri::Manager;

                                fn note_js(h: &tauri::AppHandle, js: &str) {
                                    if let Some(panel) = h.get_webview_window("note-panel") {
                                        let _ = panel.eval(js);
                                    }
                                }

                                if is_down {
                                    if let Some(panel) = handle.get_webview_window("note-panel") {
                                        let visible = panel.is_visible().unwrap_or(false);
                                        if !visible {
                                            // Set default note target to Quick Note immediately (no race)
                                            crate::commands::set_default_note_target(&handle);
                                            // Show panel
                                            move_note_panel_to_cursor(&handle);
                                            let _ = panel.show();
                                            let _ = panel.set_focus();
                                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                                            note_js(&handle, "recvShow()");
                                        } else {
                                            // Panel already visible — cancel any auto-dismiss timer,
                                            // keep current notebook selection intact
                                            note_js(&handle, "cancelDismiss()");
                                        }
                                    }
                                    // Start recording (keep current notebook target)
                                    note_js(&handle, "recvRecording()");
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    if let Err(e) = commands::dictation::start_recording(
                                        handle.clone(), state, Some(true)
                                    ).await {
                                        eprintln!("fonos: note hotkey start error: {e}");
                                    }
                                } else {
                                    // Key up: stop recording, show result, auto-dismiss
                                    note_js(&handle, "recvRecordingStop()");
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    match commands::dictation::stop_recording(
                                        handle.clone(), state, Some("note".to_string())
                                    ).await {
                                        Ok(result) => {
                                            if result.text.is_empty() {
                                                eprintln!("fonos: note recording empty");
                                                // Dismiss immediately if nothing recorded
                                                note_js(&handle, "recvDismiss()");
                                                if let Some(panel) = handle.get_webview_window("note-panel") {
                                                    let _ = panel.hide();
                                                }
                                                return;
                                            }
                                            eprintln!("fonos: note saved: {} chars", result.text.len());
                                            // Show the transcribed text in the panel
                                            let esc = result.text
                                                .replace('\\', "\\\\")
                                                .replace('\'', "\\'")
                                                .replace('\n', "\\n");
                                            note_js(&handle, &format!("recvResult('{}')", esc));
                                            // The note panel is the real UI, but stop_recording
                                            // suppresses its own float:stop for LLM modes (note
                                            // has an LLM prompt) and this path runs no LLM step,
                                            // so end the float pill here — otherwise it stays
                                            // stuck in "Processing".
                                            let _ = handle.emit("float:stop", &result.text);
                                            // Panel will auto-dismiss after 2s via JS timer,
                                            // or user can press hotkey again to dismiss immediately
                                        }
                                        Err(e) => {
                                            if !e.contains("not recording") {
                                                eprintln!("fonos: note stop error: {e}");
                                            }
                                            note_js(&handle, "recvDismiss()");
                                            if let Some(panel) = handle.get_webview_window("note-panel") {
                                                let _ = panel.hide();
                                            }
                                        }
                                    }
                                }
                            }

                            "note-1" | "note-2" | "note-3" => {
                                // Notebook-specific hold-to-talk: same as "note" but sets a specific notebook target
                                use tauri::Manager;

                                fn note_nb_js(h: &tauri::AppHandle, js: &str) {
                                    if let Some(panel) = h.get_webview_window("note-panel") {
                                        let _ = panel.eval(js);
                                    }
                                }

                                // Determine which notebook ID to target
                                let nb_id = match label.as_str() {
                                    "note-1" => note1_nb,
                                    "note-2" => note2_nb,
                                    "note-3" => note3_nb,
                                    _ => 0,
                                };

                                if is_down && nb_id > 0 {
                                    // Set the notebook target
                                    {
                                        let st = handle.state::<AppState>().inner();
                                        if let Ok(mut t) = st.note_target.lock() { *t = Some(nb_id); }
                                    }
                                    // Show panel + select the target notebook
                                    if let Some(panel) = handle.get_webview_window("note-panel") {
                                        let visible = panel.is_visible().unwrap_or(false);
                                        if !visible {
                                            move_note_panel_to_cursor(&handle);
                                            let _ = panel.show();
                                            let _ = panel.set_focus();
                                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                                            note_nb_js(&handle, &format!("recvShow({})", nb_id));
                                        } else {
                                            note_nb_js(&handle, "cancelDismiss()");
                                            note_nb_js(&handle, &format!("selectNotebookById({})", nb_id));
                                        }
                                    }
                                    note_nb_js(&handle, "recvRecording()");
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    if let Err(e) = commands::dictation::start_recording(
                                        handle.clone(), state, Some(true)
                                    ).await {
                                        eprintln!("fonos: note-N hotkey start error: {e}");
                                    }
                                } else if !is_down {
                                    // Key up: stop + show result + auto-dismiss
                                    note_nb_js(&handle, "recvRecordingStop()");
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    match commands::dictation::stop_recording(
                                        handle.clone(), state, Some("note".to_string())
                                    ).await {
                                        Ok(result) => {
                                            if result.text.is_empty() {
                                                note_nb_js(&handle, "recvDismiss()");
                                                if let Some(panel) = handle.get_webview_window("note-panel") {
                                                    let _ = panel.hide();
                                                }
                                                return;
                                            }
                                            let esc = result.text.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n");
                                            note_nb_js(&handle, &format!("recvResult('{}')", esc));
                                            // End the float pill — stop_recording suppresses its
                                            // float:stop for LLM modes (note) and this path runs
                                            // no LLM step, so it'd otherwise stay in "Processing".
                                            let _ = handle.emit("float:stop", &result.text);
                                        }
                                        Err(e) => {
                                            if !e.contains("not recording") {
                                                eprintln!("fonos: note-N stop error: {e}");
                                            }
                                            note_nb_js(&handle, "recvDismiss()");
                                            if let Some(panel) = handle.get_webview_window("note-panel") {
                                                let _ = panel.hide();
                                            }
                                        }
                                    }
                                }
                            }

                            "meeting" => {
                                // Toggle meeting mode on key down only
                                if !is_down { return; }
                                use tauri::Manager;

                                let state: tauri::State<'_, AppState> = handle.state();
                                let is_recording = state.meeting.lock().await.recording;

                                if !is_recording {
                                    // Start meeting: position panel, show, start recording
                                    move_meeting_panel_to_cursor(&handle);
                                    if let Some(panel) = handle.get_webview_window("meeting-panel") {
                                        let _ = panel.show();
                                        let _ = panel.set_focus();
                                        let _ = panel.eval("recvMeetingShow()");
                                    }
                                    let state2: tauri::State<'_, AppState> = handle.state();
                                    match commands::meeting::start_meeting(handle.clone(), state2).await {
                                        Ok(cid) => {
                                            eprintln!("fonos: meeting started via hotkey, container={}", cid);
                                        }
                                        Err(e) => {
                                            eprintln!("fonos: meeting start error: {e}");
                                        }
                                    }
                                } else {
                                    // Stop meeting: stop recording, hide panel after summary
                                    let state2: tauri::State<'_, AppState> = handle.state();
                                    match commands::meeting::stop_meeting(handle.clone(), state2).await {
                                        Ok(_summary) => {
                                            eprintln!("fonos: meeting stopped via hotkey");
                                        }
                                        Err(e) => {
                                            eprintln!("fonos: meeting stop error: {e}");
                                        }
                                    }
                                    // Hide panel after a brief delay to show summary
                                    let handle2 = handle.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                        if let Some(panel) = handle2.get_webview_window("meeting-panel") {
                                            let _ = panel.hide();
                                        }
                                    });
                                }
                            }

                            "sts" => {
                                // Hold-to-talk conversation turn (issue #24):
                                // key-down records, key-up transcribes → chat → speaks.
                                if is_down {
                                    // Never start a second recording while a turn is
                                    // still thinking/speaking or another recording is
                                    // live — the orphaned recording would corrupt the
                                    // pill state and hijack the next key-up.
                                    if commands::sts::turn_in_flight() || commands::dictation::is_recording() {
                                        return;
                                    }
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    if let Err(e) = commands::dictation::start_recording(handle.clone(), state, None).await {
                                        crate::error_surface::emit_float_error(&handle, &e);
                                    }
                                } else {
                                    // Skip key-ups whose key-down was suppressed (turn
                                    // was in flight) or when no recording is live.
                                    if commands::sts::turn_in_flight() || !commands::dictation::is_recording() {
                                        return;
                                    }
                                    let _ = commands::sts::run_sts_turn(handle.clone()).await;
                                }
                            }
                            "listen" => {
                                // Capture the current selection into the Listen queue
                                // (summarize + synthesize; async, pill shows progress).
                                if !is_down { return; }
                                let _ = commands::listen::run_listen_capture(handle.clone()).await;
                            }
                            "transform" => {
                                // Quick transform: grab selection → mode's LLM step → paste back.
                                // Reuses dictation mode definitions — only applies step 2 (LLM).
                                if !is_down { return; }

                                // 1. Grab selection while the original app has focus
                                let sel = match commands::selection::grab_selection().await {
                                    Ok(s) if !s.text.is_empty() => s,
                                    _ => {
                                        eprintln!("fonos: transform — no text selected");
                                        return;
                                    }
                                };

                                eprintln!("fonos: transform — {} chars from {}", sel.text.len(), sel.app_name);

                                // 2. Look up the configured mode and build LLM prompt
                                let (mode_id, translate_target) = {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    let cfg = state.config.lock().unwrap();
                                    let mid = if cfg.transform_mode.is_empty() { "polish".to_string() } else { cfg.transform_mode.clone() };
                                    (mid, cfg.translate_target.clone())
                                };

                                let all_modes = fonos_core::modes::all_modes();
                                let mode_def = match all_modes.get(&mode_id) {
                                    Some(m) => m.clone(),
                                    None => {
                                        eprintln!("fonos: transform — mode '{}' not found", mode_id);
                                        return;
                                    }
                                };

                                // Build messages using the mode's system prompt + user_template
                                let user_template = mode_def.user_template.as_deref().unwrap_or("{text}");
                                let mut user_text = user_template.replace("{text}", &sel.text);
                                if user_text.contains("{target_lang}") {
                                    let target = if translate_target.is_empty() { "English" } else { &translate_target };
                                    user_text = user_text.replace("{target_lang}", target);
                                }

                                let mut messages = Vec::new();
                                if let Some(ref sys) = mode_def.system {
                                    messages.push(serde_json::json!({"role": "system", "content": sys}));
                                }
                                messages.push(serde_json::json!({"role": "user", "content": user_text}));

                                // 3. Resolve LLM service — mode override → global default
                                let svc = {
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    if !mode_def.model.is_empty() {
                                        commands::get_service_config_for_profile(&state, &mode_def.model)
                                    } else {
                                        commands::get_service_config(&state, "llm")
                                    }
                                };

                                eprintln!("fonos: transform mode={} provider={} model={}", mode_id, svc.provider, svc.model);

                                let result = match svc.provider.as_str() {
                                    "anthropic" => fonos_core::llm::call_anthropic(&svc.api_key, &svc.model, &messages, mode_def.temperature, mode_def.max_tokens).await,
                                    "google" => fonos_core::llm::call_google(&svc.api_key, &svc.model, &messages, mode_def.temperature, mode_def.max_tokens).await,
                                    _ => fonos_core::llm::call_openai_compatible(&svc.api_key, &svc.model, &svc.base_url, &messages, mode_def.temperature, mode_def.max_tokens, &svc.provider).await,
                                };

                                match result {
                                    Ok(resp) if !resp.text.is_empty() => {
                                        eprintln!("fonos: transform — result {} chars, replacing", resp.text.len());
                                        let _ = commands::selection::replace_selection(
                                            resp.text,
                                            Some(sel.app_name),
                                        ).await;
                                    }
                                    Ok(_) => eprintln!("fonos: transform — LLM returned empty"),
                                    Err(e) => eprintln!("fonos: transform — LLM error: {e}"),
                                }
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
                            if let Some(w) = app_handle_menu.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
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
                if let Some(w) = app_handle_show.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
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
                if let Some(w) = _app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        });
}

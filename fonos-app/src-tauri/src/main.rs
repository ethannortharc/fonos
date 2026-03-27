#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
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
    let panel_h = 200.0;

    // Convert monitor bounds to logical
    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;
    let mon_h = target.size().height as f64 / scale;

    // Top-center: drops down from the menu bar area like a water drop
    let x = mon_x + (mon_w - panel_w) / 2.0;
    let y = mon_y + 32.0; // Just below the macOS menu bar (28pt)

    let _ = panel.set_position(tauri::PhysicalPosition::new(
        (x * scale) as i32,
        (y * scale) as i32,
    ));
}

fn main() {
    let config = AppConfig::load();

    // Initialize SQLite database for stats & history
    let db_path = fonos_core::stats::db_path();
    let _ = std::fs::create_dir_all(db_path.parent().unwrap());
    let db_conn = rusqlite::Connection::open(&db_path)
        .expect("failed to open fonos.db");
    fonos_core::stats::init_db(&db_conn);

    // ── Agent state initialization ─────────────────────────────────────────
    let agent_state = {
        use fonos_core::agent::registry::SkillRegistry;
        use fonos_core::agent::context::ConversationContext;
        use fonos_core::agent::fast_path::FastPathMatcher;
        use fonos_core::agent::safety::{CommandSafetyConfig, CommandSafetyFilter};
        use fonos_core::agent::custom_loader::load_custom_skills;
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

        // Load custom skills from the app data directory.
        let skills_dir = AppConfig::config_dir().join("skills");
        if skills_dir.exists() {
            let custom_skills = load_custom_skills(&skills_dir);
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
        )
    };

    let app_state = AppState {
        audio_capture: Arc::new(Mutex::new(None)),
        audio_playback: Arc::new(Mutex::new(None)),
        config: Arc::new(Mutex::new(config)),
        db: Arc::new(Mutex::new(db_conn)),
        agent: Arc::new(tokio::sync::Mutex::new(agent_state)),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // Config commands
            commands::config::get_config,
            commands::config::save_config,
            // Dictation commands
            commands::dictation::has_microphone,
            commands::dictation::start_recording,
            commands::dictation::stop_recording,
            commands::dictation::transcribe_file,
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
            // LLM commands
            commands::llm::process_with_llm,
            commands::llm::probe_model,
            commands::llm::list_modes,
            commands::llm::save_custom_mode,
            commands::llm::delete_custom_mode,
            // Stats & History commands
            commands::stats::record_event,
            commands::stats::delete_event,
            commands::stats::get_stats,
            commands::stats::get_history,
            commands::stats::get_today,
            // Agent commands
            commands::agent::agent_process,
            commands::agent::agent_reset,
            commands::agent::list_skills,
            commands::agent::toggle_skill,
            commands::agent::save_custom_skill,
            commands::agent::delete_custom_skill,
            commands::agent::test_skill,
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
            }

            // 1. Global hotkeys.
            let state = app.state::<AppState>();
            let (dictation_combo, agent_combo, agent_panel_combo) = {
                let config = state.config.lock().unwrap();
                (
                    config.hotkey_dictation.clone(),
                    config.hotkey_agent.clone(),
                    config.hotkey_agent_panel.clone(),
                )
            };

            let mut hm = hotkey::HotkeyManager::new();
            let mut any_hotkey = false;

            match hotkey::HotkeyManager::parse_hotkey(&dictation_combo, "dictation") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse dictation hotkey '{}': {}", dictation_combo, e),
            }
            match hotkey::HotkeyManager::parse_hotkey(&agent_combo, "agent") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse agent hotkey '{}': {}", agent_combo, e),
            }
            match hotkey::HotkeyManager::parse_hotkey(&agent_panel_combo, "agent-panel") {
                Ok(hk) => { hm.register(hk); any_hotkey = true; }
                Err(e) => eprintln!("fonos: could not parse agent-panel hotkey '{}': {}", agent_panel_combo, e),
            }

            if any_hotkey {
                let app_handle = app.handle().clone();
                hm.set_callback(move |label, is_down| {
                    use tauri::Emitter;
                    let handle = app_handle.clone();
                    let label = label.to_string();
                    tauri::async_runtime::spawn(async move {
                        match label.as_str() {
                            "dictation" => {
                                let state: tauri::State<'_, AppState> = handle.state();

                                if is_down {
                                    if let Err(e) = commands::dictation::start_recording(
                                        handle.clone(), state, None
                                    ).await {
                                        eprintln!("fonos: hotkey start error: {e}");
                                        let _ = handle.emit("float:stop", "");
                                    }
                                } else {
                                    let state2: tauri::State<'_, AppState> = handle.state();
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
                                                    match commands::llm::process_with_llm(
                                                        state2.clone(), result.text.clone(), mode.clone()
                                                    ).await {
                                                        Ok(llm_result) => {
                                                            if !llm_result.processed.is_empty() {
                                                                if llm_result.auto_paste {
                                                                    let _ = crate::injection::inject_text(&llm_result.processed);
                                                                    if llm_result.auto_press_enter {
                                                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                                                        crate::injection::press_enter();
                                                                    }
                                                                }
                                                            }
                                                            let _ = handle.emit("float:stop", &llm_result.processed);
                                                        }
                                                        Err(e) => {
                                                            eprintln!("fonos: hotkey LLM error: {e}");
                                                            let _ = handle.emit("float:stop", "");
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
                                            let _ = handle.emit("float:stop", "");
                                            if !e.contains("not recording") {
                                                eprintln!("fonos: hotkey stop error: {e}");
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
                                    let state: tauri::State<'_, AppState> = handle.state();
                                    match commands::dictation::stop_recording(handle.clone(), state, Some("agent".to_string())).await {
                                        Ok(result) => {
                                            let transcript = result.text;
                                            if transcript.is_empty() {
                                                agent_js(&handle, "recvDismiss()");
                                                return;
                                            }

                                            eprintln!("fonos: agent user-message: {}", &transcript);
                                            let esc = transcript.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n");
                                            agent_js(&handle, &format!("recvUserMessage('{}')", esc));
                                            agent_js(&handle, "recvThinking()");

                                            let state2: tauri::State<'_, AppState> = handle.state();
                                            match commands::agent::agent_process(state2, transcript).await {
                                                Ok(agent_result) => {
                                                    for exec in &agent_result.skill_executions {
                                                        let p = serde_json::to_string(&exec.params).unwrap_or_default()
                                                            .replace('\\', "\\\\").replace('\'', "\\'");
                                                        let n = exec.skill_name.replace('\'', "\\'");
                                                        agent_js(&handle, &format!(
                                                            "recvSkillExec('{}','{}',{},{})",
                                                            n, p, exec.latency_ms, exec.blocked
                                                        ));
                                                    }
                                                    let r = agent_result.response_text
                                                        .replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n");
                                                    agent_js(&handle, &format!("recvResponse('{}')", r));

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

                            _ => {}
                        }
                    });
                });

                if let Err(e) = hm.start() {
                    eprintln!("fonos: hotkey registration failed: {}", e);
                }
            }

            // 2. Position float window at primary screen bottom center (above Dock).
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
        .run(|app_handle, event| {
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        });
}

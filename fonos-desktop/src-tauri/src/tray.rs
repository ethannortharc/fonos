//! Tray health panel (onboarding P2): four status rows + unlock/doctor
//! entries, refreshed in place via menu-item handles kept in `AppState`.
//! All copy lives in the bilingual table below (resolve_lang decides EN/ZH);
//! menu ids never change across languages.

use fonos_core::workflow::builtin::{resolve_lang, Lang};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::AppState;

/// Handles to the mutable menu items, kept in `AppState.tray_menu` so
/// `refresh_tray_status` can `set_text` in place instead of rebuilding.
pub struct TrayHandles {
    pub mic: MenuItem<Wry>,
    pub stt: MenuItem<Wry>,
    pub llm: MenuItem<Wry>,
    pub tts: MenuItem<Wry>,
    pub unlock: MenuItem<Wry>,
    pub doctor: MenuItem<Wry>,
    pub settings: MenuItem<Wry>,
}

/// Which status row a progress override targets (P3 will drive Llm/Tts).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayRow {
    Mic,
    Stt,
    Llm,
    Tts,
}

/// One row's displayed state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowState {
    Ok,
    Unconfigured,
    Warn,
    Progress(u8),
}

/// Which unlock notification (Task 3 consumes the copy pair).
// Not yet called from the bin target — Task 3 wires the notification that
// consumes this (mirrors `move_note_panel_to_cursor` in main.rs).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnlockRole {
    Llm,
    Tts,
}

/// Row label + state glyph → the menu-item text, bilingual.
pub fn row_text(row: TrayRow, state: RowState, lang: Lang) -> String {
    let label = match (row, lang) {
        (TrayRow::Mic, Lang::En) => "🎤 Microphone",
        (TrayRow::Mic, Lang::Zh) => "🎤 麦克风",
        (TrayRow::Stt, Lang::En) => "📝 Dictation",
        (TrayRow::Stt, Lang::Zh) => "📝 听写",
        (TrayRow::Llm, Lang::En) => "🧠 AI commands",
        (TrayRow::Llm, Lang::Zh) => "🧠 AI 命令",
        (TrayRow::Tts, Lang::En) => "🔊 Voice replies",
        (TrayRow::Tts, Lang::Zh) => "🔊 语音回复",
    };
    let glyph = match state {
        RowState::Ok => "✓".to_string(),
        RowState::Unconfigured => "○".to_string(),
        RowState::Warn => "⚠️".to_string(),
        RowState::Progress(pct) => format!("⏳ {pct}%"),
    };
    format!("{label}  {glyph}")
}

/// Unlock/manage entry text: unconfigured → invitation, configured → manage.
pub fn unlock_text(llm_configured: bool, lang: Lang) -> &'static str {
    match (llm_configured, lang) {
        (false, Lang::En) => "✨ Unlock AI commands…",
        (false, Lang::Zh) => "✨ 解锁 AI 命令模式…",
        (true, Lang::En) => "＋ Add or manage models…",
        (true, Lang::Zh) => "＋ 添加或管理模型…",
    }
}

/// Doctor entry text.
pub fn doctor_text(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "🩺 Check & repair",
        Lang::Zh => "🩺 检查与修复",
    }
}

/// Settings entry text.
pub fn settings_text(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "⚙️ Settings",
        Lang::Zh => "⚙️ 设置",
    }
}

/// Unlock notification copy: (title, body) — "capability + a sentence you
/// can literally say", never an abstract explanation (spec §P2 formula).
// Not yet called from the bin target — Task 3 wires the notification.
#[allow(dead_code)]
pub fn unlock_body(lang: Lang, role: UnlockRole) -> (&'static str, &'static str) {
    match (role, lang) {
        (UnlockRole::Llm, Lang::En) => (
            "Command mode is ready",
            "Select some text, hold the hotkey and say “make it shorter”.",
        ),
        (UnlockRole::Llm, Lang::Zh) => (
            "命令模式已就绪",
            "选中一段文字，按住热键说“改得简短些”。",
        ),
        (UnlockRole::Tts, Lang::En) => (
            "Voice replies & calls are ready",
            "Hold the hotkey and ask “what's on my calendar today?” — it will talk back.",
        ),
        (UnlockRole::Tts, Lang::Zh) => (
            "语音回复与通话已就绪",
            "按住热键问“今天日历上有什么？”，它会说给你听。",
        ),
    }
}

/// Build the health-panel menu, stash the handles, wire click routing.
/// Replaces the old two-item menu built inline in `main.rs`.
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let mic = MenuItem::with_id(app, "tray_mic", "🎤", true, None::<&str>)?;
    let stt = MenuItem::with_id(app, "tray_stt", "📝", true, None::<&str>)?;
    let llm = MenuItem::with_id(app, "tray_llm", "🧠", true, None::<&str>)?;
    let tts = MenuItem::with_id(app, "tray_tts", "🔊", true, None::<&str>)?;
    let unlock = MenuItem::with_id(app, "tray_unlock", "✨", true, None::<&str>)?;
    let doctor = MenuItem::with_id(app, "tray_doctor", "🩺", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray_settings", "⚙️", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show_app", "Open Fonos", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Fonos", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &mic,
            &stt,
            &llm,
            &tts,
            &PredefinedMenuItem::separator(app)?,
            &unlock,
            &doctor,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &show_item,
            &quit_item,
        ],
    )?;

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
                    crate::window::raise_main_window(&app_handle_menu);
                }
                // Status rows route to where the fix lives: model roles → the
                // Models page; mic/dictation/doctor → Settings (Doctor card).
                "tray_llm" | "tray_tts" | "tray_unlock" => {
                    crate::window::raise_main_window(&app_handle_menu);
                    if let Some(w) = app_handle_menu.get_webview_window("main") {
                        let _ = w.emit("navigate-tab", "models");
                    }
                }
                "tray_mic" | "tray_stt" | "tray_doctor" | "tray_settings" => {
                    crate::window::raise_main_window(&app_handle_menu);
                    if let Some(w) = app_handle_menu.get_webview_window("main") {
                        let _ = w.emit("navigate-tab", "settings");
                    }
                }
                "quit" => {
                    // Hide every window up front (most importantly the
                    // always-on-top `float` pill) so nothing is left visible
                    // on screen while the process winds down — on Linux,
                    // GTK/WebKit teardown can take a moment or hang outright.
                    // RunEvent::Exit below is what actually guarantees the
                    // process dies.
                    for (_, window) in app_handle_menu.webview_windows() {
                        let _ = window.hide();
                    }
                    let state: tauri::State<'_, AppState> = app_handle_menu.state();
                    let _ = crate::commands::dictation::stop_and_drain(state.inner());
                    app_handle_menu.exit(0);
                }
                _ => {}
            }
        });
    }

    // Stash the handles, then paint the initial state (incl. doctor/settings
    // labels, which are language-dependent even though their state is static).
    {
        let state: tauri::State<'_, AppState> = app.handle().state();
        if let Ok(mut slot) = state.tray_menu.lock() {
            *slot = Some(TrayHandles { mic, stt, llm, tts, unlock, doctor, settings });
        };
    }
    refresh_tray_status(app.handle(), None);
    Ok(())
}

/// Recompute every row from live config/permission state and repaint the
/// menu-item texts in place. `progress` overrides one row with `⏳ N%`
/// (diarize download targets Stt; P3's engine setup will target Llm/Tts).
pub fn refresh_tray_status(app: &AppHandle, progress: Option<(TrayRow, u8)>) {
    let state: tauri::State<'_, AppState> = app.state();

    let (lang, stt_cfg, llm_cfg, tts_cfg) = match state.config.lock() {
        Ok(cfg) => (
            resolve_lang(&cfg.ui_language),
            cfg.is_stt_configured(),
            cfg.is_llm_configured(),
            cfg.is_tts_configured(),
        ),
        Err(e) => {
            eprintln!("fonos: refresh_tray_status — config lock poisoned: {e}");
            return;
        }
    };

    let mic_ok = crate::commands::dictation::has_microphone().unwrap_or(false);
    let ax_ok = crate::injection::accessibility_trusted();

    let base = |row: TrayRow| -> RowState {
        match row {
            TrayRow::Mic => {
                if mic_ok {
                    RowState::Ok
                } else {
                    RowState::Warn
                }
            }
            TrayRow::Stt => {
                if stt_cfg && ax_ok {
                    RowState::Ok
                } else if stt_cfg {
                    RowState::Warn
                } else {
                    RowState::Unconfigured
                }
            }
            TrayRow::Llm => {
                if llm_cfg {
                    RowState::Ok
                } else {
                    RowState::Unconfigured
                }
            }
            TrayRow::Tts => {
                if tts_cfg {
                    RowState::Ok
                } else {
                    RowState::Unconfigured
                }
            }
        }
    };
    let with_progress = |row: TrayRow| -> RowState {
        match progress {
            Some((p_row, pct)) if p_row == row => RowState::Progress(pct),
            _ => base(row),
        }
    };

    if let Ok(slot) = state.tray_menu.lock() {
        if let Some(h) = slot.as_ref() {
            let _ = h.mic.set_text(row_text(TrayRow::Mic, with_progress(TrayRow::Mic), lang));
            let _ = h.stt.set_text(row_text(TrayRow::Stt, with_progress(TrayRow::Stt), lang));
            let _ = h.llm.set_text(row_text(TrayRow::Llm, with_progress(TrayRow::Llm), lang));
            let _ = h.tts.set_text(row_text(TrayRow::Tts, with_progress(TrayRow::Tts), lang));
            let _ = h.unlock.set_text(unlock_text(llm_cfg, lang));
            let _ = h.doctor.set_text(doctor_text(lang));
            let _ = h.settings.set_text(settings_text(lang));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_text_composes_label_and_glyph() {
        assert_eq!(row_text(TrayRow::Mic, RowState::Ok, Lang::Zh), "🎤 麦克风  ✓");
        assert_eq!(
            row_text(TrayRow::Llm, RowState::Unconfigured, Lang::En),
            "🧠 AI commands  ○"
        );
        assert_eq!(
            row_text(TrayRow::Stt, RowState::Progress(62), Lang::Zh),
            "📝 听写  ⏳ 62%"
        );
        assert_eq!(row_text(TrayRow::Tts, RowState::Warn, Lang::En), "🔊 Voice replies  ⚠️");
    }

    #[test]
    fn unlock_text_switches_on_configured() {
        assert_eq!(unlock_text(false, Lang::Zh), "✨ 解锁 AI 命令模式…");
        assert_eq!(unlock_text(true, Lang::En), "＋ Add or manage models…");
    }

    #[test]
    fn unlock_body_pairs_capability_with_a_sayable_sentence() {
        let (t, b) = unlock_body(Lang::Zh, UnlockRole::Llm);
        assert_eq!(t, "命令模式已就绪");
        assert!(b.contains("改得简短些"));
        let (t, b) = unlock_body(Lang::En, UnlockRole::Tts);
        assert!(t.contains("Voice replies"));
        assert!(b.contains("calendar"));
    }
}

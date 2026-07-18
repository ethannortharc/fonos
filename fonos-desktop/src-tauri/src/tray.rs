//! Tray health panel (onboarding P2): four status rows + unlock/doctor
//! entries, refreshed in place via menu-item handles kept in `AppState`.
//! All copy lives in the bilingual table below (resolve_lang decides EN/ZH);
//! menu ids never change across languages.

use std::sync::Mutex;
use std::time::{Duration, Instant};

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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnlockRole {
    Llm,
    Tts,
}

/// Row label + state glyph → the menu-item text, bilingual.
///
/// Deliberately emoji-free: native menus can't render the app's stroke-SVG
/// icon language, and color-emoji prefixes clash with it — plain labels are
/// the macOS-native look. State glyphs are monochrome TEXT glyphs only
/// (`\u{FE0E}` forces the warn sign's text presentation, never the color
/// emoji).
pub fn row_text(row: TrayRow, state: RowState, lang: Lang) -> String {
    let label = match (row, lang) {
        (TrayRow::Mic, Lang::En) => "Microphone",
        (TrayRow::Mic, Lang::Zh) => "麦克风",
        (TrayRow::Stt, Lang::En) => "Dictation",
        (TrayRow::Stt, Lang::Zh) => "听写",
        (TrayRow::Llm, Lang::En) => "AI commands",
        (TrayRow::Llm, Lang::Zh) => "AI 命令",
        (TrayRow::Tts, Lang::En) => "Voice replies",
        (TrayRow::Tts, Lang::Zh) => "语音回复",
    };
    let glyph = match state {
        RowState::Ok => "✓".to_string(),
        RowState::Unconfigured => "○".to_string(),
        RowState::Warn => "⚠\u{FE0E}".to_string(),
        RowState::Progress(pct) => format!("{pct}%"),
    };
    format!("{label}  {glyph}")
}

/// Unlock/manage entry text: unconfigured → invitation, configured → manage.
pub fn unlock_text(llm_configured: bool, lang: Lang) -> &'static str {
    match (llm_configured, lang) {
        (false, Lang::En) => "Unlock AI commands…",
        (false, Lang::Zh) => "解锁 AI 命令模式…",
        (true, Lang::En) => "＋ Add or manage models…",
        (true, Lang::Zh) => "＋ 添加或管理模型…",
    }
}

/// Doctor entry text.
pub fn doctor_text(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Check & repair",
        Lang::Zh => "检查与修复",
    }
}

/// Settings entry text.
pub fn settings_text(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Settings",
        Lang::Zh => "设置",
    }
}

/// Unlock notification copy: (title, body) — "capability + a sentence you
/// can literally say", never an abstract explanation (spec §P2 formula).
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

/// The unlock transition: a role's default profile going from empty to
/// non-empty. Swapping one profile for another is not an unlock.
pub fn unlocked(old: &str, new: &str) -> bool {
    old.trim().is_empty() && !new.trim().is_empty()
}

/// Empty-selection notice copy: (title, body). The whole message is
/// "No text selected — select some text and try again"; the em-dash maps
/// onto this notification's (title, body) split, mirroring `unlock_body`'s
/// shape (a state title + an actionable sentence). This is a *notice*, not an
/// error and not "no speech" — a text source produced empty input.
pub fn empty_input_body(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::En => ("No text selected", "Select some text and try again."),
        Lang::Zh => ("未选中文本", "请选中文本后重试。"),
    }
}

/// Minimum gap between empty-input notices. Unlike [`notify_unlock`] (fires
/// at most once ever, funnel-gated), this is on the hotkey's hot path: mashing
/// a selection hotkey with nothing selected would otherwise re-run the
/// permission-check/request/show sequence on every single attempt. Same
/// pattern as `main.rs`'s `TOGGLE_DEBOUNCE_LAST`.
static EMPTY_INPUT_LAST: Mutex<Option<Instant>> = Mutex::new(None);
const EMPTY_INPUT_DEBOUNCE: Duration = Duration::from_secs(10);

/// Shared permission-check/request/show boilerplate for this module's two OS
/// notifications ([`notify_empty_input`], [`notify_unlock`]): request
/// permission lazily if not already granted, silently skip on denial (a
/// notification must never gate the flow — spec: 拒绝不阻塞), then show.
/// `context` labels the `eprintln!` diagnostics so a skipped/failed
/// notification is traceable to its caller.
fn show_notification(app: &AppHandle, context: &str, title: &str, body: &str) {
    use tauri_plugin_notification::{NotificationExt, PermissionState};

    let n = app.notification();
    let granted = matches!(n.permission_state(), Ok(PermissionState::Granted))
        || matches!(n.request_permission(), Ok(PermissionState::Granted));
    if !granted {
        eprintln!("fonos: {context} skipped — notification permission not granted");
        return;
    }
    if let Err(e) = n.builder().title(title).body(body).show() {
        eprintln!("fonos: {context} failed: {e}");
    }
}

/// Fire the "no text selected" notice for the hotkey path (float pill only
/// flashes red, which alone doesn't say *why*). Language is resolved from
/// live config, like [`refresh_tray_status`].
///
/// Two things this must NOT do on the caller's path — the pipeline's event
/// emit, i.e. every single empty-input attempt: (1) re-run the permission
/// check/request/show boilerplate back-to-back on a mashed hotkey, guarded by
/// [`EMPTY_INPUT_LAST`]; (2) block on the notification-permission calls
/// themselves ([`show_notification`] can request permission synchronously),
/// deferred via `tauri::async_runtime::spawn` so the engine's call path never
/// waits on it.
pub fn notify_empty_input(app: &AppHandle) {
    {
        let mut last = EMPTY_INPUT_LAST.lock().unwrap();
        let debounced = last.map(|t| t.elapsed() < EMPTY_INPUT_DEBOUNCE).unwrap_or(false);
        if debounced {
            return;
        }
        *last = Some(Instant::now());
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<'_, AppState> = app.state();
        let lang = match state.config.lock() {
            Ok(cfg) => resolve_lang(&cfg.ui_language),
            Err(e) => {
                eprintln!("fonos: notify_empty_input — config lock poisoned: {e}");
                Lang::En
            }
        };
        let (title, body) = empty_input_body(lang);
        show_notification(&app, "empty-input notice", title, body);
    });
}

/// Fire the once-ever unlock notification. Permission is requested lazily on
/// first use; a denial is silently absorbed — notifications are a nicety,
/// never a gate (spec: 拒绝不阻塞). Call sites (`commands/config.rs`) already
/// funnel-gate this to once-ever, so unlike [`notify_empty_input`] it neither
/// debounces nor needs to get off the caller's path.
pub fn notify_unlock(app: &AppHandle, role: UnlockRole, lang: Lang) {
    let (title, body) = unlock_body(lang, role);
    show_notification(app, "unlock notification", title, body);
}

/// Build the health-panel menu, stash the handles, wire click routing.
/// Replaces the old two-item menu built inline in `main.rs`.
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    // Placeholder texts only — refresh_tray_status paints the real labels
    // before the menu is ever shown (end of this function).
    let mic = MenuItem::with_id(app, "tray_mic", "…", true, None::<&str>)?;
    let stt = MenuItem::with_id(app, "tray_stt", "…", true, None::<&str>)?;
    let llm = MenuItem::with_id(app, "tray_llm", "…", true, None::<&str>)?;
    let tts = MenuItem::with_id(app, "tray_tts", "…", true, None::<&str>)?;
    let unlock = MenuItem::with_id(app, "tray_unlock", "…", true, None::<&str>)?;
    let doctor = MenuItem::with_id(app, "tray_doctor", "…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray_settings", "…", true, None::<&str>)?;
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

    // Clone the seven MenuItem handles out of the lock, then drop the guard
    // BEFORE any `set_text`. `MenuItem::set_text` blocks on the main thread, and
    // this function is itself reachable from a sync command on the main thread —
    // holding `tray_menu` across the blocking calls is a circular wait (the
    // classic tray deadlock). The handles are cheap Arc-backed clones, so this
    // costs nothing and drives the OS menu outside the lock.
    let handles = match state.tray_menu.lock() {
        Ok(slot) => slot.as_ref().map(|h| {
            (
                h.mic.clone(),
                h.stt.clone(),
                h.llm.clone(),
                h.tts.clone(),
                h.unlock.clone(),
                h.doctor.clone(),
                h.settings.clone(),
            )
        }),
        Err(e) => {
            eprintln!("fonos: refresh_tray_status — tray_menu lock poisoned: {e}");
            return;
        }
    };
    if let Some((mic, stt, llm, tts, unlock, doctor, settings)) = handles {
        let _ = mic.set_text(row_text(TrayRow::Mic, with_progress(TrayRow::Mic), lang));
        let _ = stt.set_text(row_text(TrayRow::Stt, with_progress(TrayRow::Stt), lang));
        let _ = llm.set_text(row_text(TrayRow::Llm, with_progress(TrayRow::Llm), lang));
        let _ = tts.set_text(row_text(TrayRow::Tts, with_progress(TrayRow::Tts), lang));
        let _ = unlock.set_text(unlock_text(llm_cfg, lang));
        let _ = doctor.set_text(doctor_text(lang));
        let _ = settings.set_text(settings_text(lang));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_text_composes_label_and_glyph() {
        assert_eq!(row_text(TrayRow::Mic, RowState::Ok, Lang::Zh), "麦克风  ✓");
        assert_eq!(
            row_text(TrayRow::Llm, RowState::Unconfigured, Lang::En),
            "AI commands  ○"
        );
        assert_eq!(
            row_text(TrayRow::Stt, RowState::Progress(62), Lang::Zh),
            "听写  62%"
        );
        assert_eq!(row_text(TrayRow::Tts, RowState::Warn, Lang::En), "Voice replies  ⚠\u{FE0E}");
    }

    /// The tray is a native menu (no SVG): the no-emoji rule is enforced as
    /// "every label/glyph stays out of the emoji blocks" so a color-emoji
    /// prefix can't sneak back in.
    #[test]
    fn tray_texts_contain_no_emoji() {
        let is_emoji = |s: &str| {
            s.chars().any(|c| {
                let cp = c as u32;
                (0x1F000..=0x1FAFF).contains(&cp) || (0x2600..=0x27BF).contains(&cp) && c != '✓' && c != '⚠'
            })
        };
        for lang in [Lang::En, Lang::Zh] {
            for row in [TrayRow::Mic, TrayRow::Stt, TrayRow::Llm, TrayRow::Tts] {
                for state in [RowState::Ok, RowState::Unconfigured, RowState::Warn, RowState::Progress(50)] {
                    assert!(!is_emoji(&row_text(row, state, lang)), "emoji in {row:?}/{state:?}");
                }
            }
            assert!(!is_emoji(unlock_text(false, lang)));
            assert!(!is_emoji(unlock_text(true, lang)));
            assert!(!is_emoji(doctor_text(lang)));
            assert!(!is_emoji(settings_text(lang)));
        }
    }

    #[test]
    fn unlock_text_switches_on_configured() {
        assert_eq!(unlock_text(false, Lang::Zh), "解锁 AI 命令模式…");
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

    #[test]
    fn empty_input_body_is_a_notice_not_no_speech() {
        let (t, b) = empty_input_body(Lang::En);
        assert_eq!(t, "No text selected");
        assert!(b.contains("Select some text"));
        // The reserved "no speech" vocabulary must never leak into this notice.
        assert!(!t.to_lowercase().contains("speech") && !b.to_lowercase().contains("speech"));
        let (t, b) = empty_input_body(Lang::Zh);
        assert_eq!(t, "未选中文本");
        assert!(b.contains("请选中文本"));
    }

    #[test]
    fn unlocked_fires_only_on_empty_to_nonempty() {
        assert!(unlocked("", "scenario-openai-llm"));
        assert!(unlocked("  ", "p1"));
        assert!(!unlocked("p0", "p1")); // swap ≠ unlock
        assert!(!unlocked("p0", "")); // clearing ≠ unlock
        assert!(!unlocked("", ""));
    }
}

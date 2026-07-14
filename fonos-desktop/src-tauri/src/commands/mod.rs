//! Tauri command handlers exposed to the frontend via invoke().

pub mod agent;
pub mod agent_widget;
pub mod bench;
pub mod call;
pub mod call_widget;
pub mod config;
pub mod dialog;
pub mod diarize;
pub mod dictation;
pub mod doctor;
pub mod engine_setup;
pub mod funnel;
pub mod llm;
pub mod meeting;
pub mod meeting_widget;
pub mod permissions;
pub mod scenarios;
pub mod selection;
pub mod sts;
pub mod stats;
pub mod storage;
pub mod text_action;
pub mod tts;
pub mod update;
pub mod voices;
pub mod widget_uppercase;
pub mod workflow_cfg;
pub mod workflow_exec;
pub mod workflow_widgets;

// Re-export storage commands at the commands level so integration tests can
// import them as `fonos_app::commands::list_entries` etc.
#[allow(unused_imports)]
pub use storage::{
    list_entries,
    get_entry,
    update_entry,
    delete_entry,
    search_entries,
    list_containers,
    create_container,
    delete_container,
    update_container_metadata,
    get_container_entries,
    export_notebook_md,
    export_notebook_json,
};

// Re-export existing command functions for the compat test imports
#[allow(unused_imports)]
pub use dictation::{has_microphone, start_recording, stop_recording, transcribe_file};
#[allow(unused_imports)]
pub use tts::{synthesize_speech, generate_and_play, play_audio_file, play_speech, stop_playback, pause_playback, resume_playback};
#[allow(unused_imports)]
pub use config::{get_config, save_config};
#[allow(unused_imports)]
pub use stats::{record_event, delete_event, get_stats, get_history, get_today};
#[allow(unused_imports)]
pub use agent::{agent_process, agent_reset, list_skills, toggle_skill, save_custom_skill, delete_custom_skill, test_skill};

use std::sync::{Arc, Mutex};

use crate::audio::capture::AudioCapture;
use crate::audio::playback::AudioPlayback;
use fonos_core::config::AppConfig;

/// Hide the agent-panel window and stop any TTS playback.
#[tauri::command]
pub fn hide_agent_panel(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    use tauri::Manager;
    let _ = tts::stop_playback(state);
    dictation::force_reset_recording();
    if let Some(w) = app.get_webview_window("agent-panel") {
        let _ = w.hide();
    }
    Ok(())
}

/// Set the target notebook for note mode. Called by note panel when user selects a notebook.
/// Pass container_id = 0 or negative to clear (Quick Note).
#[tauri::command(rename_all = "snake_case")]
pub fn set_note_notebook(state: tauri::State<'_, AppState>, container_id: i64) -> Result<(), String> {
    let mut target = state.note_target.lock().map_err(|e| e.to_string())?;
    *target = if container_id > 0 { Some(container_id) } else { None };
    eprintln!("fonos: note target set to {:?}", *target);
    Ok(())
}

/// Hide the note-panel window and force-reset the recording state
/// to prevent stale IS_RECORDING flag from blocking future dictation.
#[tauri::command]
pub fn hide_note_panel(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    // Force-reset the recording flag in case the note session left it stale
    dictation::force_reset_recording();
    if let Some(w) = app.get_webview_window("note-panel") {
        let _ = w.hide();
    }
    Ok(())
}

/// Resize the agent-panel window, keeping it centered at its current position.
#[tauri::command]
pub fn resize_agent_panel(app: tauri::AppHandle, width: u32, height: u32) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("agent-panel") {
        let old_size = w.outer_size().map_err(|e| e.to_string())?;
        let old_pos = w.outer_position().map_err(|e| e.to_string())?;

        // Keep the top-left corner anchored (let height grow downward)
        let new_x = old_pos.x + (old_size.width as i32 - width as i32) / 2;
        let new_y = old_pos.y;

        w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width, height)))
            .map_err(|e| e.to_string())?;
        w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, new_y)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Resize the float window, keeping it horizontally centered on its current monitor
/// and pinned to the same bottom edge. Uses absolute monitor center rather than
/// relative offsets to prevent rounding drift across resize cycles.
///
/// No-op geometry is skipped: on a transparent NSWindow every set_size /
/// set_position forces the window server to re-composite the surface, so
/// repeated same-geometry calls (idle→idle reverts, defensive re-sizing)
/// would churn the compositor for nothing — and every such churn is another
/// chance to interleave a stale frame into the composite.
///
/// NOTE: this command must stay synchronous (non-async), so it runs on the
/// main thread and float.html's `await invoke('resize_float')` is a real
/// barrier: when the promise resolves, the frame change has been applied.
/// The front-end's paint/resize ordering relies on that.
#[tauri::command]
pub fn resize_float(app: tauri::AppHandle, width: u32, height: u32) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("float") {
        let old_size = w.outer_size().map_err(|e| e.to_string())?;
        let old_pos = w.outer_position().map_err(|e| e.to_string())?;

        // Keep the same bottom edge
        let bottom = old_pos.y + old_size.height as i32;
        let new_y = bottom - height as i32;

        // Find the monitor the pill is currently on and center horizontally on it
        let center_x = old_pos.x + old_size.width as i32 / 2;
        let new_x = if let Ok(monitors) = w.available_monitors() {
            monitors.iter()
                .find(|m| {
                    let mx = m.position().x;
                    let mw = m.size().width as i32;
                    center_x >= mx && center_x < mx + mw
                })
                .map(|m| {
                    // Absolute center on this monitor
                    m.position().x + (m.size().width as i32 - width as i32) / 2
                })
                .unwrap_or_else(|| old_pos.x + (old_size.width as i32 - width as i32) / 2)
        } else {
            old_pos.x + (old_size.width as i32 - width as i32) / 2
        };

        let size_changed = old_size.width != width || old_size.height != height;
        let pos_changed = old_pos.x != new_x || old_pos.y != new_y;
        if size_changed {
            w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width, height)))
                .map_err(|e| e.to_string())?;
        }
        if pos_changed {
            w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, new_y)))
                .map_err(|e| e.to_string())?;
        }
        // The frame change has been applied (sync command on the main thread);
        // now make the window server drop the old composite — see
        // refresh_ns_window. Skipped-geometry calls skip this too: no frame
        // change, nothing new for the window server to go stale on (the
        // no-resize state transitions are covered by refresh_float_window).
        #[cfg(target_os = "macos")]
        if size_changed || pos_changed {
            refresh_ns_window(&w);
        }
        // Linux/GTK analog: after a programmatic resize/move an X11 or
        // Wayland compositor can keep compositing the pill's PREVIOUS frame
        // until the widget gets a natural draw pass — the same ghost the
        // macOS path fixes. Force a full-surface redraw. See refresh_gtk_window.
        #[cfg(target_os = "linux")]
        if size_changed || pos_changed {
            refresh_gtk_window(&w);
        }
    }
    Ok(())
}

/// Make the window server drop a transparent window's stale composite:
/// recompute the NSWindow shadow from current content and force a display pass.
///
/// On macOS, a `transparent:true` borderless NSWindow that is programmatically
/// resized or moved can keep its PREVIOUS backing store / shadow composited on
/// screen until the window receives a natural display pass — which is why a
/// click (activation → display) used to repair the float pill's ghost. That
/// ghost is two superimposed window frames (e.g. the 110px processing frame
/// under/over the 90px idle frame); no in-webview repaint can fix it because
/// the stale layer lives below the webview, at the window-server level. The
/// canonical AppKit remedy is `[window invalidateShadow]` plus forcing a
/// display pass (`displayIfNeeded`).
///
/// NSWindow methods must run on the main thread. Sync Tauri commands already
/// execute there (and `run_on_main_thread` then runs the closure inline, so
/// ordering is preserved), but routing through it keeps this helper correct
/// when called from any other thread.
#[cfg(target_os = "macos")]
pub(crate) fn refresh_ns_window(w: &tauri::WebviewWindow) {
    let win = w.clone();
    let _ = w.run_on_main_thread(move || {
        let Ok(ptr) = win.ns_window() else { return };
        if ptr.is_null() {
            return;
        }
        let ns_window = ptr as *mut objc2::runtime::AnyObject;
        // SAFETY: `ptr` is the live NSWindow backing `win` (the captured
        // handle keeps it alive for the closure's duration) and we are on
        // the main thread. Both selectors take no arguments and return void.
        unsafe {
            let _: () = objc2::msg_send![ns_window, invalidateShadow];
            let _: () = objc2::msg_send![ns_window, displayIfNeeded];
        }
    });
}

/// Linux/GTK analog of [`refresh_ns_window`]: force the float pill's toplevel
/// GTK window to re-run its size allocation and repaint its full surface, so an
/// X11 or Wayland compositor drops any stale frame it kept after a programmatic
/// resize/move.
///
/// On Linux the transparent, borderless webview is a GTK `ApplicationWindow`
/// (`WebviewWindow::gtk_window()`). After `set_size`/`set_position` the
/// compositor can keep compositing the window's PREVIOUS backing surface at the
/// old geometry until the widget gets a natural draw pass — the same "two
/// superimposed pill frames" ghost the macOS shadow-invalidation path fixes.
/// `queue_resize()` re-negotiates the allocation for the new geometry and
/// `queue_draw()` marks the whole widget (and its child webview) dirty, forcing
/// GTK to re-rasterize the full surface on the next frame.
///
/// GTK objects are `!Send` and must be touched only on the main thread, so the
/// `gtk::ApplicationWindow` is fetched INSIDE `run_on_main_thread` — only the
/// `Send` `WebviewWindow` handle crosses into the closure. Sync Tauri commands
/// already execute on the main thread, where the closure then runs inline, so
/// ordering relative to the preceding resize is preserved.
#[cfg(target_os = "linux")]
pub(crate) fn refresh_gtk_window(w: &tauri::WebviewWindow) {
    use gtk::prelude::WidgetExt;
    let win = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(gtk_window) = win.gtk_window() {
            gtk_window.queue_resize();
            gtk_window.queue_draw();
        }
    });
}

/// Fire-and-forget window-server refresh for the float pill.
///
/// float.html calls this after its repaint-guard flash settles on every state
/// transition. It matters for transitions that do NOT change window geometry
/// (success→idle at equal 90px widths, defensive idle reverts): those skip
/// `resize_float`'s native call entirely — and with it the shadow invalidation
/// above — yet the window server may still hold a stale composite from an
/// earlier frame. Resizing transitions get the invalidation twice (at frame
/// change and at settle); both are imperceptible no-ops when nothing is stale.
/// No-op on Windows; the macOS and Linux paths force a display pass on their
/// respective native windows (see [`refresh_ns_window`] / [`refresh_gtk_window`]).
#[tauri::command]
pub fn refresh_float_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(_w) = app.get_webview_window("float") {
        #[cfg(target_os = "macos")]
        refresh_ns_window(&_w);
        #[cfg(target_os = "linux")]
        refresh_gtk_window(&_w);
    }
    Ok(())
}

/// The mouse cursor's current location (logical coords) and the monitor
/// containing it. Falls back to the first monitor when the cursor is
/// off every known display; `None` when no monitors are reported.
///
/// Lives here (rather than in `main.rs`, where the sibling
/// `move_*_panel_to_cursor` helpers used to be defined) so it — and
/// [`move_panel_to_cursor`] below — are reachable via `super::`
/// from `commands::text_action`. `main.rs` and `lib.rs` each declare their
/// own independent `mod commands;`/`pub mod commands;` (the latter purely to
/// re-export commands for integration tests), so a `crate::`-rooted item
/// defined only in `main.rs` is invisible when this file is compiled as part
/// of the `lib.rs` crate root — anything under `commands/` avoids that split
/// since both roots include this exact module tree identically.
#[cfg(target_os = "macos")]
pub(crate) fn monitor_under_cursor(
    panel: &tauri::WebviewWindow,
) -> Option<(tauri::Monitor, core_graphics::geometry::CGPoint)> {
    let monitors = match panel.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return None,
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
    }).unwrap_or(&monitors[0]).clone();

    Some((target, cursor))
}

/// Anchor strategy for [`move_panel_to_cursor`]. The task-14 cleanup brief
/// named this `Cursor | BottomRight`, but the agent panel's original
/// placement (top-center, not offset from the cursor at all) is a genuinely
/// third geometry — folding it into either existing variant would change its
/// on-screen position, which the brief also requires to stay byte-identical.
/// So this is `Cursor | TopCenter | BottomRight`, one variant per distinct
/// placement strategy the five original `move_*_panel_to_cursor` functions
/// implemented. `TopRight` (Call Panel UX Pass) is a fourth, fixed-margin
/// corner geometry for the call panel's default position — distinct from
/// `BottomRight` in that both margins are fixed constants (no caller-supplied
/// `top_margin`) and the right edge keeps a small margin rather than sitting
/// flush.
#[cfg(target_os = "macos")]
pub(crate) enum PanelAnchor {
    /// Below-right of the exact cursor position, flipped to the opposite
    /// side when it would cross a monitor edge. Used by the text-action and
    /// dialog panels — `w`/`h` are the panel's actual current size (its
    /// `PanelSize` prop, not a fixed conf value).
    Cursor,
    /// Horizontally centered near the top of the monitor under the cursor,
    /// just below the macOS menu bar — doesn't otherwise use the cursor's
    /// exact position. `w` is the panel's fixed width; `h` is unused.
    TopCenter,
    /// Flush with the monitor's right edge, `top_margin` down from the top —
    /// a fixed corner so the panel doesn't obscure whatever app it's
    /// annotating. `w` is the panel's fixed width; `h` is unused.
    BottomRight { top_margin: f64 },
    /// Fixed top-right corner: ~40px down from the top (clear of the macOS
    /// menu bar, matching `TopCenter`'s 32px margin plus a little breathing
    /// room) and ~16px in from the right edge. Used by the call panel, which
    /// defaults to a corner rather than the cursor so it doesn't cover
    /// whatever the user is doing while a call runs. `w` is the panel's fixed
    /// width; `h` is unused.
    TopRight,
}

/// Position a satellite panel window relative to the monitor under the
/// cursor. Parameterized on the window `label`, its current `(w, h)`
/// (logical px — meaningless for the two fixed-geometry anchors, which still
/// take `w` as their fixed width so callers don't need a separate constant),
/// and an [`PanelAnchor`] picking which of the three placement strategies to
/// use. Replaces five near-identical `move_{text_action,dialog,call,agent,
/// meeting}_panel_to_cursor` copies (Workbench P2 Task 14); every call site
/// now names its own window label and anchor directly instead of that being
/// implied by which same-shaped function it happened to call.
#[cfg(target_os = "macos")]
pub(crate) fn move_panel_to_cursor(app: &tauri::AppHandle, label: &str, w: u32, h: u32, anchor: PanelAnchor) {
    use tauri::Manager;
    let Some(panel) = app.get_webview_window(label) else { return };
    let Some((target, cursor)) = monitor_under_cursor(&panel) else { return };

    let scale = target.scale_factor();
    let (panel_w, panel_h) = (w as f64, h as f64);

    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;
    let mon_h = target.size().height as f64 / scale;

    let (x, y) = match anchor {
        PanelAnchor::Cursor => {
            let offset = 12.0_f64;
            // Below-right of the cursor; flip to the opposite side at monitor edges.
            let mut x = cursor.x + offset;
            let mut y = cursor.y + offset;
            if x + panel_w > mon_x + mon_w { x = cursor.x - panel_w - offset; }
            if y + panel_h > mon_y + mon_h { y = cursor.y - panel_h - offset; }
            // Never leave the monitor; keep clear of the macOS menu bar.
            x = x.max(mon_x);
            y = y.max(mon_y + 28.0);
            (x, y)
        }
        PanelAnchor::TopCenter => {
            // Top-center: drops down from the menu bar area like a water drop.
            let x = mon_x + (mon_w - panel_w) / 2.0;
            let y = mon_y + 32.0; // Just below the macOS menu bar (28pt)
            (x, y)
        }
        PanelAnchor::BottomRight { top_margin } => {
            // Right edge of panel flush with right edge of screen, near the top.
            let x = mon_x + mon_w - panel_w;
            let y = mon_y + top_margin;
            (x, y)
        }
        PanelAnchor::TopRight => {
            // Same flush-right-corner math as BottomRight, but with fixed
            // margins on both edges instead of flush-right + caller-supplied
            // top_margin — the call panel's default resting spot.
            let x = mon_x + mon_w - panel_w - 16.0;
            let y = mon_y + 40.0;
            (x, y)
        }
    };

    let _ = panel.set_position(tauri::PhysicalPosition::new(
        (x * scale) as i32,
        (y * scale) as i32,
    ));
}

/// Escape a string for safe interpolation inside a single-quoted JS string
/// literal (`recvXxx('...')`-style panel `eval()` calls). Shared across every
/// satellite panel bridge — originally `meeting.rs`'s own private helper,
/// moved here (final review wave, I2) so `agent_widget.rs` could reuse it
/// too instead of its own manual `replace('\\', ..).replace('\'', ..)` chain,
/// which missed embedded newlines: a multi-line error produced an
/// unterminated single-quoted JS string, leaving the panel stuck on its
/// "thinking" indicator forever.
pub(crate) fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "")
}

// Service resolution moved to fonos-core (issue #21); the unified
// ServiceConfig lives in fonos_core::llm. These wrappers only add the
// AppState config-lock handling.
pub use fonos_core::llm::ServiceConfig;
pub use fonos_core::services::service_from_profile as config_from_profile;

/// Get connection info for a service by reading the active model profile.
pub fn get_service_config(state: &AppState, service: &str) -> ServiceConfig {
    match state.config.lock() {
        Ok(config) => fonos_core::services::resolve_service(&config, service),
        Err(_) => fonos_core::services::resolve_service(&Default::default(), service),
    }
}

/// Get connection info for a specific model profile by its ID.
pub fn get_service_config_for_profile(state: &AppState, profile_id: &str) -> ServiceConfig {
    match state.config.lock() {
        Ok(config) => fonos_core::services::resolve_profile(&config, profile_id),
        Err(_) => fonos_core::services::resolve_profile(&Default::default(), profile_id),
    }
}

/// Shared application state.
pub struct AppState {
    pub audio_capture: Arc<Mutex<Option<AudioCapture>>>,
    pub audio_playback: Arc<Mutex<Option<AudioPlayback>>>,
    pub config: Arc<Mutex<AppConfig>>,
    pub db: Arc<Mutex<rusqlite::Connection>>,
    /// Mutable agent state: skill registry + conversation context.
    /// Uses `tokio::sync::Mutex` so the lock can be held across `.await` points
    /// in async Tauri commands.
    pub agent: Arc<tokio::sync::Mutex<agent::AgentState>>,
    /// Mutable meeting state: recording flag, active container ID, chunk counter.
    /// Uses `tokio::sync::Mutex` for async access in the chunk-transcription loop.
    pub meeting: Arc<tokio::sync::Mutex<meeting::MeetingState>>,
    /// Target notebook for note mode. Set by the note panel when user selects a notebook.
    /// None = Quick Note (no container). Some(id) = specific notebook.
    pub note_target: Arc<Mutex<Option<i64>>>,
    /// STS conversation memory (issue #24), reset when the app restarts.
    pub sts_session: Arc<tokio::sync::Mutex<fonos_core::sts::StsSession>>,
    /// Active Dialog follow-up session (session-type output). Set by
    /// [`dialog::DialogOutput`] when a dialog workflow delivers its first turn,
    /// and driven by [`dialog::dialog_send`] for follow-ups. Replaced when a new
    /// dialog workflow delivers; the prior Conversation container stays in
    /// history. A `tokio::sync::Mutex` because the guard is held across the
    /// `next_turn().await` in `dialog_send`.
    pub dialog_session: Arc<tokio::sync::Mutex<Option<dialog::ActiveDialog>>>,
    /// Whether a hands-free "call mode" loop is running (issue #24). The loop
    /// task polls this flag for cooperative cancellation; `call_stop` clears it.
    pub call_active: Arc<std::sync::atomic::AtomicBool>,
    /// The workflow component registry, built once in `main`'s `.setup()` (it
    /// needs an `AppHandle`, only available there) and shared by every workflow
    /// run and the settings CRUD commands. Built exactly once — `run_workflow`
    /// and `workflow_cfg` both borrow this instance rather than rebuilding.
    pub registry: Arc<fonos_core::workflow::registry::Registry>,
    /// Tray health-panel menu-item handles (onboarding P2). `None` until
    /// `tray::setup_tray` runs in `.setup()`; refreshed in place afterwards.
    pub tray_menu: Arc<Mutex<Option<crate::tray::TrayHandles>>>,
}

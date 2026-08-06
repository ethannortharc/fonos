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
pub mod float_geom;
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
pub use agent::{agent_process, agent_reset, agent_llm_provider, list_skills, toggle_skill, save_custom_skill, delete_custom_skill, test_skill};

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

/// Where the user *chose* to put the indicator: the center of its visible
/// mark, in logical points. `None` until [`seed_float_anchor`] runs at setup.
///
/// Distinct from [`FLOAT_ANCHOR`], which is where the indicator currently is.
/// Following the cursor onto another display computes a position from this home
/// value every time, never from the last computed one — chaining transplants is
/// lossy, because the clamp applied on a narrow display permanently shrinks the
/// offset, so hopping to a small screen and back would not return the indicator
/// to where the user put it. See [`float_geom::transplant_anchor`].
static FLOAT_HOME: std::sync::Mutex<Option<float_geom::Point>> = std::sync::Mutex::new(None);

/// Where the indicator is right now: [`FLOAT_HOME`] normally, or its transplant
/// onto whichever display the cursor last pulled it to.
///
/// This is the source of truth every resize positions from, rather than
/// re-deriving a center from the window's current rect each time. Error pills
/// are sized from their text and are routinely an odd number of points wide, so
/// a re-derived center would round by half a point and could walk the window
/// sideways across a long run of resizes — the drift the previous
/// implementation avoided by re-centering on the monitor, which is no longer an
/// option now that the user can drag the indicator somewhere they chose.
static FLOAT_ANCHOR: std::sync::Mutex<Option<float_geom::Point>> = std::sync::Mutex::new(None);

/// The logical rect `(w, h, x, y)` [`resize_float`] last asked the platform for.
///
/// De-duplication needs this as well as the platform's own report, because the
/// two disagree in opposite directions. tao queues the frame change on the main
/// dispatch queue, so `outer_size()`/`outer_position()` keep reporting the
/// PREVIOUS rect until that block runs: a request matching the old rect would
/// be dismissed as a no-op while the queued change is still on its way to
/// making it wrong (recording 122 → processing 116 → recording 122: the third
/// request arrives while tao still reports 122, is skipped, and the queued 116
/// lands with nothing behind it). Conversely, if something outside this app
/// resizes the window, our record is the stale one.
///
/// So a call is skipped only when BOTH agree the window is already there.
static LAST_REQUESTED: std::sync::Mutex<Option<(f64, f64, f64, f64)>> =
    std::sync::Mutex::new(None);

/// Set where the indicator is, without changing where it calls home. Used by
/// the follow-the-cursor path, whose moves are transient.
pub fn set_float_anchor(anchor: float_geom::Point) {
    if let Ok(mut a) = FLOAT_ANCHOR.lock() {
        *a = Some(anchor);
    }
}

/// Set both home and current position — the user picked this spot (a drag, a
/// reset, or the default at first launch).
pub fn set_float_home(anchor: float_geom::Point) {
    if let Ok(mut h) = FLOAT_HOME.lock() {
        *h = Some(anchor);
    }
    set_float_anchor(anchor);
}

/// The indicator's current position, if one has been established.
pub fn float_anchor() -> Option<float_geom::Point> {
    FLOAT_ANCHOR.lock().ok().and_then(|a| *a)
}

/// The position the user chose, if one has been established.
pub fn float_home() -> Option<float_geom::Point> {
    FLOAT_HOME.lock().ok().and_then(|h| *h)
}

/// Resize the float window, keeping the center of its visible mark pinned to
/// [`FLOAT_ANCHOR`] so it grows and shrinks symmetrically about the spot the
/// user sees it at.
///
/// Sizes are **logical points**, and everything downstream stays in points —
/// see the coordinate-space section of [`float_geom`]'s module docs for why
/// physical pixels are unusable here. (The front end used to convert with a
/// `devicePixelRatio` captured once at load, which additionally went stale the
/// moment a dragged indicator crossed onto a display with a different scale.)
///
/// No-op geometry is skipped: on a transparent NSWindow every set_size /
/// set_position forces the window server to re-composite the surface, so
/// repeated same-geometry calls (idle→idle reverts, defensive re-sizing)
/// would churn the compositor for nothing — and every such churn is another
/// chance to interleave a stale frame into the composite.
///
/// NOTE: this command must stay synchronous (non-async) so it runs on the main
/// thread. It is NOT, however, a frame barrier, despite what this comment used
/// to claim: tao 0.34.6 implements both setters via
/// `DispatchQueue::main().exec_async` (`platform_impl/macos/util/async.rs`), so
/// the frame change is *queued* and runs when the main thread next returns to
/// its run loop — after this command has returned and its promise resolved.
/// float.html's grow path therefore waits two animation frames after the
/// promise before painting the larger content; see `applyVisual`.
///
/// `anchor_offset_y` is how far below the window's top edge the anchor point
/// sits; `None` means "half the height", i.e. centered. The front end passes it
/// explicitly when the mark is not in the middle of the window — see
/// [`float_geom::top_left_for_offset`].
#[tauri::command]
pub fn resize_float(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
    anchor_offset_y: Option<f64>,
) -> Result<(), String> {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("float") else { return Ok(()) };

    let size = (width.max(1.0), height.max(1.0));
    let scale = window_scale(&w);
    let reported = w.outer_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    let old_pos = w.outer_position().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    // What we last asked for, defaulting to the platform's report before the
    // first request — see LAST_REQUESTED.
    let last_req = LAST_REQUESTED
        .lock()
        .map_err(|e| e.to_string())?
        .unwrap_or((reported.width, reported.height, old_pos.x, old_pos.y));
    let old_size = tauri::LogicalSize::new(last_req.0, last_req.1);

    // An anchor should always exist by the time the front end resizes anything
    // (setup seeds it before the window is shown). If one somehow does not,
    // adopt the window's current center so this resize is still stable rather
    // than jumping to a corner.
    let anchor = float_anchor().unwrap_or_else(|| {
        let c = float_geom::center_of((old_pos.x, old_pos.y), (old_size.width, old_size.height));
        set_float_home(c);
        c
    });

    let offset_y = anchor_offset_y.unwrap_or(size.1 / 2.0);
    let raw = float_geom::top_left_for_offset(anchor, size.0, offset_y);
    // Clamp the *rect*, not the anchor. The anchor stays exactly where the user
    // dropped it — but a hairline legally parked 22pt from the left edge would
    // put the 122pt recording pill at x = −39 and the 168pt workflow menu at
    // x = −62. Those transient larger states get nudged back on screen; the
    // small idle shape returns to the untouched anchor afterwards.
    let pos = match indicator_monitor(&w, Some(anchor)) {
        Some((mon, _)) => float_geom::clamp_top_left(raw, size, mon),
        None => raw,
    };

    // Round to whole points before comparing, so "did anything change" is not
    // decided by sub-point noise that the platform would quantize away anyway.
    let (nw, nh) = (size.0.round(), size.1.round());
    let (nx, ny) = (pos.0.round(), pos.1.round());
    // Issue unless BOTH our record and the platform's report already say the
    // window is there — see LAST_REQUESTED.
    let size_changed = old_size.width.round() != nw
        || old_size.height.round() != nh
        || reported.width.round() != nw
        || reported.height.round() != nh;
    let pos_changed = last_req.2.round() != nx
        || last_req.3.round() != ny
        || old_pos.x.round() != nx
        || old_pos.y.round() != ny;

    if size_changed {
        w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(nw, nh)))
            .map_err(|e| e.to_string())?;
    }
    if let Ok(mut last) = LAST_REQUESTED.lock() {
        *last = Some((nw, nh, nx, ny));
    }
    if pos_changed {
        w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(nx, ny)))
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
    // Deferred by one run-loop turn on purpose. tao queues `setFrameOrigin` /
    // `setContentSize` onto the main dispatch queue, so at the moment a caller
    // returns from `set_position`/`set_size` the frame has NOT changed yet.
    // Invalidating the shadow and forcing a display pass right then would
    // recompute both from the OLD frame and leave exactly the ghost this is
    // meant to clear. Sleeping briefly lets the queued frame change run first.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        let _ = win.clone().run_on_main_thread(move || {
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

// ── Float indicator: shape, placement, persistence ──────────────────────────

/// A snapshot of the *live* config — the in-memory copy the rest of the app
/// mutates, not a fresh disk read.
///
/// The distinction matters for writes (see [`update_config`]) and, for reads,
/// keeps the indicator from acting on a stale file if anything ever changes
/// config in memory without flushing. Falls back to disk when the managed state
/// is not registered yet, which is the case during early `setup`.
pub(crate) fn current_config(app: &tauri::AppHandle) -> AppConfig {
    use tauri::Manager;
    app.try_state::<AppState>()
        .and_then(|s| s.config.lock().ok().map(|g| g.clone()))
        .unwrap_or_else(AppConfig::load)
}

/// Mutate the config and persist it, updating the in-memory copy *and* disk.
///
/// Writing straight to disk is not enough and quietly loses the change:
/// `save_config` builds every save by serializing the in-memory config and
/// merging the incoming patch onto it, so a disk-only write is overwritten by
/// the next settings change the user makes. That is exactly how a dragged
/// indicator position would silently snap back.
pub(crate) fn update_config(app: &tauri::AppHandle, f: impl FnOnce(&mut AppConfig)) -> Result<(), String> {
    use tauri::Manager;
    if let Some(state) = app.try_state::<AppState>() {
        let mut guard = state.config.lock().map_err(|e| e.to_string())?;
        f(&mut guard);
        return guard.save().map_err(|e| e.to_string());
    }
    let mut cfg = AppConfig::load();
    f(&mut cfg);
    cfg.save().map_err(|e| e.to_string())
}

/// What `float.html` needs to lay itself out: which shape to render, how long
/// to wait before dimming, and the idle window size of every shape.
///
/// The size table ships whole rather than just the active shape's entry so a
/// live shape switch needs no round trip, and — more importantly — so
/// `float.html` never restates a dimension that [`float_geom`] owns.
#[derive(serde::Serialize)]
pub struct FloatGeometry {
    /// Normalized shape id (never an unknown value — see
    /// `fonos_core::config::normalize_float_indicator`).
    pub form: String,
    /// Seconds of idleness before dimming; `0` disables dimming.
    pub idle_fade_secs: u32,
    /// `[shape, logical_width, logical_height]` for every shape.
    pub sizes: Vec<(String, f64, f64)>,
    /// Logical height of the working-state pill.
    pub working_height: f64,
}

/// Read the indicator's layout inputs. Called by `float.html` at startup and
/// again whenever `config:saved` reports a change.
#[tauri::command]
pub fn float_geometry(app: tauri::AppHandle) -> FloatGeometry {
    let cfg = current_config(&app);
    FloatGeometry {
        form: cfg.float_form().to_string(),
        idle_fade_secs: cfg.float_idle_fade_secs,
        sizes: float_geom::all_idle_sizes()
            .iter()
            .map(|(n, w, h)| ((*n).to_string(), *w, *h))
            .collect(),
        working_height: float_geom::WORKING_H,
    }
}

/// Pick the display the indicator belongs on, returning its physical bounds and
/// scale factor.
///
/// Preference order: the monitor containing `near` (a remembered anchor, or the
/// indicator's current position), then the primary monitor (the one at the
/// coordinate-space origin), then whatever the platform listed first. `None`
/// only when no monitors are reported at all, in which case there is nothing
/// meaningful to place against.
///
/// Bounds come back in **logical points**: each monitor's reported rect is
/// divided by *its own* scale factor, which is what makes the rects mutually
/// consistent on a mixed-DPI desktop. See [`float_geom`]'s module docs — the
/// "physical" rects tao reports can literally overlap, so a point could resolve
/// to the wrong display.
pub(crate) fn indicator_monitor(
    w: &tauri::WebviewWindow,
    near: Option<float_geom::Point>,
) -> Option<(float_geom::Rect, f64)> {
    let monitors = w.available_monitors().ok()?;
    let rects: Vec<float_geom::Rect> = monitors
        .iter()
        .map(|m| {
            let s = if m.scale_factor() > 0.0 && m.scale_factor().is_finite() {
                m.scale_factor()
            } else {
                1.0
            };
            (
                m.position().x as f64 / s,
                m.position().y as f64 / s,
                m.size().width as f64 / s,
                m.size().height as f64 / s,
            )
        })
        .collect();
    let idx = near
        .and_then(|(x, y)| float_geom::rect_index_containing(&rects, x, y))
        .or_else(|| rects.iter().position(|(x, y, _, _)| *x == 0.0 && *y == 0.0))
        .or(if rects.is_empty() { None } else { Some(0) })?;
    Some((rects[idx], monitors[idx].scale_factor()))
}

/// A window's scale factor, falling back to 1.0 when the platform declines to
/// report one (or reports something nonsensical). Never returns 0 or NaN, so
/// callers can divide by it without stranding the window at an infinite
/// coordinate.
fn window_scale(w: &tauri::WebviewWindow) -> f64 {
    match w.scale_factor() {
        Ok(s) if s > 0.0 && s.is_finite() => s,
        _ => 1.0,
    }
}

/// The float window's current rect in logical points.
fn float_rect(w: &tauri::WebviewWindow) -> Option<(float_geom::Point, float_geom::Size)> {
    let scale = window_scale(w);
    let pos = w.outer_position().ok()?.to_logical::<f64>(scale);
    let size = w.outer_size().ok()?.to_logical::<f64>(scale);
    Some(((pos.x, pos.y), (size.width, size.height)))
}

/// Establish the indicator's home position at startup: the spot the user
/// dragged it to if there is one, otherwise the default on the primary display.
///
/// A remembered position is clamped into whichever live monitor contains it, so
/// one inherited from a disconnected or higher-resolution display can never
/// strand the indicator off screen with no way to drag it back.
pub fn seed_float_anchor(
    w: &tauri::WebviewWindow,
    form: &str,
    remembered: Option<float_geom::Point>,
) {
    let Some((mon, _)) = indicator_monitor(w, remembered) else { return };
    // A remembered position only survives if it is still ON a display. Clamping
    // one that is not — a position saved on a monitor since unplugged — drags
    // it to the nearest edge of whatever display is left, which is a worse
    // answer than the default spot and is what the user would see on the first
    // launch after disconnecting an external screen.
    let on_screen = remembered.is_some_and(|a| {
        float_geom::rect_index_containing(&[mon], a.0, a.1).is_some()
    });
    let anchor = match remembered {
        // Preserved verbatim when nothing shows at idle: `idle_size("off")` is
        // only a stand-in (the pill's), and clamping against it would walk the
        // saved position inward — a hairline parked at x=22 comes back at x=49
        // after a round trip through Off. Whatever DOES appear (a working pill)
        // is kept on screen by resize_float's own rect clamp.
        Some(a) if on_screen && !float_geom::shows_when_idle(form) => a,
        Some(a) if on_screen => float_geom::clamp_anchor(a, float_geom::idle_size(form), mon),
        _ => float_geom::default_anchor(mon),
    };
    set_float_home(anchor);
}

/// Show or hide the indicator for `form`, without touching its geometry.
///
/// Deliberately does NOT resize. A shape change can arrive while the pill is
/// mid-recording, and resizing to the *idle* size from here would clip the
/// working pane and — worse — desynchronize float.html's `curW`/`curH` from the
/// real window, since that resize never went through the front end's protected
/// transition path. The front end applies the new shape's geometry itself, on
/// its next return to idle. See `float.html`'s `applyVisual`.
/// Startup-only visibility. Once float.html is alive IT owns this — visibility
/// depends on the state as much as the shape (`"off"` still shows a dictation
/// in flight), and only the front end knows the state. Two deciders raced.
pub fn set_float_visible(app: &tauri::AppHandle, form: &str) {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("float") else { return };
    if !float_geom::shows_when_idle(form) {
        let _ = w.hide();
        return;
    }
    // Becoming visible is NOT done here. While the shape was "off" the front
    // end skipped every resize, so the native window is still whatever size it
    // last had — showing it now would flash a working pane clipped into a
    // smaller frame while the front end's own resize makes its IPC round trip.
    // The front end calls `show_float` once its geometry has landed.
    //
    // Safety net: if that call never arrives (a wedged webview), show it anyway
    // rather than leave the user with an indicator they turned on and cannot
    // see — and no way back, since its settings live in the main window.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        // Re-read the form: the user may have switched back to "off" while this
        // was pending, and showing on `is_visible()` alone would resurrect an
        // indicator they just turned off — with Settings still reading Off.
        if !float_geom::shows_when_idle(current_config(&app).float_form()) {
            return;
        }
        let inner = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = inner.get_webview_window("float") {
                if !w.is_visible().unwrap_or(false) {
                    let _ = w.show();
                }
            }
        });
    });
}

/// Show the float window. Called by the front end once it has sized itself —
/// on a shape change, and whenever a working state begins while the shape is
/// `"off"`. See [`set_float_visible`].
#[tauri::command]
pub fn show_float(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("float") {
        let _ = w.show();
    }
    Ok(())
}

/// Hide the float window. The front end calls this when an `"off"` indicator
/// returns to idle, so the pill is on screen for exactly as long as it has
/// something to report.
#[tauri::command]
pub fn hide_float(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("float") {
        let _ = w.hide();
    }
    Ok(())
}

/// Place the indicator at its anchor in `form`'s idle geometry, then show it.
///
/// Startup only — this is the one moment a native resize is safe without the
/// front end's involvement, because no working state can be in flight yet, and
/// it is what stops a frame of the wrong-sized window reaching the screen
/// before float.html has painted.
pub fn place_and_show_float(app: &tauri::AppHandle, form: &str) {
    use tauri::Manager;
    if !float_geom::shows_when_idle(form) {
        set_float_visible(app, form);
        return;
    }
    let (lw, lh) = float_geom::idle_size(form);
    // Via resize_float so the placement rides the same clamp + stale-composite
    // invalidation as every other resize.
    let _ = resize_float(app.clone(), lw, lh, None);
    // Shown right here, not left to the front end. Startup is the one moment
    // there is no state to consult — nothing can be recording — so the shape
    // alone decides, and the geometry it needs was just applied on the line
    // above. Deferring it meant the indicator's first appearance depended on
    // the webview completing a round trip, which is a lot of machinery to hang
    // "does the app look alive" on.
    if let Some(w) = app.get_webview_window("float") {
        let _ = w.show();
    }
}

/// Move the indicator by a pointer delta, in logical points.
///
/// The drag is driven from the front end (pointer capture in `float.html`)
/// rather than handed to the platform's own window-drag loop. That loop
/// swallows the mouseup, leaving the webview to *infer* when the gesture ended
/// — and every way of inferring it was wrong in a different way: a debounce on
/// window-move events fires during a mid-drag pause, and it cannot tell a
/// user's drag from the window moves this app makes itself (following the
/// cursor to another display, or clamping a bigger state back on screen), so a
/// programmatic move could be persisted as the user's chosen position. Owning
/// the gesture makes its end unambiguous: it is the pointerup.
///
/// Memory only — nothing is written to disk until [`commit_float_anchor`], so a
/// drag is one config write rather than one per frame.
#[tauri::command]
pub fn nudge_float(app: tauri::AppHandle, dx: f64, dy: f64) -> Result<(), String> {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("float") else { return Ok(()) };
    // Base the delta on where the indicator actually IS, not on where the user
    // last chose to put it. Following the cursor to another display moves the
    // current anchor without touching home, so starting from home would
    // teleport the indicator back to its old display on the first pointer move.
    let Some(from) = float_anchor() else { return Ok(()) };
    let Some((_, size)) = float_rect(&w) else { return Ok(()) };

    let moved = (from.0 + dx, from.1 + dy);
    // Clamp only when the anchor has left every display. Clamping into the
    // monitor under it on each step — as this first did — makes crossing
    // between displays impossible: at the edge, every small delta is pulled
    // straight back inside before the anchor can reach the neighbour, so the
    // indicator sticks half a window-width from the boundary. Letting the
    // anchor travel freely while it is over *some* display, and catching it
    // only when it is over none, allows the crossing and still keeps the
    // indicator on the desktop. The full-window fit is enforced at the end of
    // the gesture by `commit_float_anchor`.
    // Over a display: free travel. Over none (the desktop can have gaps between
    // monitors, and the anchor carries the pointer's grab offset, so it can be
    // in a gap while the pointer is already on the next display): keep the
    // anchor where the drag put it rather than snapping it back to the display
    // it came from. Snapping strands the gesture — the front end has already
    // advanced its delta origin, so every later delta is measured from a place
    // the indicator is not, and with a wide enough gap it can never catch up.
    // The window is pulled fully on-screen at the end of the gesture by
    // `commit_float_anchor`, which is what makes this safe.
    let next = moved;
    // The user is choosing this spot, so it becomes home as well as current.
    set_float_home(next);

    let (x, y) = float_geom::top_left_for(next, size);
    w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
        x.round(),
        y.round(),
    )))
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Persist the indicator's current home position — called once, when a drag
/// ends. Separate from [`nudge_float`] so the gesture costs a single write.
#[tauri::command]
pub fn commit_float_anchor(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let Some(mut home) = float_home() else { return Ok(()) };

    // The gesture-end clamp. `nudge_float` deliberately lets the anchor roam
    // anywhere over a display so the indicator can cross between monitors, but
    // that admits anchors less than half a window from an edge — dropping there
    // would leave half the indicator off-screen, and persist that. Pull the
    // whole window on-screen now that the gesture is over.
    if let Some(w) = app.get_webview_window("float") {
        if let Some((_, size)) = float_rect(&w) {
            if let Some((mon, _)) = indicator_monitor(&w, Some(home)) {
                let clamped = float_geom::clamp_anchor(home, size, mon);
                if !same_point(clamped, home) {
                    home = clamped;
                    set_float_home(home);
                    let (x, y) = float_geom::top_left_for(home, size);
                    // Propagate a failure here: this move is what rescues an
                    // indicator dropped off-screen, and silently persisting the
                    // clamped anchor while the window stayed unreachable would
                    // report success for a state the user cannot recover from.
                    w.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                        x.round(),
                        y.round(),
                    )))
                    .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if current_config(&app).float_anchor().is_some_and(|a| same_point(a, home)) {
        return Ok(()); // already on disk — a drag that ended where it started
    }
    update_config(&app, |cfg| {
        cfg.float_anchor_x = Some(home.0);
        cfg.float_anchor_y = Some(home.1);
    })
}

/// Two positions that land on the same whole point. Anchors are `f64`, so exact
/// equality would make the "nothing moved" short-circuit above miss on noise
/// the platform would quantize away anyway.
fn same_point(a: float_geom::Point, b: float_geom::Point) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

/// Forget a dragged position and return the indicator to the default spot on
/// the monitor it is currently on. Backs the Settings "reset position" button.
#[tauri::command]
pub fn reset_float_anchor(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    update_config(&app, |cfg| {
        cfg.float_anchor_x = None;
        cfg.float_anchor_y = None;
    })?;
    let form = current_config(&app).float_form();

    if let Some(w) = app.get_webview_window("float") {
        // "Default position" means default *on the display it is on*, not back
        // on the primary — someone working on a second monitor who nudged the
        // indicator askew wants it straightened there, not teleported. So the
        // monitor is chosen from where it currently sits, which is why this
        // does not just call seed_float_anchor (that one has no anchor left to
        // look up, and would fall through to primary).
        let here = float_anchor().or_else(|| float_rect(&w).map(|(p, s)| float_geom::center_of(p, s)));
        if let Some((mon, _)) = indicator_monitor(&w, here) {
            set_float_home(float_geom::default_anchor(mon));
        }
        // Safe to resize here even mid-recording: the button lives in the main
        // window's settings, and re-placing is the entire point of pressing it.
        // The size passed is the CURRENT window's, so only the position moves —
        // float.html's curW/curH stay accurate.
        if let Some((_, size)) = float_rect(&w) {
            resize_float(app.clone(), size.0, size.1, None)?;
        }
        set_float_visible(&app, form);
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

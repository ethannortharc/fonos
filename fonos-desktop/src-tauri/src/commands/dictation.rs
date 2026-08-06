//! Tauri commands for voice dictation.
//!
//! Simple flow: record locally → save WAV → HTTP POST → get transcript.
//! No WebSocket streaming — avoids model contention and is faster.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::audio::capture::AudioCapture;
use crate::injection::inject_text;
use fonos_core::workflow::engine::effective_widgets;
use fonos_core::workflow::llm_step::LlmProps;
use fonos_core::workflow::model::WidgetDef;
use tauri::{Emitter, Manager};

use super::workflow_widgets::SttProps;
use super::AppState;

/// Resolve the [`LlmProps`] of the `llm.{mode_id}` processor widget backing a
/// legacy dictation-mode id (`config.dictation_mode`, or a `mode_override`
/// carrying one), if any — the engine-world replacement for the deleted
/// `modes::all_modes()` "does this mode have LLM content" lookup (Workbench
/// P2 Task 12). Mirrors [`crate::workflow::migrate::mode_resolves_to_llm_widget`]'s
/// rule 2 (`"raw"` never has an LLM step) without needing that private
/// function: any other id either resolves to a real widget with a prompt, or
/// doesn't — an unresolved id (including one with no matching widget at all)
/// is treated as content-less, matching `raw`'s behavior.
pub(crate) fn dictation_mode_llm_props(widgets: &[WidgetDef], mode_id: &str) -> Option<LlmProps> {
    if mode_id.is_empty() || mode_id == "raw" {
        return None;
    }
    let widget_id = format!("llm.{mode_id}");
    let props: LlmProps = widgets
        .iter()
        .find(|w| w.id == widget_id)
        .and_then(|w| serde_json::from_value(w.props.clone()).ok())?;
    if props.system.is_some() || props.user_template.is_some() {
        Some(props)
    } else {
        None
    }
}

/// Resolve the built-in `stt.default` widget's [`SttProps`] from `widgets`
/// ([`effective_widgets`]'s result). Falls back to
/// [`SttProps`]'s serde defaults if the widget is missing (shouldn't happen —
/// built-ins are never deletable) or its props fail to deserialize.
fn resolve_stt_default_props(widgets: &[WidgetDef]) -> SttProps {
    widgets
        .iter()
        .find(|w| w.id == "stt.default")
        .and_then(|w| serde_json::from_value(w.props.clone()).ok())
        .unwrap_or_else(|| serde_json::from_value(serde_json::json!({})).unwrap())
}

/// Prevents duplicate start/stop calls from rapid hotkey events.
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

/// Check if currently recording.
pub fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::SeqCst)
}

/// Force-reset the recording flag. Called when hiding panels to prevent
/// a stale IS_RECORDING=true from blocking future recording sessions.
pub fn force_reset_recording() {
    let was_recording = IS_RECORDING.swap(false, Ordering::SeqCst);
    if was_recording {
        eprintln!("fonos: force-reset IS_RECORDING (was stuck at true)");
    }
}

/// Place the float indicator at its startup position and show it in the shape
/// the config asks for (or leave it hidden, for `"off"`).
///
/// All the geometry lives in [`super::float_geom`]; this is just the setup-time
/// entry point. Cross-platform: the placement math no longer needs a per-OS
/// branch, only the clearance constant does.
pub fn move_float_to_primary_pub(app: &tauri::AppHandle) {
    let Some(float_win) = app.get_webview_window("float") else { return };
    let cfg = super::current_config(app);
    super::seed_float_anchor(&float_win, cfg.float_form(), cfg.float_anchor());
    super::place_and_show_float(app, cfg.float_form());
}

/// When a recording starts, bring the indicator to the display the cursor is
/// on — but only when it is not already there.
///
/// The "only when not already there" guard is what makes a dragged position
/// survive: the previous implementation re-ran bottom-center placement on
/// *every* recording start, which would now silently undo the user's drag the
/// first time they spoke.
///
/// Crossing displays carries the user's intent along — the offset they dragged
/// the indicator to, relative to that display's default spot, is reapplied on
/// the new one. That transplant always starts from the **home** anchor, never
/// from the current (possibly already-transplanted) one: chaining is lossy,
/// because the clamp applied on a narrow display permanently shrinks the
/// offset, so hopping to a small screen and back would strand the indicator
/// somewhere the user never put it.
///
/// macOS-only, matching [`super::monitor_under_cursor`]; on other platforms the
/// indicator simply stays where it is, which is also what a user who placed it
/// deliberately would expect.
#[cfg(target_os = "macos")]
pub(crate) fn follow_cursor_to_monitor(app: &tauri::AppHandle) {
    use super::float_geom;
    let Some(float_win) = app.get_webview_window("float") else { return };
    let Some(home) = super::float_home() else { return };
    let Some(current) = super::float_anchor() else { return };
    let Some((mon, _cursor)) = super::monitor_under_cursor(&float_win) else { return };

    // The cursor's display, in the same logical space as every other rect here.
    let scale = if mon.scale_factor() > 0.0 && mon.scale_factor().is_finite() {
        mon.scale_factor()
    } else {
        1.0
    };
    let target: float_geom::Rect = (
        mon.position().x as f64 / scale,
        mon.position().y as f64 / scale,
        mon.size().width as f64 / scale,
        mon.size().height as f64 / scale,
    );
    if float_geom::rect_index_containing(&[target], current.0, current.1).is_some() {
        return; // already on the cursor's display — nothing to do
    }

    // Measure the user's offset against the default of the display they chose
    // the position ON, not wherever the indicator happens to be sitting now.
    let new_default = float_geom::default_anchor(target);
    let moved = match super::indicator_monitor(&float_win, Some(home)) {
        Some((home_mon, _)) => {
            float_geom::transplant_anchor(home, float_geom::default_anchor(home_mon), new_default)
        }
        None => new_default,
    };

    // Always clamped, unlike seed_float_anchor. That path PRESERVES a position
    // the user chose, so clamping it against a stand-in size would drift it;
    // this one RELOCATES the indicator onto the cursor's display, so an
    // unclamped transplant landing past that display's edge would strand the
    // pill on the previous screen (or, via indicator_monitor's fallback, the
    // primary) — with `off` there is no idle marker to hint at where it went.
    let form = super::current_config(app).float_form();
    let placed = float_geom::clamp_anchor(moved, float_geom::clamp_size(form), target);
    // Deliberately updates the current anchor only, never home: following the
    // cursor is transient and must not overwrite the position the user chose.
    // Only an explicit drag (`commit_float_anchor`) moves home.
    super::set_float_anchor(placed);
    reposition_float_to_anchor(&float_win);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn follow_cursor_to_monitor(_app: &tauri::AppHandle) {}

/// Move the float window so its current size is centered on the current anchor.
/// Used by the paths that change the anchor without changing the size — so
/// float.html's `curW`/`curH` stay accurate without it being told anything.
pub(crate) fn reposition_float_to_anchor(float_win: &tauri::WebviewWindow) {
    use super::float_geom;
    let Some(anchor) = super::float_anchor() else { return };
    let Some((_, size)) = super::float_rect(float_win) else { return };
    let raw = float_geom::top_left_for(anchor, size);
    // Clamp the RECT, exactly as resize_float does. The window may currently be
    // a working state far wider than the idle shape whose anchor was clamped,
    // so an anchor legal for a 44pt hairline can still put a 122pt recording
    // pill off the edge. Without this, that pill stays half off-screen until
    // its next state transition happens to re-run resize_float.
    let (x, y) = match super::indicator_monitor(float_win, Some(anchor)) {
        Some((mon, _)) => float_geom::clamp_top_left(raw, size, mon),
        None => raw,
    };
    let _ = float_win.set_position(tauri::LogicalPosition::new(x.round(), y.round()));
    // A programmatic move of a transparent NSWindow can leave the window
    // server compositing the old frame at the old spot until a display pass
    // happens — same ghost mechanism as resize_float; see refresh_ns_window.
    #[cfg(target_os = "macos")]
    super::refresh_ns_window(float_win);
    #[cfg(target_os = "linux")]
    super::refresh_gtk_window(float_win);
}

/// Re-place the float indicator after the display configuration changes
/// (external monitor connected/disconnected, resolution/arrangement change).
///
/// Rule: if the home position still falls inside a live monitor, keep it and
/// only clamp it back into that monitor's (possibly changed) bounds — a
/// resolution shift must not throw away a position the user chose. If its
/// display is gone, fall back to the default placement on the primary monitor,
/// same as first launch.
#[cfg(target_os = "macos")]
pub(crate) fn reposition_float_after_display_change(app: &tauri::AppHandle) {
    use super::float_geom;
    let Some(float_win) = app.get_webview_window("float") else { return };
    let Some(home) = super::float_home() else { return };

    let Some((mon, _)) = super::indicator_monitor(&float_win, Some(home)) else { return };
    // `indicator_monitor` falls back to the primary display when the point is
    // inside none of them, so "did its own display survive?" is exactly "is the
    // point inside the monitor it just handed back".
    let survived = float_geom::rect_index_containing(&[mon], home.0, home.1).is_some();

    let form = super::current_config(app).float_form();
    let next = if !survived {
        float_geom::default_anchor(mon)
    } else if float_geom::shows_when_idle(form) {
        float_geom::clamp_anchor(home, float_geom::idle_size(form), mon)
    } else {
        home // see seed_float_anchor: no idle window to fit
    };
    // Home moves too: the old coordinates refer to hardware that is gone (or to
    // bounds that no longer exist), so keeping them would make the next
    // follow-the-cursor transplant measure its offset against a phantom display.
    super::set_float_home(next);
    // Deliberately NOT persisted. `survived` only tells us whether the saved
    // coordinate still falls inside a live rect, which conflates "the display
    // was unplugged" with "the display's resolution shrank past it" — and
    // writing a default over the user's chosen position for the latter would
    // lose it permanently, even after they restore the resolution. The stale
    // saved anchor is instead recognized at startup, where
    // `seed_float_anchor` falls back to the default rather than clamping a
    // coordinate that belongs to hardware that is gone.
    reposition_float_to_anchor(&float_win);
}

/// Register a CoreGraphics display-reconfiguration callback so the float pill
/// re-places itself whenever displays are added/removed/rearranged. Call once,
/// at setup, from the main thread.
///
/// The callback is a plain `extern "C"` fn pointer (no `block2`/NSNotification
/// dependency — CoreGraphics is already linked). The `AppHandle` reaches the
/// callback through a `static OnceLock` rather than the `user_info` pointer:
/// `OnceLock` is set-once, needs no `Box::into_raw`/leak, and reads back with a
/// safe `.get()` — strictly simpler than round-tripping a raw pointer.
///
/// CoreGraphics fires this callback several times per user-visible change (once
/// with the Begin flag *before*, once *after*, per affected display), so we
/// debounce rather than filter flags: every fire bumps a generation counter and
/// schedules a reposition ~500ms later that only runs if it is still the latest
/// fire. Debouncing (over flag-filtering) coalesces the multi-fire storm into a
/// single reposition *and* defers it until the arrangement has settled, so
/// `available_monitors()` reports the final geometry, not a mid-transition one.
#[cfg(target_os = "macos")]
pub fn register_display_reconfig_callback(app: &tauri::AppHandle) {
    use core::ffi::c_void;
    use core_graphics::display::{CGDirectDisplayID, CGDisplayRegisterReconfigurationCallback};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;

    static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
    /// Debounce token: bumped on every callback; a scheduled reposition acts
    /// only if its captured value is still current when it wakes.
    static GEN: AtomicU64 = AtomicU64::new(0);

    // SAFETY: signature matches `CGDisplayReconfigurationCallBack` exactly.
    // Touches only a `static OnceLock` (read) and a `static AtomicU64`, then
    // hands off to the async runtime — no windows are touched on this thread.
    unsafe extern "C" fn on_reconfig(
        _display: CGDirectDisplayID,
        _flags: u32,
        _user_info: *const c_void,
    ) {
        let Some(app) = APP.get() else { return };
        let ticket = GEN.fetch_add(1, Ordering::SeqCst) + 1;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // Superseded by a later fire in the same reconfiguration burst.
            if GEN.load(Ordering::SeqCst) != ticket {
                return;
            }
            // Hop to the main thread before touching any window.
            let app_main = app.clone();
            let _ = app.run_on_main_thread(move || {
                reposition_float_after_display_change(&app_main);
            });
        });
    }

    if APP.set(app.clone()).is_err() {
        return; // already registered
    }
    // SAFETY: `on_reconfig` is a valid `extern "C"` fn of the exact callback
    // type; a null `user_info` is fine (the AppHandle travels via `APP`). The
    // OnceLock guard above makes registration run at most once.
    unsafe {
        let _ = CGDisplayRegisterReconfigurationCallback(on_reconfig, std::ptr::null());
    }
}

#[derive(Serialize)]
pub struct SttResult {
    pub text: String,
    pub audio_path: String,
    pub latency_ms: u64,
    pub duration_secs: f64,
    /// For Apple Speech: "on-device" or "server". Empty for HTTP providers.
    pub stt_engine: String,
    /// STT backend identifier used for this transcription (for latency stats).
    pub stt_model: String,
    /// Low-frequency noise removed by high-pass filter, as percentage of total energy.
    pub noise_removed_pct: f64,
    /// Normalization gain applied in dB (positive = amplified, 0 = no change).
    pub gain_db: f64,
}

/// Check if a microphone is available AND accessible (has permission).
#[tauri::command]
pub fn has_microphone() -> Result<bool, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => d,
        None => return Ok(false),
    };
    Ok(device.supported_input_configs().is_ok())
}

/// List all available audio input devices.
#[tauri::command]
pub fn list_audio_inputs() -> Result<Vec<String>, String> {
    Ok(crate::audio::capture::list_input_devices())
}

/// Warm-up debounce: at most one backend ping per interval.
static LAST_WARMUP: AtomicU64 = AtomicU64::new(0);
const WARMUP_INTERVAL_SECS: u64 = 180;

fn is_local_endpoint(base_url: &str) -> bool {
    base_url.contains("localhost") || base_url.contains("127.0.0.1") || base_url.contains("0.0.0.0")
}

/// Fire-and-forget warm-up of the configured STT (and, for LLM modes, LLM)
/// backend so the first capture after idle doesn't pay a model cold start.
/// The ping runs while the user is speaking, so by stop time the model is
/// loaded. Local endpoints only — cloud APIs don't cold-start and probes
/// would cost money. Debounced to once per WARMUP_INTERVAL_SECS.
fn spawn_backend_warmup(state: &tauri::State<'_, AppState>) {
    let (enabled, mode_name, widgets) = match state.config.lock() {
        Ok(cfg) => (cfg.warmup_enabled, cfg.dictation_mode.clone(), effective_widgets(&cfg)),
        Err(_) => return,
    };
    if !enabled {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(LAST_WARMUP.load(Ordering::Relaxed)) < WARMUP_INTERVAL_SECS {
        return;
    }
    LAST_WARMUP.store(now, Ordering::Relaxed);

    let stt = super::get_service_config(state, "stt");
    let llm = super::get_service_config(state, "llm");
    let mode_has_llm = dictation_mode_llm_props(&widgets, &mode_name).is_some();

    tauri::async_runtime::spawn(async move {
        if stt.provider != "apple" && !stt.base_url.trim().is_empty() && is_local_endpoint(&stt.base_url) {
            let model = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
            eprintln!("fonos: warm-up ping → STT {} ({})", stt.base_url, model);
            let bytes = silent_probe_wav();
            let _ = if stt.stt_api == "chat" {
                transcribe_chat(&stt, &bytes, "", &[]).await
            } else {
                transcribe_http(&stt, &bytes, &model, "", "", 0.0, &[]).await
            };
        }
        if mode_has_llm && !llm.base_url.trim().is_empty() && is_local_endpoint(&llm.base_url) {
            eprintln!("fonos: warm-up ping → LLM {} ({})", llm.base_url, llm.model);
            let base = llm.base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                format!("{base}/chat/completions")
            } else {
                format!("{base}/v1/chat/completions")
            };
            let client = reqwest::Client::new();
            let _ = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", llm.api_key))
                .json(&serde_json::json!({
                    "model": llm.model,
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 1,
                }))
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await;
        }
    });
}

/// A 0.3s silent 16kHz mono WAV, used as a warm-up / endpoint-probe clip.
fn silent_probe_wav() -> Vec<u8> {
    let sample_rate = 16000u32;
    let sample_count = (sample_rate as usize) * 3 / 10;
    let pcm = vec![0u8; sample_count * 2]; // i16 LE silence
    let dir = std::env::temp_dir().join("fonos_audio");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("stt_probe.wav");
    if fonos_core::audio::write_wav(&path, &pcm, sample_rate).is_ok() {
        std::fs::read(&path).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Start capturing audio from the microphone (local only, no network).
/// When `skip_float` is true, the float pill is not moved or activated (used by agent hotkey).
#[tauri::command]
pub async fn start_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, skip_float: Option<bool>) -> Result<(), String> {
    eprintln!("fonos: start_recording called, skip_float={:?}", skip_float);

    // Read selected device from config.
    let device_name = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.audio_input_device.clone()
    };

    // Check device availability up front. These probes read no shared state, so
    // they run outside the recording-state critical section below.
    use crate::audio::capture::find_input_device;
    match find_input_device(&device_name) {
        None => {
            return Err("No microphone found. Connect an audio input device.".into());
        }
        Some(dev) => {
            use cpal::traits::DeviceTrait;
            if dev.supported_input_configs().is_err() {
                return Err("Microphone permission denied. Grant access in System Settings > Privacy > Microphone.".into());
            }
        }
    }

    // Funnel: the microphone is provably usable from here on (record-once).
    if let Ok(db) = state.db.lock() {
        let _ = fonos_core::funnel::record(&db, "mic_granted");
    }

    // Preload the backends while the user speaks (issue #4 warm-up).
    spawn_backend_warmup(&state);

    // Critical section: hold the capture lock while we both check the recording
    // flag and bring the stream up. `stop_recording` flips the same flag under
    // the same lock, so the two can't interleave — this prevents a fast tap
    // (down then up) from draining an empty capture and then leaving the mic
    // live with IS_RECORDING=false. The flag is set to true only *after* the
    // stream is running, so the invariant "IS_RECORDING == there is a live
    // capture" always holds.
    let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;

    if IS_RECORDING.load(Ordering::SeqCst) {
        eprintln!("fonos: start_recording ignored — already recording");
        return Ok(()); // Already recording — ignore duplicate
    }

    // Always create a fresh AudioCapture — the old one may reference a
    // disconnected device or the user may have changed the device setting.
    *guard = None;
    let capture = AudioCapture::with_device(&device_name)
        .map_err(|e| format!("mic init failed: {e}"))?;
    *guard = Some(capture);
    guard.as_mut().unwrap().start()
        .map_err(|e| format!("mic start failed: {e}"))?;

    // Stream is live — now mark recording, still holding the lock.
    IS_RECORDING.store(true, Ordering::SeqCst);
    drop(guard);

    // Move float pill to the monitor where the cursor is (skip for agent mode)
    if !skip_float.unwrap_or(false) {
        follow_cursor_to_monitor(&app);
        let _ = app.emit("float:start", "");
    }

    Ok(())
}

/// Silently clear the recording flag. Used by the hands-free call loop, which
/// drives capture directly (draining chunks for VAD) instead of going through
/// `stop_recording`, so it needs to reset the flag without the "was stuck"
/// warning `force_reset_recording` prints.
pub(crate) fn clear_recording_flag() {
    IS_RECORDING.store(false, Ordering::SeqCst);
}

/// A zeroed [`SttResult`] (empty transcript). Returned by the no-op stop paths.
fn empty_stt_result() -> SttResult {
    SttResult {
        text: String::new(),
        audio_path: String::new(),
        latency_ms: 0,
        duration_secs: 0.0,
        stt_engine: String::new(),
        stt_model: String::new(),
        noise_removed_pct: 0.0,
        gain_db: 0.0,
    }
}

/// Stop recording, save WAV, transcribe via HTTP API, inject text at cursor.
#[tauri::command]
pub async fn stop_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, mode_override: Option<String>) -> Result<SttResult, String> {
    // Stop mic + drain all samples under the shared capture lock. `None` means
    // we weren't recording, so there's nothing to transcribe — return the no-op
    // result exactly as before (no float:processing emit).
    let all_samples = match stop_and_drain(state.inner())? {
        Some(samples) => samples,
        None => return Ok(empty_stt_result()),
    };

    transcribe_samples(&app, state.inner(), all_samples, mode_override, None).await
}

/// Stop the live capture and drain every buffered sample, flipping the
/// `IS_RECORDING` flag under the same `state.audio_capture` lock
/// [`start_recording`] uses — so a concurrent start/stop pair can't interleave
/// (see the invariant note in [`start_recording`]). Returns `None` when nothing
/// was recording (the flag was already clear); `Some(samples)` otherwise
/// (`samples` is empty when the capture slot held no live stream).
///
/// The raw counterpart to [`stop_recording`]: the same lock + swap + drain
/// discipline, without the transcription / injection / stats side effects — so
/// the workflow `MicSource` can reuse it rather than copy the lock logic.
pub(crate) fn stop_and_drain(state: &AppState) -> Result<Option<Vec<i16>>, String> {
    let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
    if !IS_RECORDING.swap(false, Ordering::SeqCst) {
        // Not recording — nothing to stop.
        return Ok(None);
    }
    Ok(Some(match guard.as_mut() {
        Some(capture) => {
            capture.stop();
            let mut samples = Vec::new();
            while let Some(chunk) = capture.take_chunk(200) {
                samples.extend_from_slice(&chunk);
            }
            samples
        }
        None => Vec::new(),
    }))
}

/// Preprocess, save, transcribe (Apple / Whisper-HTTP / chat) and record a set
/// of already-captured 16 kHz mono i16 samples, returning the transcript and
/// metrics. Shared by [`stop_recording`] (which drains the live capture first)
/// and the hands-free call loop (which accumulates chunks itself while running
/// VAD), so both take exactly the same STT + vocab + stats + storage path.
/// `mode_override` selects the STT profile and gates the float-pill lifecycle
/// just as before ("sts-page" and "agent" stay silent). `stt_profile_override`
/// (Workbench P2 Task 9) is a highest-priority STT profile id — the call
/// composite's resolved `stt_widget` ref — that beats the mode-based profile
/// resolution when `Some`; every other caller passes `None` (unchanged
/// behavior).
pub(crate) async fn transcribe_samples(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
    mode_override: Option<String>,
    stt_profile_override: Option<String>,
) -> Result<SttResult, String> {
    let stop_time = std::time::Instant::now();

    // Immediately signal the float pill to switch from recording → processing.
    // Conversation-page turns never touch the pill (they render in-app).
    if mode_override.as_deref() != Some("sts-page") {
        let _ = app.emit("float:processing", ());
    }

    if all_samples.is_empty() {
        return Ok(empty_stt_result());
    }

    // Pure preprocess + STT (+ vocab) via the shared core; this function layers
    // the float-pill, injection, stats, and storage side effects on top.
    let SttCore {
        transcript,
        stt_engine,
        audio_path: audio_path_str,
        stats_model,
        dictation_mode,
        mode_has_llm,
        recording_duration,
        preprocess_metrics,
    } = transcribe_core(app, state, all_samples, mode_override, stt_profile_override, true).await?;

    let latency_ms = stop_time.elapsed().as_millis() as u64;

    // Raw dictations complete inside this function (transcribe + inject), so
    // their end-to-end latency is recorded here; LLM modes are recorded by the
    // hotkey pipeline after processing + injection.
    let mut raw_e2e_done = false;

    // 4. Notify float window (stops the recording animation) — skip for agent mode
    eprintln!("fonos: stop_recording dictation_mode='{}' transcript_len={}", dictation_mode, transcript.len());
    if !matches!(dictation_mode.as_str(), "agent" | "sts-page") {
        // `mode_has_llm` (from the core): when set, the STT transcript is NOT the
        // final output — the LLM caller (the hotkey pipelines in main.rs, or the
        // Dictation view) emits the real float:stop/float:error after LLM +
        // injection completes.

        // 5. Inject raw text only in raw mode (not agent, not LLM modes).
        // Injection runs BEFORE the success emit: on failure the pill must
        // show only the error — a float:stop first would schedule the pill's
        // revert-to-idle timer, which would cut the error display short.
        let mut delivered = true;
        if dictation_mode == "raw" && !transcript.is_empty() {
            eprintln!("fonos: INJECTING raw text at cursor ({} chars)", transcript.len());
            let inj_cfg = state.config.lock().map(|c| c.clone()).unwrap_or_default();
            if let Err(e) = inject_text(&transcript, &inj_cfg) {
                delivered = false;
                let msg = format!("Injection failed: {e}");
                crate::error_surface::emit_float_error(app, &msg);
            } else {
                // Boundary marker: inject_text is a *blocking* call (arboard /
                // xdotool on Linux) with no timeout, sitting between the
                // float:processing emit above and the float:stop emit below. If
                // the logs show "INJECTING raw text at cursor" but never this
                // line (nor "Injection failed"), injection itself is wedged and
                // the pill stays stuck in "Processing".
                eprintln!("fonos: raw injection delivered ({} chars)", transcript.len());
                // Funnel: first successful system-level insertion (record-once).
                if let Ok(db) = state.db.lock() {
                    let _ = fonos_core::funnel::record(&db, "first_insert");
                }
                // Onboarding's guided task listens for this; target_app lets it
                // tell an insertion into another app from one into Fonos. The
                // frontmost_app lookup is one osascript/xdotool round-trip —
                // acceptable on this already-blocking delivery path.
                let target = crate::commands::selection::frontmost_app();
                let target_app: Option<String> =
                    if target.is_empty() { None } else { Some(target) };
                let _ = app.emit(
                    "dictation:delivered",
                    serde_json::json!({ "target_app": target_app }),
                );
            }
            raw_e2e_done = delivered;
        }
        if delivered {
            if mode_has_llm && !transcript.is_empty() {
                // Pipeline isn't done: the transcript still goes to the LLM.
                // Keep the pill in "Processing" and let the LLM caller emit the
                // final float:stop — emitting float:stop here would flash a
                // premature green "Done" before the LLM step even runs.
                let _ = app.emit("float:processing", ());
            } else {
                let _ = app.emit("float:stop", &transcript);
            }
        }
    } else {
        eprintln!("fonos: {dictation_mode} mode — skipping float:stop and inject");
    }

    // Record STT event to stats DB (legacy). `stats_model` comes from the core.
    if !transcript.is_empty() {
        if let Ok(db) = state.db.lock() {
            // Funnel: first non-empty transcript ever (record-once).
            let _ = fonos_core::funnel::record(&db, "first_transcript");
            let _ = fonos_core::stats::record_event(
                &db, "stt", &transcript, "", recording_duration,
                latency_ms as i64, &dictation_mode, &stats_model, "", &audio_path_str,
                0, 0, "",
            );

            // End-to-end latency (key release → injected) for raw dictations.
            if raw_e2e_done {
                let _ = fonos_core::stats::record_dictation_latency(
                    &db, stop_time.elapsed().as_millis() as i64, &dictation_mode, &stats_model,
                );
            }

            // Write to v2 unified entries table — all activity is recorded.
            // Recent view shows everything; Notes view filters to note-only.
            let source = match dictation_mode.as_str() {
                "agent" | "sts-page" => fonos_core::storage::SourceType::Agent,
                "note" => fonos_core::storage::SourceType::Note,
                _ => fonos_core::storage::SourceType::Dictation,
            };
            let entry = fonos_core::storage::Entry {
                id: None,
                created_at: crate::commands::storage::now_iso8601(),
                source_type: source,
                role: fonos_core::storage::EntryRole::User,
                mode: dictation_mode.clone(),
                raw_text: transcript.clone(),
                processed_text: None,
                container_id: if dictation_mode == "note" {
                    // Resolve the note panel's selected notebook (stored in
                    // AppState) through the shared resolver: no target / the
                    // sentinel 0 / a since-deleted notebook all fall back to
                    // Quick Note, so a note dictation never writes into a dead
                    // container row.
                    let target = state.note_target.lock().ok().and_then(|g| *g);
                    fonos_core::storage::resolve_notebook_container(&db, target.unwrap_or(0))
                } else {
                    None
                },
                audio_ref: if dictation_mode == "note" { None } else { Some(audio_path_str.clone()) },
                metadata: serde_json::json!({
                    "duration_secs": recording_duration,
                    "latency_ms": latency_ms,
                }),
            };
            if let Err(e) = fonos_core::storage::insert_entry(&db, &entry) {
                eprintln!("fonos: entry write error: {e}");
            }
        }
    }

    Ok(SttResult {
        text: transcript,
        audio_path: audio_path_str,
        latency_ms,
        duration_secs: recording_duration,
        stt_engine,
        stt_model: stats_model,
        noise_removed_pct: preprocess_metrics.0,
        gain_db: preprocess_metrics.1,
    })
}

/// The pure preprocess + STT product, before any float-pill / stats / storage /
/// injection side effects — returned by [`transcribe_core`] and layered on by
/// its callers.
struct SttCore {
    transcript: String,
    /// Apple Speech engine tag ("on-device"/"server"), empty for HTTP providers.
    stt_engine: String,
    /// Path to the saved WAV (kept for history / stats `audio_ref`).
    audio_path: String,
    /// Model label for stats (`"Apple Speech (…)"` or the HTTP model name).
    stats_model: String,
    /// Effective dictation mode after resolving `mode_override`.
    dictation_mode: String,
    /// Whether the resolved mode runs a downstream LLM step.
    mode_has_llm: bool,
    recording_duration: f64,
    /// `(noise_removed_pct, gain_db)` from preprocessing.
    preprocess_metrics: (f64, f64),
}

/// Resolve the STT profile from the `stt.default` widget (overridable), run
/// the shared [`stt_transcribe`] core (preprocess + Apple/Whisper-HTTP/chat),
/// and apply the deterministic vocab post-correction. This is the STT step
/// shared by [`transcribe_samples`] (which then records stats, storage, and
/// drives the float pill / injection) and the call-mode barge content
/// verifier ([`transcribe_stt_only`], which wants only the transcript).
///
/// STT configuration (model profile, whisper prompt, sampling temperature,
/// language, extra vocab books) comes from the built-in `stt.default`
/// processor widget — the same widget the engine's `SttProcessor` reads for
/// every other mic-sourced workflow (Workbench P1 unified dictation onto one
/// shared STT widget instead of a per-mode config; this function was the one
/// remaining straggler still reading the legacy per-mode fields directly,
/// cut over in Workbench P2 Task 12). `mode_override` / `config.dictation_mode`
/// only decides the (separate) `llm.{mode}` widget used for the downstream
/// LLM step — see [`dictation_mode_llm_props`].
///
/// No stats, storage, or float-pill side effects of its own; an STT *error* is
/// still surfaced to the float pill for interactive modes (not "agent"/"sts-page"),
/// exactly as before. `mode_override` selects the LLM-step mode just as in
/// [`transcribe_samples`]; `stt_profile_override` (Task 9, the call
/// composite's resolved `stt_widget` ref — possibly the `"apple-speech"`
/// sentinel) beats `stt.default`'s own `model_profile` when `Some`;
/// `apply_vocab` runs the vocab rules (the verifier skips them for speed).
async fn transcribe_core(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
    mode_override: Option<String>,
    stt_profile_override: Option<String>,
    apply_vocab: bool,
) -> Result<SttCore, String> {
    let recording_duration = all_samples.len() as f64 / 16000.0;

    // Load config to resolve the effective dictation mode (for the LLM step)
    // and the stt.default widget's props (for the STT step) in one lock scope.
    // mode_override (from Dictation view) takes precedence over config.dictation_mode (float pill)
    let (dictation_mode, stt_props, widgets) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let mode = mode_override.unwrap_or_else(|| config.dictation_mode.clone());
        let widgets = effective_widgets(&config);
        let stt_props = resolve_stt_default_props(&widgets);
        (mode, stt_props, widgets)
    };

    // Resolve effective vocab books for this dictation (global ∪ stt.default's, issue #3).
    // Cloned out of the config lock so the books outlive the transcription awaits.
    let vocab_books: Vec<fonos_core::vocab::VocabBook> = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        fonos_core::vocab::effective_books(&config.vocab_books, &config.global_vocab_books, &stt_props.vocab_books)
            .into_iter()
            .cloned()
            .collect()
    };
    let vocab_refs: Vec<&fonos_core::vocab::VocabBook> = vocab_books.iter().collect();
    let vocab_terms = fonos_core::vocab::collect_terms(&vocab_refs);

    // Read STT config — an explicit profile override (Task 9: the call
    // composite's resolved stt_widget ref) wins outright, then the Apple
    // Speech sentinel, then stt.default's own model_profile, then the global
    // default. Mirrors `workflow_widgets::SttProcessor`'s resolution exactly.
    let stt = match &stt_profile_override {
        Some(profile) if profile == "apple-speech" => {
            eprintln!("fonos: STT using Apple Speech on-device (override, mode={dictation_mode})");
            super::ServiceConfig {
                base_url: String::new(), api_key: String::new(),
                model: "apple-speech".to_string(), provider: "apple".to_string(),
                stt_api: "whisper".to_string(),
            }
        }
        Some(profile) => {
            eprintln!("fonos: STT using explicit override profile '{profile}' (mode={dictation_mode})");
            super::get_service_config_for_profile(state, profile)
        }
        None if stt_props.model_profile == "apple-speech" => {
            eprintln!("fonos: STT using Apple Speech on-device (mode={})", dictation_mode);
            super::ServiceConfig {
                base_url: String::new(), api_key: String::new(),
                model: "apple-speech".to_string(), provider: "apple".to_string(),
                stt_api: "whisper".to_string(),
            }
        }
        None if !stt_props.model_profile.is_empty() => {
            eprintln!("fonos: STT using stt.default profile '{}' (mode={})", stt_props.model_profile, dictation_mode);
            super::get_service_config_for_profile(state, &stt_props.model_profile)
        }
        None => {
            eprintln!("fonos: STT using global default profile (mode={})", dictation_mode);
            super::get_service_config(state, "stt")
        }
    };
    eprintln!("fonos: STT provider={} endpoint={} model={}", stt.provider, stt.base_url, stt.model);

    // stt.default's own whisper prompt and sampling temperature (vocab biasing
    // is merged in by the shared core itself).
    let stt_prompt = stt_props.stt_prompt.clone();
    let stt_temperature = stt_props.temperature;
    let stt_language = stt_props.language.clone();

    // ── Shared "samples → transcript" STT core (Apple / Whisper-HTTP / chat) ──
    // Returns the raw transcript plus the byproducts this function surfaces
    // (Apple engine tag, saved WAV path, preprocess metrics). An STT failure is
    // returned as Err(raw); surface it on the float pill for interactive modes
    // here, exactly as the inline dispatch did before.
    let SttTranscription {
        transcript,
        stt_engine,
        audio_path: audio_path_str,
        preprocess_metrics,
    } = match stt_transcribe(
        all_samples,
        stt.clone(),
        stt_language,
        stt_prompt,
        vocab_terms,
        stt_temperature,
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            let msg = format!("STT failed via {} at {}: {}", stt.provider, stt.base_url, e);
            if !matches!(dictation_mode.as_str(), "agent" | "sts-page") {
                crate::error_surface::emit_float_error(app, &msg);
            } else {
                eprintln!("fonos: {msg}");
            }
            return Err(msg);
        }
    };

    // ② Deterministic post-correction from the effective vocab books — runs
    // for every mode (raw, LLM, note, agent) before any downstream use.
    let transcript = if !apply_vocab || vocab_refs.is_empty() {
        transcript
    } else {
        fonos_core::vocab::apply_rules(&transcript, &vocab_refs)
    };

    let mode_has_llm = dictation_mode_llm_props(&widgets, &dictation_mode).is_some();

    let stats_model = if stt.provider == "apple" {
        format!("Apple Speech ({})", if stt_engine.is_empty() { "server" } else { &stt_engine })
    } else if stt.model.is_empty() {
        "fast".to_string()
    } else {
        stt.model.clone()
    };

    Ok(SttCore {
        transcript,
        stt_engine,
        audio_path: audio_path_str,
        stats_model,
        dictation_mode,
        mode_has_llm,
        recording_duration,
        preprocess_metrics,
    })
}

/// The raw product of the shared "samples → transcript" STT core
/// ([`stt_transcribe`]): the transcript *before* vocab post-correction, plus the
/// byproducts callers surface (Apple engine tag, saved WAV path, preprocess
/// metrics).
pub(crate) struct SttTranscription {
    /// Raw transcript from the STT backend — no vocab [`fonos_core::vocab::apply_rules`] yet.
    pub transcript: String,
    /// Apple Speech engine tag (`"on-device"`/`"server"`); empty for HTTP/chat.
    pub stt_engine: String,
    /// Path to the saved WAV (kept for history / stats `audio_ref`).
    pub audio_path: String,
    /// `(noise_removed_pct, gain_db)` from preprocessing.
    pub preprocess_metrics: (f64, f64),
}

/// Preprocess (high-pass + normalize), persist a 16 kHz mono WAV, and transcribe
/// already-captured `samples` via the *already-resolved* STT profile `svc`
/// (Apple on-device / Whisper-HTTP / chat-completions), biasing recognition with
/// `vocab_terms` and the whisper `prompt` at `temperature`. `language` is a
/// display name (`"English"`, `"chinese"`, …) or `"auto"`/`""`; it is mapped to
/// an ISO 639-1 / BCP-47 code here.
///
/// This is the mode-free STT core shared by the dictation flow
/// ([`transcribe_core`]) and the workflow `SttProcessor`. It applies **no** vocab
/// post-correction (`apply_rules` is the caller's job, so each caller picks its
/// own book set) and has **no** stats / storage / float-pill / injection side
/// effects: an STT failure is returned as `Err(raw)` for the caller to surface.
/// Apple failures yield an empty transcript (never `Err`), matching the prior
/// inline dispatch.
pub(crate) async fn stt_transcribe(
    samples: Vec<i16>,
    svc: fonos_core::llm::ServiceConfig,
    language: String,
    prompt: String,
    vocab_terms: Vec<String>,
    temperature: f64,
) -> Result<SttTranscription, String> {
    // 1. Audio preprocessing: high-pass filter + RMS normalization.
    let (samples, preprocess_metrics) = preprocess_audio(samples);

    // 2. Save WAV for history + transcription.
    let audio_dir = std::env::temp_dir().join("fonos_audio");
    let _ = std::fs::create_dir_all(&audio_dir);
    let audio_path = audio_dir.join(format!("stt_{}.wav", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));

    let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fonos_core::audio::write_wav(&audio_path, &pcm_bytes, 16000)
        .map_err(|e| e.to_string())?;
    let audio_path_str = audio_path.to_string_lossy().to_string();

    // 3. Read the WAV bytes back for the HTTP / chat upload path.
    let file_bytes = std::fs::read(&audio_path)
        .map_err(|e| format!("failed to read WAV: {e}"))?;

    // Convert language name to ISO 639-1 / BCP-47 code.
    let lang_code = {
        let lang = language.trim();
        if lang.is_empty() || lang == "auto" { String::new() } else {
            match lang.to_lowercase().as_str() {
                "chinese"    => "zh",
                "english"    => "en",
                "japanese"   => "ja",
                "korean"     => "ko",
                "cantonese"  => "yue",
                "french"     => "fr",
                "german"     => "de",
                "spanish"    => "es",
                "portuguese" => "pt",
                "italian"    => "it",
                "russian"    => "ru",
                "arabic"     => "ar",
                "hindi"      => "hi",
                "thai"       => "th",
                "vietnamese" => "vi",
                "dutch"      => "nl",
                "polish"     => "pl",
                "turkish"    => "tr",
                "indonesian" => "id",
                other        => other,
            }.to_string()
        }
    };

    let model_name = if svc.model.is_empty() { "fast".to_string() } else { svc.model.clone() };

    // ── Branch: Apple on-device / Whisper API / Chat Completions STT ────────
    if svc.provider == "apple" {
        // Apple Speech is a macOS-only SFSpeechRecognizer helper. On any other
        // platform the helper binary cannot exist; the old code returned an
        // empty transcript here, silently swallowing the failure so the pill
        // flashed a bogus "no speech" (or appeared stuck) instead of a real
        // error. Return an explicit Err so the caller (transcribe_core) surfaces
        // it via emit_float_error — see the `Err(e)` arm in transcribe_core.
        #[cfg(target_os = "macos")]
        {
            let (transcript, stt_engine) =
                transcribe_apple(&audio_path_str, &lang_code, &vocab_terms).await;
            return Ok(SttTranscription { transcript, stt_engine, audio_path: audio_path_str, preprocess_metrics });
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err(
                "Apple Speech is only available on macOS. Pick a different STT model in Settings.".to_string(),
            );
        }
    }

    let result = if svc.stt_api == "chat" {
        eprintln!("fonos: STT via chat completions (base64 audio)");
        transcribe_chat(&svc, &file_bytes, &lang_code, &vocab_terms).await
    } else {
        transcribe_http(&svc, &file_bytes, &model_name, &lang_code, &prompt, temperature, &vocab_terms).await
    };

    // Non-Apple STT failures propagate as Err(raw); the caller surfaces them.
    let transcript = result?;
    Ok(SttTranscription {
        transcript,
        stt_engine: String::new(),
        audio_path: audio_path_str,
        preprocess_metrics,
    })
}

/// STT-only transcription for the call-mode barge content verifier: preprocess +
/// transcribe and return just the transcript. No vocab pass (speed), and no
/// stats, storage, float-pill, or injection side effects. Uses the same STT
/// profile the call's listen phase uses (mode "sts-page" → global default), so
/// the snippet is transcribed exactly as a normal listen would transcribe it.
pub(crate) async fn transcribe_stt_only(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
) -> Result<String, String> {
    if all_samples.is_empty() {
        return Ok(String::new());
    }
    let core =
        transcribe_core(app, state, all_samples, Some("sts-page".to_string()), None, false).await?;
    Ok(core.transcript)
}

/// Transcribe an audio file via POST /v1/audio/transcriptions.
#[tauri::command]
pub async fn transcribe_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let file_bytes = tokio::fs::read(&path).await
        .map_err(|e| format!("failed to read '{path}': {e}"))?;

    let file_name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let stt = super::get_service_config(&state, "stt");
    let model_name = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model_name);
    let url = format!("{}/v1/audio/transcriptions", stt.base_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().map_err(|e| e.to_string())?;

    let mut req = client.post(&url).multipart(form);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    let response = req.send().await
        .map_err(|e| format!("transcription failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("transcription error {status}: {body}"));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("parse error: {e}"))?;

    Ok(json["text"].as_str().unwrap_or("").to_string())
}

// ─── STT provider helpers ────────────────────────────────────────────────────

/// Transcribe via macOS SFSpeechRecognizer (calls the bundled Swift helper).
/// Returns (transcript, engine) where engine is "on-device" or "server".
///
/// macOS-only: the `svc.provider == "apple"` branch in [`stt_transcribe`]
/// returns an explicit error on every other platform, so this helper (and the
/// binary finder below) are never referenced off macOS.
#[cfg(target_os = "macos")]
async fn transcribe_apple(audio_path: &str, lang_code: &str, vocab_terms: &[String]) -> (String, String) {
    // Find the helper binary next to the app binary or in resources
    let helper = find_apple_stt_binary();
    let Some(helper_path) = helper else {
        eprintln!("fonos: fonos-stt-apple binary not found — cannot use Apple Speech");
        return (String::new(), String::new());
    };

    // Map ISO 639-1 → BCP-47 locale for Apple Speech
    let locale = match lang_code {
        "zh" => "zh-CN",
        "en" => "en-US",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "es" => "es-ES",
        "pt" => "pt-BR",
        "it" => "it-IT",
        "ru" => "ru-RU",
        "ar" => "ar-SA",
        "th" => "th-TH",
        "vi" => "vi-VN",
        "nl" => "nl-NL",
        "pl" => "pl-PL",
        "tr" => "tr-TR",
        "id" => "id-ID",
        other if !other.is_empty() => other,
        _ => "en-US",
    };

    eprintln!("fonos: Apple STT transcribing {} (locale={})", audio_path, locale);

    let mut cmd = tokio::process::Command::new(&helper_path);
    cmd.arg(audio_path).arg(locale);
    // Vocabulary biasing via SFSpeechRecognizer contextualStrings.
    if !vocab_terms.is_empty() {
        if let Ok(json) = serde_json::to_string(vocab_terms) {
            cmd.arg(json);
        }
    }
    let output = cmd.output().await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("fonos: Apple STT failed: {}{}", stdout.trim(), stderr.trim());
                return (String::new(), String::new());
            }
            let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_default();
            if let Some(err) = json["error"].as_str() {
                eprintln!("fonos: Apple STT error: {}", err);
                return (String::new(), String::new());
            }
            let text = json["text"].as_str().unwrap_or("").to_string();
            let engine = json["engine"].as_str().unwrap_or("server").to_string();
            let on_device = json["on_device_available"].as_bool().unwrap_or(false);
            eprintln!("fonos: Apple STT [{}] (on-device available: {}): {}",
                engine, on_device, text.chars().take(80).collect::<String>());
            (text, engine)
        }
        Err(e) => {
            eprintln!("fonos: failed to run fonos-stt-apple: {e}");
            (String::new(), String::new())
        }
    }
}

/// Locate the fonos-stt-apple binary.
///
/// Delegates to the shared six-candidate finder (dev/`cargo test`/packaged
/// `.app`) — this used to hand-roll a four-candidate search that lacked the
/// nested `Contents/Resources/resources/` path Tauri v2 actually bundles
/// into, so packaged apps could never find the helper. Same bug family as
/// `fonos-audio-capture` (d7978bc) and `fonos-voice-capture` (#56).
#[cfg(target_os = "macos")]
fn find_apple_stt_binary() -> Option<String> {
    crate::audio::diarize::find_helper_binary("fonos-stt-apple")
}

// STT clients moved to fonos-core (issue #21); re-exported so existing
// call sites (meeting.rs, warm-up, test_stt) keep their paths.
pub use fonos_core::stt::{transcribe_chat, transcribe_http};

// ─── STT endpoint verification ───────────────────────────────────────────────

/// Verify a model's STT endpoint by sending a short silent probe clip.
///
/// Distinguishes "endpoint works" (any 2xx, even an empty transcript from
/// silence) from "endpoint missing/broken" (404/401/network). Lets users
/// confirm a self-hosted STT server before relying on it, instead of finding
/// out via a silent empty dictation.
#[tauri::command]
pub async fn test_stt(state: tauri::State<'_, AppState>, profile_id: String) -> Result<String, String> {
    let stt = super::get_service_config_for_profile(&state, &profile_id);

    if stt.provider == "apple" {
        // Apple Speech is macOS-only (SFSpeechRecognizer helper). Off macOS it
        // can never run, so report that instead of a misleading "nothing to test".
        #[cfg(target_os = "macos")]
        {
            return Ok("Apple on-device speech — no network endpoint to test.".to_string());
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err("Apple Speech is only available on macOS.".to_string());
        }
    }
    if stt.base_url.trim().is_empty() {
        return Err("This model has no base URL configured.".to_string());
    }

    // Short silent WAV probe clip (shared with the warm-up path).
    let bytes = silent_probe_wav();
    if bytes.is_empty() {
        return Err("failed to build probe clip".to_string());
    }

    let model_name = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
    let result = if stt.stt_api == "chat" {
        transcribe_chat(&stt, &bytes, "", &[]).await
    } else {
        transcribe_http(&stt, &bytes, &model_name, "", "", 0.0, &[]).await
    };

    match result {
        Ok(_) => Ok(format!("OK — {} responded at {}", stt.provider, stt.base_url)),
        Err(e) => Err(e),
    }
}

// ─── Audio preprocessing ─────────────────────────────────────────────────────

/// Apply speech-optimized preprocessing: high-pass filter (80Hz) + RMS normalization.
/// Returns (processed_samples, (noise_removed_pct, gain_db)).
fn preprocess_audio(samples: Vec<i16>) -> (Vec<i16>, (f64, f64)) {
    if samples.is_empty() { return (samples, (0.0, 0.0)); }

    // 1. High-pass filter at 80Hz (first-order IIR)
    let alpha: f64 = 1.0 / (1.0 + 2.0 * std::f64::consts::PI * 80.0 / 16000.0);
    let mut filtered = Vec::with_capacity(samples.len());
    let mut prev_in: f64 = samples[0] as f64;
    let mut prev_out: f64 = samples[0] as f64;
    filtered.push(samples[0]);
    for &s in &samples[1..] {
        let x = s as f64;
        let y = alpha * (prev_out + x - prev_in);
        prev_in = x;
        prev_out = y;
        filtered.push(y.round().clamp(-32768.0, 32767.0) as i16);
    }

    // Measure noise removed: energy difference between original and filtered
    let energy_orig: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let energy_removed: f64 = samples.iter().zip(filtered.iter())
        .map(|(&a, &b)| { let d = a as f64 - b as f64; d * d }).sum();
    let noise_removed_pct = if energy_orig > 0.0 { 100.0 * energy_removed / energy_orig } else { 0.0 };

    // 2. RMS normalization to -20 dBFS (target ~3277)
    let rms: f64 = {
        let sum_sq: f64 = filtered.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum_sq / filtered.len() as f64).sqrt()
    };

    if rms < 1.0 {
        eprintln!("fonos: audio preprocessing: silence detected, skipping normalization");
        return (filtered, (noise_removed_pct, 0.0));
    }

    let target_rms: f64 = 3277.0;
    let gain = (target_rms / rms).min(10.0);
    let gain_db = 20.0 * gain.log10();

    let normalized: Vec<i16> = filtered.iter().map(|&s| {
        let v = (s as f64 * gain).round();
        v.clamp(-32768.0, 32767.0) as i16
    }).collect();

    eprintln!("fonos: audio preprocessing: HPF removed {:.1}% noise, normalized {:.1}dB",
        noise_removed_pct, gain_db);

    (normalized, (noise_removed_pct, gain_db))
}

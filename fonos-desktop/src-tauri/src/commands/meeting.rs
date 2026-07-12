//! Tauri commands for meeting mode.
//!
//! Meeting mode captures audio continuously (dual-channel when ScreenCaptureKit
//! is available, mic-only otherwise), transcribes each chunk via the configured
//! STT engine, streams results to the meeting-panel webview, and generates an
//! AI summary when the meeting ends.
//!
//! Workbench P2 Task 7 split each command's guts into a `pub(crate)` `*_with`
//! function taking already-resolved parameters (an STT [`ServiceConfig`] for
//! start, an LLM model-profile id + summary-prompt override for stop), so
//! [`super::meeting_widget::MeetingOutput`] (the `meeting` composite output)
//! can drive the exact same logic with its own `stt_widget`/`llm_widget`-ref
//! resolution instead of this module's global-only fallback. The thin
//! `#[tauri::command]` shells stay registered (the `meeting-panel.html` panel
//! still invokes `stop_meeting` directly via `iv('stop_meeting')`) and keep
//! their original global-fallback behavior for any other caller. This also
//! fixes a pre-existing bug: `start_meeting` used to read the global `"stt"`
//! profile unconditionally, silently ignoring `config.meeting_stt_profile` —
//! see `meeting_widget::MeetingOutput`'s STT resolution for the fix (a
//! `stt_widget` ref wins, then this now-consulted field, then the global
//! profile).

use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;

use fonos_core::llm::ServiceConfig;
use fonos_core::storage::{
    Container, ContainerType, Entry, EntryRole, SourceType,
};

use super::AppState;
use crate::commands::storage::now_iso8601;

// ─── MeetingState ─────────────────────────────────────────────────────────────

/// Mutable state for an active (or just-ended) meeting session.
pub struct MeetingState {
    /// Whether a meeting is currently being recorded.
    pub recording: bool,
    /// The container ID of the current meeting session.
    pub container_id: Option<i64>,
    /// Running count of transcribed chunks.
    pub chunk_counter: i32,
    /// Wall-clock time when the meeting started.
    pub start_time: Option<std::time::Instant>,
}

impl MeetingState {
    pub fn new() -> Self {
        Self {
            recording: false,
            container_id: None,
            chunk_counter: 0,
            start_time: None,
        }
    }
}

// ─── MeetingDetail ────────────────────────────────────────────────────────────

/// Full detail for a single meeting session, returned by `get_meeting_detail`.
#[derive(Serialize)]
pub struct MeetingDetail {
    /// The meeting_session container.
    pub container: Container,
    /// All transcript entries (role = user / participant).
    pub entries: Vec<Entry>,
    /// The AI-generated summary entry (role = system), if it exists.
    pub summary: Option<Entry>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// [`js_escape`] moved to `commands::mod` (final review wave, I2) so
/// `agent_widget.rs` could reuse it too instead of its own buggy manual
/// escaping — re-exported here so this module's own call sites (and
/// `meeting_widget.rs`'s existing `use super::meeting::{..., js_escape, ...}`
/// import) keep working unchanged.
pub(crate) use super::js_escape;

/// Helper: evaluate JS in the meeting-panel webview.
///
/// Security note (mirrors `agent_widget::agent_js`'s doc comment):
/// `WebviewWindow::eval` injects JS into an app-owned Tauri webview (the
/// bundled `meeting-panel.html`), not a general code-exec sink for untrusted
/// input. Every value interpolated into `js` by callers is pre-escaped via
/// [`js_escape`] (single-quoted `recv*('...')` calls) or `serde_json::to_string`
/// (double-quoted JSON args) before reaching this function.
pub(crate) fn meeting_js(app: &tauri::AppHandle, js: &str) {
    if let Some(panel) = app.get_webview_window("meeting-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: meeting-panel JS eval error: {e}");
        }
    }
}

/// The default meeting title: `"{Meeting label} {yyyy-MM-dd HH:mm}"`, derived
/// from the current time. `pub(crate)` (not just used by `start_meeting_with`
/// below) so [`super::meeting_widget::MeetingOutput`] can compute it BEFORE
/// calling `start_meeting_with` — it needs the title immediately, for its own
/// `recvStart` panel eval, rather than waiting on `start_meeting_with`'s
/// return.
///
/// Workbench P2 Task 13: the label is `meeting.default`'s bilingual builtin
/// display name (`fonos_core::workflow::builtin::builtin_display_name`) for
/// `lang`, so the meeting panel/container title reads "Meeting …" for an
/// EN-language user and "会议 …" for a ZH-language user (never a hardcoded
/// English literal regardless of `config.ui_language`, as it was before this
/// task) — falls back to the literal "Meeting" in the (unreachable outside a
/// map-coverage bug) case the id has no display name.
pub(crate) fn default_meeting_title(lang: fonos_core::workflow::builtin::Lang) -> String {
    let now = now_iso8601();
    let ts = now[..16].replace('T', " ");
    let label = fonos_core::workflow::builtin::builtin_display_name("meeting.default", lang)
        .unwrap_or("Meeting");
    format!("{label} {}", ts)
}

/// The built-in meeting-summary instructions, used when no custom prompt
/// (component prop or, pre-Task-7, the deprecated `meeting_summary_prompt`
/// config field) is set. Extracted to a constant so [`build_summary_prompt`]
/// can substitute a custom prompt in its place while still appending the same
/// transcript tail.
const BUILTIN_SUMMARY_INSTRUCTIONS: &str = concat!(
    "You are a meeting assistant. Given the following transcript, generate:\n",
    "1. A concise meeting summary (3-5 sentences)\n",
    "2. Key discussion points (bullet list grouped by topic)\n",
    "3. Action items (who does what, by when)\n",
    "4. Decisions made during the meeting\n\n",
    "Output in clean Markdown. Do not wrap in a code block.",
);

/// Map a stored `speaker_hint` (`"me"`, a diarization machine tag like
/// `"s1"`/`"s10"`, or the legacy `"audio"` catch-all) to the display label
/// used both in the summary transcript ([`build_summary_prompt`]) and by the
/// meeting-panel/detail UI (Task 6 depends on this exact behavior): `"me"` →
/// `"Me"`; a diarization tag (`s` followed by one-or-more ASCII digits, e.g.
/// `"s1"`/`"s10"`) passes through UNCHANGED (it's already the machine tag the
/// UI groups by, not a display string to translate) so per-speaker labeling
/// survives; anything else (the legacy `"audio"` hint, an empty string, or a
/// malformed tail like `"sx"`) collapses to the pre-diarization `"Audio"`
/// catch-all.
pub(crate) fn speaker_display_hint(hint: &str) -> String {
    if hint == "me" {
        return "Me".to_string();
    }
    if let Some(rest) = hint.strip_prefix('s') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return hint.to_string();
        }
    }
    "Audio".to_string()
}

/// Build the AI summary prompt from a list of transcript entries.
///
/// `custom_instructions` replaces [`BUILTIN_SUMMARY_INSTRUCTIONS`] when
/// non-empty (`MeetingProps::summary_prompt`'s component-prop-first
/// resolution — Workbench P2 Task 7); empty falls back to the built-in
/// literal, so this is a pure superset of the pre-Task-7 always-literal
/// behavior. Either way, an empty transcript still short-circuits to an empty
/// prompt (regardless of `custom_instructions`) — an empty entries list means
/// there's nothing to summarize.
pub fn build_summary_prompt(entries: &[Entry], custom_instructions: &str) -> String {
    let transcript = entries
        .iter()
        .enumerate()
        .map(|(_i, e)| {
            let speaker = e
                .metadata
                .get("speaker_hint")
                .and_then(|v| v.as_str())
                .map(speaker_display_hint)
                .unwrap_or_else(|| "Me".to_string());
            let ts = e
                .metadata
                .get("timestamp_in_session")
                .and_then(|v| v.as_str())
                .unwrap_or("--:--");
            format!("[{}] {}: {}", ts, speaker, e.raw_text)
        })
        .collect::<Vec<_>>()
        .join("\n");

    if transcript.is_empty() {
        return String::new();
    }

    let instructions = if custom_instructions.is_empty() {
        BUILTIN_SUMMARY_INSTRUCTIONS
    } else {
        custom_instructions
    };
    format!("{instructions}\n\nTRANSCRIPT:\n{transcript}")
}

/// Resolve `profile_id` into connection info for the meeting summary LLM call
/// — pure (no lock/await), so callers resolve it inside a scoped
/// config-lock block and drop the guard before their subsequent `.await`
/// (same lock discipline as `commands::agent::resolve_agent_llm_service`,
/// which this mirrors almost verbatim). An empty or unknown `profile_id` is a
/// hard error, matching the original `call_llm_raw`'s inline behavior.
fn resolve_llm_profile_service(
    config: &fonos_core::config::AppConfig,
    profile_id: &str,
) -> Result<ServiceConfig, String> {
    if profile_id.is_empty() {
        return Err("No LLM profile configured. Please add one in Settings > Model Registry.".into());
    }
    let profile = config
        .model_profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| format!("LLM profile '{}' not found", profile_id))?;
    Ok(super::config_from_profile(profile))
}

/// Call an LLM with a raw prompt string via an already-resolved service — no
/// config/state access at all, so it never holds a lock across this `.await`.
/// Returns the generated text or an error string. `profile_id` is only used
/// for the log line (the connection info itself is all in `svc`).
async fn call_llm_raw(svc: &ServiceConfig, profile_id: &str, prompt: &str) -> Result<String, String> {
    use fonos_core::llm::call_openai_compatible;
    use fonos_core::llm::call_anthropic;

    if prompt.is_empty() {
        return Ok(String::new());
    }

    eprintln!(
        "fonos: meeting summary LLM profile={} provider={} model={}",
        profile_id, svc.provider, svc.model
    );

    let messages = vec![
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let resp = if svc.provider == "anthropic" {
        call_anthropic(&svc.api_key, &svc.model, &messages, 0.3, 2048).await
    } else {
        call_openai_compatible(&svc.api_key, &svc.model, &svc.base_url, &messages, 0.3, 2048, &svc.provider).await
    };

    resp.map(|r| r.text).map_err(|e| e.to_string())
}

/// Schedule hiding the meeting-panel window after a brief delay — moved here
/// (Workbench P2 Task 7) from the retired `main.rs` "meeting" hotkey arm,
/// which ran this unconditionally right after `stop_meeting` resolved
/// (Ok or Err alike). Called from [`stop_meeting_with`] once the recording
/// has genuinely stopped, so both the composite (`MeetingOutput`) and the
/// plain `stop_meeting` command shell (the panel's own stop button) get the
/// same auto-hide — previously only the hotkey-toggle path did. Harmless if
/// the panel is already hidden by the time this fires (e.g. by the panel's
/// own `recvSummary`-driven 3s auto-hide in `meeting-panel.html`, which this
/// duplicates as a longer-delay backstop, exactly as it did before this
/// task).
fn schedule_delayed_hide(app: tauri::AppHandle) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if let Some(panel) = app.get_webview_window("meeting-panel") {
            let _ = panel.hide();
        }
    });
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Start a new meeting session using the global `"stt"` service profile —
/// the plain command-shell's fallback (no widget-ref awareness; see the
/// module docs). The panel no longer invokes this directly (only
/// `stop_meeting`, via its stop button) and neither does the retired
/// `main.rs` hotkey arm (folded into the `meeting` composite — see
/// `meeting_widget::MeetingOutput`, which calls [`start_meeting_with`]
/// directly with its own resolved STT service instead); this command shell
/// stays registered for IPC/API compat (`lib/meeting-api.ts`'s
/// `startMeeting()` binding).
///
/// Returns the container ID of the new meeting session.
#[tauri::command(rename_all = "snake_case")]
pub async fn start_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let stt_svc = super::get_service_config(&state, "stt");
    // A poisoned lock degrades to resolve_lang("auto") rather than failing
    // the whole command (same convention as dialog.rs/workflow_widgets.rs) —
    // losing the language pick shouldn't block starting the meeting.
    let lang = match state.config.lock() {
        Ok(config) => fonos_core::workflow::builtin::resolve_lang(&config.ui_language),
        Err(_) => fonos_core::workflow::builtin::resolve_lang("auto"),
    };
    let title = default_meeting_title(lang);
    // Legacy command shell has no component props to carry a `diarize` flag
    // — pass `false` (unchanged pre-diarization behavior for this caller).
    start_meeting_with(&app, &state, stt_svc, title, false).await
}

/// Create a `meeting_session` container, set up dual-channel capture, and
/// spawn a background task that transcribes audio chunks and streams results
/// to the `meeting-panel` webview — the parameterized guts shared by the
/// plain `start_meeting` command (global-only fallback) and
/// `meeting_widget::MeetingOutput` (resolves `stt_svc` from its own
/// `stt_widget` ref, falling back to the legacy `meeting_stt_profile` field —
/// **the BUG 1 fix**: this function itself no longer reads config, so it
/// can't silently ignore that field the way the old unparameterized
/// `start_meeting` did).
///
/// `title` is the caller's already-decided meeting title (rather than
/// generated here) so `MeetingOutput` can compute it once and reuse it for
/// its own immediate `recvStart` panel eval before this function's spawned
/// capture loop reaches its own (later, capture-outcome-accurate) `recvStart`
/// call — see [`default_meeting_title`].
///
/// `diarize` is [`super::meeting_widget::MeetingProps::diarize`] (`false` for
/// the plain `start_meeting` command shell, which has no component props).
/// When true AND capture negotiates dual-channel audio, a `DiarizeSession`
/// (`crate::audio::diarize`) is spawned and fed the system-audio channel
/// alongside transcription so remote-participant entries can be relabeled by
/// dominant speaker. Every way this can fail to give a working session (mono
/// capture, helper/models missing, spawn error, mid-session death) degrades
/// to the pre-diarization flat `"Audio"` labeling — diarization never
/// interrupts the meeting itself, only notifies the panel via
/// `recvDiarizeNotice`.
///
/// Returns the container ID of the new meeting session.
pub(crate) async fn start_meeting_with(
    app: &tauri::AppHandle,
    state: &AppState,
    stt_svc: ServiceConfig,
    title: String,
    diarize: bool,
) -> Result<i64, String> {
    // Guard against double-start
    {
        let ms = state.meeting.lock().await;
        if ms.recording {
            return Err("Meeting already in progress".into());
        }
    }

    // 1. Create meeting_session container
    let now = now_iso8601();

    let container = Container {
        id: None,
        container_type: ContainerType::MeetingSession,
        title: title.clone(),
        parent_id: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        metadata: serde_json::json!({
            "audio_source": "mic_only",
            "channel_mode": "mono",
            "summary_generated": false,
            "duration_total_ms": 0,
        }),
    };

    let container_id = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        fonos_core::storage::insert_container(&db, &container).map_err(|e| e.to_string())?
    };

    eprintln!("fonos: start_meeting container_id={}", container_id);

    // 2. Update meeting state
    {
        let mut ms = state.meeting.lock().await;
        ms.recording = true;
        ms.container_id = Some(container_id);
        ms.chunk_counter = 0;
        ms.start_time = Some(std::time::Instant::now());
    }

    // 3. STT config — resolved by the caller (see this function's doc comment).
    let stt_base_url = stt_svc.base_url;
    let stt_api_key = stt_svc.api_key;
    let stt_model = if stt_svc.model.is_empty() { "fast".to_string() } else { stt_svc.model };

    // 4. Spawn the chunk-transcription loop
    let app_handle = app.clone();
    let meeting_arc = Arc::clone(&state.meeting);
    let db_arc = Arc::clone(&state.db);

    tauri::async_runtime::spawn(async move {
        use crate::audio::dual_capture::DualCapture;
        use fonos_core::audio::write_wav;
        use crate::commands::dictation::transcribe_http;

        // Log system audio availability before attempting dual capture
        {
            use crate::audio::system_capture::SystemAudioCapture;
            eprintln!(
                "fonos: system audio available: {}",
                SystemAudioCapture::is_available()
            );
        }

        // Create and start dual-channel capture
        let mut capture = match DualCapture::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("fonos: meeting capture init failed: {e}");
                meeting_js(&app_handle, &format!("recvMeetingError('{}')", js_escape(&e)));
                let mut ms = meeting_arc.lock().await;
                ms.recording = false;
                return;
            }
        };

        if let Err(e) = capture.start() {
            eprintln!("fonos: meeting capture start failed: {e}");
            meeting_js(&app_handle, &format!("recvMeetingError('{}')", js_escape(&e)));
            let mut ms = meeting_arc.lock().await;
            ms.recording = false;
            return;
        }

        let channel_mode = if capture.is_dual_channel() { "dual" } else { "mono" };
        eprintln!("fonos: meeting capture started, channel_mode={}", channel_mode);

        // Reconcile the container's initial metadata (optimistically written
        // as mic_only/mono in step 1, before capture negotiation completed)
        // with the actual channel mode now that it's known.
        if channel_mode == "dual" {
            if let Ok(db) = db_arc.lock() {
                let _ = db.execute(
                    "UPDATE containers SET metadata = json_patch(metadata, ?2), updated_at = ?3 WHERE id = ?1",
                    rusqlite::params![
                        container_id,
                        serde_json::json!({ "audio_source": "dual", "channel_mode": "dual" }).to_string(),
                        now_iso8601(),
                    ],
                );
            }
        }

        // Notify panel that capture started. Deliberately BEFORE the
        // diarization setup below (Task 6 fix): `recvStart` resets the whole
        // panel (`resetState()`), including hiding `#mb-status` — any
        // `recvDiarizeNotice` fired after a `recvStart` would have its
        // display immediately clobbered back to hidden by that reset, since
        // both evals run back-to-back with no yield in between. Ordering
        // `recvStart` first means the mono/no-model/spawn-failed notices
        // below land in an already-reset, already-visible-transcript panel
        // and stay on screen for their intended ~6s (recvDiarizeNotice's
        // setTimeout) instead of never rendering at all.
        meeting_js(&app_handle, &format!(
            "recvStart('{}', '{}')",
            js_escape(&title),
            if channel_mode == "dual" { "remote" } else { "in-person" },
        ));

        // Diarization (opt-in via MeetingProps::diarize): every failure mode
        // here — mono capture, helper/models missing, spawn error — degrades
        // to `diar = None` (flat "Audio" labeling downstream) and a
        // `recvDiarizeNotice` so the panel can show why, rather than ever
        // interrupting the meeting itself.
        let mut diar: Option<crate::audio::diarize::DiarizeSession> = None;
        if diarize {
            if channel_mode != "dual" {
                meeting_js(&app_handle, "recvDiarizeNotice('mono')");
            } else {
                let mdir = crate::audio::diarize::models_dir();
                match crate::audio::diarize::check(&mdir) {
                    Ok(st) if st.available && st.models_present => {
                        match crate::audio::diarize::DiarizeSession::spawn(&mdir) {
                            Ok(s) => { diar = Some(s); }
                            Err(e) => {
                                eprintln!("fonos: diarize spawn failed: {e}");
                                meeting_js(&app_handle, "recvDiarizeNotice('spawn-failed')");
                            }
                        }
                    }
                    Ok(_) => meeting_js(&app_handle, "recvDiarizeNotice('no-model')"),
                    Err(e) => {
                        eprintln!("fonos: diarize check failed: {e}");
                        meeting_js(&app_handle, "recvDiarizeNotice('no-model')");
                    }
                }
            }
        }
        let mut sys_total: u64 = 0; // 系统声道累计样本数 = 分离时间基准（16 样本/ms）
        let mut diar_notified_death = false;

        // Accumulate small chunks (500ms each) into 10-second transcription segments.
        // Mic and system audio are transcribed SEPARATELY so transcripts stay clean:
        //   mic    → speaker "Me"    (your voice)
        //   system → speaker "Audio" (remote participants / sound card)
        const POLL_MS: u64 = 500;
        const TARGET_SAMPLES: usize = 16000 * 10; // 10 seconds at 16kHz
        let mut mic_acc: Vec<i16> = Vec::new();
        let mut sys_acc: Vec<i16> = Vec::new();
        let mut local_counter: i32 = 0;

        loop {
            // Check if still recording
            {
                let ms = meeting_arc.lock().await;
                if !ms.recording { break; }
            }

            tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;

            let (still_recording, cid, start_time) = {
                let ms = meeting_arc.lock().await;
                (ms.recording, ms.container_id.unwrap_or(0), ms.start_time)
            };
            if !still_recording { break; }

            // Drain whatever mic/system audio is available (small chunks)
            if let Some(chunk) = capture.take_chunk(POLL_MS) {
                mic_acc.extend_from_slice(&chunk.mic_samples);
                if let Some(sys) = &chunk.system_samples {
                    sys_acc.extend_from_slice(sys);
                    sys_total += sys.len() as u64;
                    if let Some(d) = diar.as_mut() {
                        d.feed(sys);
                        if d.is_dead() && !diar_notified_death {
                            diar_notified_death = true;
                            meeting_js(&app_handle, "recvDiarizeNotice('died')");
                        }
                    }
                }
            }

            // Not enough accumulated on either channel — keep polling
            if mic_acc.len() < TARGET_SAMPLES && sys_acc.len() < TARGET_SAMPLES {
                continue;
            }

            // Collect whichever channels are ready for independent transcription.
            // Each entry: (samples, speaker_label, channel_tag, entry_role,
            // sample-clock range) — the range is the [start_ms, end_ms) this
            // chunk covers on the system-audio sample clock, used below to
            // query the diarizer's timeline for the dominant speaker; mic
            // audio has no diarization range (`None` — it's always "Me").
            let mut ready: Vec<(Vec<i16>, &str, &str, EntryRole, Option<(u64, u64)>)> = Vec::new();
            if mic_acc.len() >= TARGET_SAMPLES {
                ready.push((std::mem::take(&mut mic_acc), "Me", "mic", EntryRole::User, None));
            }
            if sys_acc.len() >= TARGET_SAMPLES {
                let end_ms = sys_total / 16;
                let start_ms = end_ms.saturating_sub(sys_acc.len() as u64 / 16);
                ready.push((std::mem::take(&mut sys_acc), "Audio", "system", EntryRole::User,
                            Some((start_ms, end_ms))));
            }

            let audio_dir = std::env::temp_dir().join("fonos_audio").join("meetings");
            let _ = std::fs::create_dir_all(&audio_dir);

            for (chunk_samples, speaker, channel_tag, role, range) in ready {
                // Dominant-speaker relabeling: when this chunk has a
                // diarization range (system audio only) and a live session,
                // ask the timeline for whichever speaker dominated that time
                // range — falling back to the flat "Audio"/"Me" label
                // whenever there's no range, no session, or no overlap
                // recorded yet (`dominant` returning `None`). `.to_lowercase()`
                // on a machine tag like "s1" is a no-op, so the existing
                // `speaker_hint` storage/display path downstream is unchanged.
                let speaker: String = match (range, diar.as_ref()) {
                    (Some((s, e)), Some(d)) => d.dominant(s, e).unwrap_or_else(|| speaker.to_string()),
                    _ => speaker.to_string(),
                };
                let speaker = speaker.as_str();

                eprintln!(
                    "fonos: meeting chunk [{}] ready: {} samples",
                    speaker, chunk_samples.len()
                );

                let pcm_bytes: Vec<u8> = chunk_samples
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
                if pcm_bytes.is_empty() { continue; }

                // Save temp WAV
                let audio_path = audio_dir.join(format!(
                    "meeting_{}_{}_{}.wav",
                    cid, local_counter, channel_tag
                ));
                if let Err(e) = write_wav(&audio_path, &pcm_bytes, 16000) {
                    eprintln!("fonos: meeting WAV write error: {e}");
                    continue;
                }

                let file_bytes = match std::fs::read(&audio_path) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("fonos: meeting WAV read error: {e}");
                        continue;
                    }
                };

                // Transcribe this channel independently
                let svc = ServiceConfig {
                    base_url: stt_base_url.clone(),
                    api_key: stt_api_key.clone(),
                    model: stt_model.clone(),
                    provider: "openai".to_string(),
                    stt_api: "whisper".to_string(),
                };
                // A meeting streams many chunks; a single failed chunk shouldn't
                // abort the session. Log and skip it (per-chunk error surfacing
                // would be a separate meetings-UI concern).
                let transcript = match transcribe_http(&svc, &file_bytes, &stt_model, "", "", 0.0, &[]).await {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("fonos: meeting chunk #{local_counter} STT error: {e}");
                        continue;
                    }
                };

                if transcript.is_empty() { continue; }

                eprintln!(
                    "fonos: [{}] chunk #{}: {}",
                    speaker, local_counter,
                    transcript.chars().take(80).collect::<String>()
                );

                // Session-relative timestamp
                let elapsed_secs = start_time
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                let ts = format!(
                    "{:02}:{:02}:{:02}",
                    elapsed_secs / 3600,
                    (elapsed_secs % 3600) / 60,
                    elapsed_secs % 60,
                );

                // Store entry
                let entry = Entry {
                    id: None,
                    created_at: now_iso8601(),
                    source_type: SourceType::Meeting,
                    role,
                    mode: "meeting".to_string(),
                    raw_text: transcript.clone(),
                    processed_text: None,
                    container_id: Some(cid),
                    audio_ref: Some(audio_path.to_string_lossy().into_owned()),
                    metadata: serde_json::json!({
                        "chunk_index": local_counter,
                        "speaker_hint": speaker.to_lowercase(),
                        "timestamp_in_session": ts,
                        "channel": channel_tag,
                        "duration_ms": 10000,
                    }),
                };

                if let Ok(db) = db_arc.lock() {
                    if let Err(e) = fonos_core::storage::insert_entry(&db, &entry) {
                        eprintln!("fonos: meeting entry insert error: {e}");
                    }
                }

                // Increment counters
                local_counter += 1;
                {
                    let mut ms = meeting_arc.lock().await;
                    ms.chunk_counter += 1;
                }

                // Notify panel — recvChunk(text, speaker, timestampMs)
                let esc_tx = js_escape(&transcript);
                let elapsed_ms = start_time
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                meeting_js(
                    &app_handle,
                    &format!("recvChunk('{}', '{}', {})", esc_tx, speaker, elapsed_ms),
                );
            }
        }

        capture.stop();
        if let Some(d) = diar.take() { d.shutdown(); }
        eprintln!("fonos: meeting chunk loop exited");
    });

    Ok(container_id)
}

/// Stop an active meeting session using the legacy `meeting_llm_profile`→
/// `llm_profile` config fallback chain and no custom summary prompt override
/// — the plain command-shell's fallback (no widget-ref awareness; see the
/// module docs). Still invoked directly by `meeting-panel.html`'s stop
/// button (`iv('stop_meeting')`), so this stays registered and behaviorally
/// unchanged from before this task for that caller.
///
/// Returns the generated summary text.
#[tauri::command(rename_all = "snake_case")]
pub async fn stop_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let llm_profile_id = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        if !config.meeting_llm_profile.is_empty() {
            config.meeting_llm_profile.clone()
        } else {
            config.llm_profile.clone()
        }
    };
    stop_meeting_with(&app, &state, llm_profile_id, String::new()).await
}

/// Halt the capture loop, compute total duration, generate an AI summary,
/// insert it as a system entry, and send it to the panel — the parameterized
/// guts shared by the plain `stop_meeting` command (legacy fallback chain,
/// builtin summary prompt) and `meeting_widget::MeetingOutput` (resolves
/// `llm_profile_id` from its own `llm_widget` ref, falling back to the same
/// `meeting_llm_profile`→`llm_profile` chain, and passes its own
/// `summary_prompt` prop as `summary_prompt_override`).
///
/// `summary_prompt_override` empty ⇒ [`build_summary_prompt`]'s built-in
/// literal (unchanged pre-Task-7 behavior); non-empty replaces it.
///
/// Also schedules the meeting-panel's delayed hide (moved here from the
/// retired `main.rs` hotkey arm — see [`schedule_delayed_hide`]) once the
/// recording has genuinely stopped, regardless of what happens afterward.
///
/// Returns the generated summary text.
pub(crate) async fn stop_meeting_with(
    app: &tauri::AppHandle,
    state: &AppState,
    llm_profile_id: String,
    summary_prompt_override: String,
) -> Result<String, String> {
    let (container_id, start_time) = {
        let mut ms = state.meeting.lock().await;
        if !ms.recording {
            return Err("No meeting in progress".into());
        }
        ms.recording = false;
        (ms.container_id.take(), ms.start_time.take())
    };

    // From here on the recording has genuinely stopped — schedule the
    // panel's delayed hide unconditionally, mirroring the legacy hotkey arm's
    // "hide after a beat" which ran regardless of what follows below.
    schedule_delayed_hide(app.clone());

    let Some(cid) = container_id else {
        return Ok(String::new());
    };

    let duration_ms = start_time
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);

    eprintln!("fonos: stop_meeting container_id={} duration={}ms", cid, duration_ms);

    // Update container metadata with final duration
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let _ = db.execute(
            "UPDATE containers SET metadata = json_patch(metadata, ?2), updated_at = ?3 WHERE id = ?1",
            rusqlite::params![
                cid,
                serde_json::json!({ "duration_total_ms": duration_ms }).to_string(),
                now_iso8601(),
            ],
        );
    }

    // Give the chunk loop a moment to finish its current iteration
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Fetch all transcript entries for this session
    let entries: Vec<Entry> = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        fonos_core::storage::get_container_entries(&db, cid).map_err(|e| e.to_string())?
    };

    if entries.is_empty() {
        meeting_js(app, "recvSummary('')");
        return Ok(String::new());
    }

    // Build summary and call LLM. The empty-prompt short-circuit happens
    // BEFORE profile resolution (matching the original `call_llm_raw`'s
    // ordering) so a meeting with no real transcript content never errors on
    // a missing LLM profile.
    let prompt = build_summary_prompt(&entries, &summary_prompt_override);
    let summary_text = if prompt.is_empty() {
        String::new()
    } else {
        let svc_result = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            resolve_llm_profile_service(&config, &llm_profile_id)
        };
        match svc_result {
            Ok(svc) => match call_llm_raw(&svc, &llm_profile_id, &prompt).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("fonos: meeting summary LLM error: {e}");
                    format!("[Summary generation failed: {}]", e)
                }
            },
            Err(e) => {
                eprintln!("fonos: meeting summary LLM error: {e}");
                format!("[Summary generation failed: {}]", e)
            }
        }
    };

    eprintln!("fonos: meeting summary generated ({} chars)", summary_text.len());

    // Insert summary as a system entry
    if !summary_text.is_empty() {
        let summary_entry = Entry {
            id: None,
            created_at: now_iso8601(),
            source_type: SourceType::Meeting,
            role: EntryRole::System,
            mode: "meeting".to_string(),
            raw_text: summary_text.clone(),
            processed_text: None,
            container_id: Some(cid),
            audio_ref: None,
            metadata: serde_json::json!({
                "source_entries": entries.len(),
                "generation_model": "llm",
            }),
        };
        if let Ok(db) = state.db.lock() {
            let _ = fonos_core::storage::insert_entry(&db, &summary_entry);
        }

        // Mark summary_generated in container
        if let Ok(db) = state.db.lock() {
            let _ = db.execute(
                "UPDATE containers SET metadata = json_patch(metadata, ?2), updated_at = ?3 WHERE id = ?1",
                rusqlite::params![
                    cid,
                    serde_json::json!({ "summary_generated": true }).to_string(),
                    now_iso8601(),
                ],
            );
        }
    }

    let esc = js_escape(&summary_text);
    meeting_js(app, &format!("recvSummary('{}')", esc));

    Ok(summary_text)
}

/// List all meeting session containers.
#[tauri::command(rename_all = "snake_case")]
pub fn get_meetings(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Container>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let all = fonos_core::storage::get_containers(&db).map_err(|e| e.to_string())?;
    let meetings: Vec<Container> = all
        .into_iter()
        .filter(|c| c.container_type == ContainerType::MeetingSession)
        .collect();
    Ok(meetings)
}

/// Return full detail for a single meeting session.
#[tauri::command(rename_all = "snake_case")]
pub fn get_meeting_detail(
    state: tauri::State<'_, AppState>,
    container_id: i64,
) -> Result<MeetingDetail, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let container = fonos_core::storage::get_container(&db, container_id)
        .map_err(|e| e.to_string())?;

    let all_entries = fonos_core::storage::get_container_entries(&db, container_id)
        .map_err(|e| e.to_string())?;

    let summary = all_entries
        .iter()
        .find(|e| e.role == EntryRole::System)
        .cloned();

    let entries: Vec<Entry> = all_entries
        .into_iter()
        .filter(|e| e.role != EntryRole::System)
        .collect();

    Ok(MeetingDetail {
        container,
        entries,
        summary,
    })
}

/// Hide the meeting panel and stop any active recording.
#[tauri::command(rename_all = "snake_case")]
pub fn hide_meeting_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let meeting_arc = Arc::clone(&state.meeting);
    tauri::async_runtime::spawn(async move {
        let mut ms = meeting_arc.lock().await;
        ms.recording = false;
    });

    if let Some(panel) = app.get_webview_window("meeting-panel") {
        let _ = panel.hide();
    }
    Ok(())
}

/// Export a meeting session as a Markdown file.
#[tauri::command(rename_all = "snake_case")]
pub fn export_meeting_md(
    state: tauri::State<'_, AppState>,
    container_id: i64,
    output_dir: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(&output_dir);
    let result = fonos_core::storage::export_notebook_markdown(&db, container_id, &dir)
        .map_err(|e| e.to_string())?;
    Ok(result.to_string_lossy().into_owned())
}

/// Export a meeting session as a JSON file.
#[tauri::command(rename_all = "snake_case")]
pub fn export_meeting_json(
    state: tauri::State<'_, AppState>,
    container_id: i64,
    output_dir: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(&output_dir);
    let result = fonos_core::storage::export_notebook_json(&db, container_id, &dir)
        .map_err(|e| e.to_string())?;
    Ok(result.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> Entry {
        Entry {
            id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            source_type: SourceType::Meeting,
            role: EntryRole::User,
            mode: "meeting".to_string(),
            raw_text: text.to_string(),
            processed_text: None,
            container_id: Some(1),
            audio_ref: None,
            metadata: serde_json::json!({ "speaker_hint": "me", "timestamp_in_session": "00:00:01" }),
        }
    }

    #[test]
    fn build_summary_prompt_empty_entries_yields_empty_prompt() {
        assert_eq!(build_summary_prompt(&[], ""), "");
        // Even a non-empty custom prompt doesn't matter — no transcript means
        // nothing to summarize.
        assert_eq!(build_summary_prompt(&[], "Custom instructions."), "");
    }

    #[test]
    fn build_summary_prompt_empty_custom_instructions_uses_builtin_literal() {
        let entries = vec![entry("hello")];
        let prompt = build_summary_prompt(&entries, "");
        assert!(prompt.starts_with(BUILTIN_SUMMARY_INSTRUCTIONS));
        assert!(prompt.contains("TRANSCRIPT:"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn build_summary_prompt_custom_instructions_replace_builtin_literal() {
        let entries = vec![entry("hello")];
        let prompt = build_summary_prompt(&entries, "Summarize action items only.");
        assert!(prompt.starts_with("Summarize action items only."));
        assert!(!prompt.contains(BUILTIN_SUMMARY_INSTRUCTIONS));
        assert!(prompt.contains("TRANSCRIPT:"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn default_meeting_title_starts_with_meeting_prefix() {
        assert!(default_meeting_title(fonos_core::workflow::builtin::Lang::En).starts_with("Meeting "));
    }

    /// Workbench P2 Task 13: the ZH-language title uses the same builtin
    /// display name (`meeting.default` → "会议") the meeting composite is
    /// named with in `builtin.rs`, rather than a hardcoded English literal.
    #[test]
    fn default_meeting_title_localizes_for_zh() {
        assert!(default_meeting_title(fonos_core::workflow::builtin::Lang::Zh).starts_with("会议 "));
    }

    #[test]
    fn resolve_llm_profile_service_empty_profile_id_errors() {
        let config = fonos_core::config::AppConfig::default();
        let err = resolve_llm_profile_service(&config, "").unwrap_err();
        assert!(err.contains("No LLM profile configured"));
    }

    #[test]
    fn resolve_llm_profile_service_unknown_profile_id_errors() {
        let config = fonos_core::config::AppConfig::default();
        let err = resolve_llm_profile_service(&config, "does-not-exist").unwrap_err();
        assert!(err.contains("does-not-exist"));
    }

    #[test]
    fn resolve_llm_profile_service_known_profile_resolves() {
        let mut config = fonos_core::config::AppConfig::default();
        config.model_profiles.push(serde_json::json!({
            "id": "p1", "provider": "openai", "model": "gpt-4o", "api_key": "k", "base_url": "",
        }));
        let svc = resolve_llm_profile_service(&config, "p1").expect("known profile resolves");
        assert_eq!(svc.provider, "openai");
        assert_eq!(svc.model, "gpt-4o");
        assert_eq!(svc.api_key, "k");
    }

    // ── diarization wiring (Task 3) ───────────────────────────────────────────

    #[test]
    fn meeting_props_diarize_defaults_false() {
        let p: super::super::meeting_widget::MeetingProps = serde_json::from_str(
            r#"{"stt_widget":"","llm_widget":"","summary_prompt":""}"#,
        )
        .unwrap();
        assert!(!p.diarize);
    }

    #[test]
    fn speaker_display_hint_mapping() {
        assert_eq!(speaker_display_hint("me"), "Me");
        assert_eq!(speaker_display_hint("s1"), "s1");
        assert_eq!(speaker_display_hint("s10"), "s10");
        assert_eq!(speaker_display_hint("audio"), "Audio");
        assert_eq!(speaker_display_hint("sx"), "Audio"); // 非法尾缀不透传
        assert_eq!(speaker_display_hint(""), "Audio");
    }
}

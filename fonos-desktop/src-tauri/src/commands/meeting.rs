//! Tauri commands for meeting mode.
//!
//! Meeting mode captures audio continuously (dual-channel when ScreenCaptureKit
//! is available, mic-only otherwise), transcribes each chunk via the configured
//! STT engine, streams results to the meeting-panel webview, and generates an
//! AI summary when the meeting ends.

use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;

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

/// Escape a string for safe interpolation inside a JS single-quoted string.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Helper: evaluate JS in the meeting-panel webview.
fn meeting_js(app: &tauri::AppHandle, js: &str) {
    if let Some(panel) = app.get_webview_window("meeting-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: meeting-panel JS eval error: {e}");
        }
    }
}

/// Build the AI summary prompt from a list of transcript entries.
pub fn build_summary_prompt(entries: &[Entry]) -> String {
    let transcript = entries
        .iter()
        .enumerate()
        .map(|(_i, e)| {
            let speaker = e
                .metadata
                .get("speaker_hint")
                .and_then(|v| v.as_str())
                .map(|s| if s == "me" { "Me" } else { "Audio" })
                .unwrap_or("Me");
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

    format!(
        "You are a meeting assistant. Given the following transcript, generate:\n\
         1. A concise meeting summary (3-5 sentences)\n\
         2. Key discussion points (bullet list grouped by topic)\n\
         3. Action items (who does what, by when)\n\
         4. Decisions made during the meeting\n\n\
         Output in clean Markdown. Do not wrap in a code block.\n\n\
         TRANSCRIPT:\n{transcript}"
    )
}

/// Call an LLM with a raw prompt string using the meeting or fallback LLM profile.
/// Returns the generated text or an error string.
async fn call_llm_raw(state: &AppState, prompt: &str) -> Result<String, String> {
    use fonos_core::llm::call_openai_compatible;
    use fonos_core::llm::call_anthropic;

    if prompt.is_empty() {
        return Ok(String::new());
    }

    let (profile_id, model, api_key, base_url, provider) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        // Prefer meeting_llm_profile; fall back to llm_profile
        let pid = if !config.meeting_llm_profile.is_empty() {
            config.meeting_llm_profile.clone()
        } else {
            config.llm_profile.clone()
        };
        if pid.is_empty() {
            return Err("No LLM profile configured. Please add one in Settings > Model Registry.".into());
        }
        let profile = config
            .model_profiles
            .iter()
            .find(|p| p["id"].as_str() == Some(&pid))
            .ok_or_else(|| format!("LLM profile '{}' not found", pid))?
            .clone();
        (
            pid,
            profile["model"].as_str().unwrap_or("gpt-4o").to_string(),
            profile["api_key"].as_str().unwrap_or("").to_string(),
            profile["base_url"].as_str().unwrap_or("").to_string(),
            profile["provider"].as_str().unwrap_or("openai").to_string(),
        )
    };

    eprintln!("fonos: meeting summary LLM profile={} provider={} model={}", profile_id, provider, model);

    let messages = vec![
        serde_json::json!({"role": "user", "content": prompt}),
    ];

    let resp = if provider == "anthropic" {
        call_anthropic(&api_key, &model, &messages, 0.3, 2048).await
    } else {
        call_openai_compatible(&api_key, &model, &base_url, &messages, 0.3, 2048, &provider).await
    };

    resp.map(|r| r.text).map_err(|e| e.to_string())
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Start a new meeting session.
///
/// Creates a `meeting_session` container, sets up dual-channel capture, and
/// spawns a background task that transcribes audio chunks and streams results
/// to the `meeting-panel` webview.
///
/// Returns the container ID of the new meeting session.
#[tauri::command(rename_all = "snake_case")]
pub async fn start_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
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
    let title = {
        let ts = &now[..16].replace('T', " ");
        format!("Meeting {}", ts)
    };

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

    // 3. Read STT config
    let (stt_base_url, stt_api_key, stt_model) = {
        let svc = super::get_service_config(&state, "stt");
        (svc.base_url, svc.api_key, if svc.model.is_empty() { "fast".to_string() } else { svc.model })
    };

    // 4. Spawn the chunk-transcription loop
    let app_handle = app.clone();
    let meeting_arc = Arc::clone(&state.meeting);
    let db_arc = Arc::clone(&state.db);

    tauri::async_runtime::spawn(async move {
        use crate::audio::dual_capture::DualCapture;
        use fonos_core::audio::write_wav;
        use crate::commands::dictation::transcribe_http;
        use super::ServiceConfig;

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

        // Notify panel that capture started
        meeting_js(&app_handle, &format!(
            "recvStart('{}', '{}')",
            js_escape(&title),
            if channel_mode == "dual" { "remote" } else { "in-person" },
        ));

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
                }
            }

            // Not enough accumulated on either channel — keep polling
            if mic_acc.len() < TARGET_SAMPLES && sys_acc.len() < TARGET_SAMPLES {
                continue;
            }

            // Collect whichever channels are ready for independent transcription.
            // Each entry: (samples, speaker_label, channel_tag, entry_role)
            let mut ready: Vec<(Vec<i16>, &str, &str, EntryRole)> = Vec::new();
            if mic_acc.len() >= TARGET_SAMPLES {
                ready.push((std::mem::take(&mut mic_acc), "Me", "mic", EntryRole::User));
            }
            if sys_acc.len() >= TARGET_SAMPLES {
                ready.push((std::mem::take(&mut sys_acc), "Audio", "system", EntryRole::User));
            }

            let audio_dir = std::env::temp_dir().join("fonos_audio").join("meetings");
            let _ = std::fs::create_dir_all(&audio_dir);

            for (chunk_samples, speaker, channel_tag, role) in ready {
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
                let transcript = match transcribe_http(&svc, &file_bytes, &stt_model, "", None, &[], None).await {
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
        eprintln!("fonos: meeting chunk loop exited");
    });

    Ok(container_id)
}

/// Stop an active meeting session.
///
/// Halts the capture loop, computes total duration, generates an AI summary,
/// inserts it as a system entry, and sends it to the panel.
///
/// Returns the generated summary text.
#[tauri::command(rename_all = "snake_case")]
pub async fn stop_meeting(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let (container_id, start_time) = {
        let mut ms = state.meeting.lock().await;
        if !ms.recording {
            return Err("No meeting in progress".into());
        }
        ms.recording = false;
        (ms.container_id.take(), ms.start_time.take())
    };

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
        meeting_js(&app, "recvSummary('')");
        return Ok(String::new());
    }

    // Build summary and call LLM
    let prompt = build_summary_prompt(&entries);
    let summary_text = match call_llm_raw(&state, &prompt).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("fonos: meeting summary LLM error: {e}");
            format!("[Summary generation failed: {}]", e)
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
    meeting_js(&app, &format!("recvSummary('{}')", esc));

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

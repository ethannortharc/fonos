//! Tauri commands for the meeting speaker-diarization sidecar.

use crate::audio::diarize;
use crate::commands::AppState;
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Serialize)]
pub struct DiarizeStatusDto { pub available: bool, pub models_present: bool }

#[tauri::command(rename_all = "snake_case")]
pub async fn diarize_check() -> Result<DiarizeStatusDto, String> {
    let dir = diarize::models_dir();
    match tauri::async_runtime::spawn_blocking(move || diarize::check(&dir)).await {
        Ok(Ok(st)) => Ok(DiarizeStatusDto { available: st.available, models_present: st.models_present }),
        Ok(Err(_)) => Ok(DiarizeStatusDto { available: false, models_present: false }), // helper 缺失=不可用，非错误
        Err(e) => Err(format!("join: {e}")),
    }
}

fn emit_download(app: &tauri::AppHandle, payload: serde_json::Value) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("diarize:download", payload.to_string());
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn diarize_download_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let endpoint = state.config.lock().map(|c| c.diarization_hf_endpoint.clone()).unwrap_or_default();
    let dir = diarize::models_dir();
    let bin = diarize::find_diarize_binary().ok_or("fonos-diarize binary not found")?;
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let mut cmd = Command::new(&bin);
        cmd.args(["download-models", "--models-dir"]).arg(&dir);
        if !endpoint.is_empty() { cmd.args(["--endpoint", &endpoint]); }
        let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("spawn: {e}"))?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut finished_ok = false;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            match v.get("type").and_then(|t| t.as_str()) {
                Some("progress") => emit_download(&app2, serde_json::json!({
                    "kind": "progress", "pct": v.get("pct").and_then(|p| p.as_u64()).unwrap_or(0) })),
                Some("done") => { finished_ok = true; emit_download(&app2, serde_json::json!({"kind": "done"})); }
                Some("error") => {
                    let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("download failed").to_string();
                    emit_download(&app2, serde_json::json!({"kind": "error", "message": msg.clone()}));
                    return Err(msg);
                }
                _ => {}
            }
        }
        let _ = child.wait();
        if finished_ok { Ok(()) } else {
            emit_download(&app2, serde_json::json!({"kind": "error", "message": "download ended without done"}));
            Err("download ended without done".into())
        }
    }).await.map_err(|e| format!("join: {e}"))?
}

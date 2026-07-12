//! Scenario-based setup commands (issue #29).
//!
//! Thin network / filesystem shell around the pure logic in
//! [`fonos_core::scenarios`]:
//!
//! * [`scan_models`] — probe a server's `/v1/models` (used both by the card's
//!   "detected ✓" check and by the step-2 probe).
//! * [`scenario_probe`] — scan + classify + measure TTS RTFs + build a default
//!   [`ModelPlan`] in one call.
//! * [`save_scenario`] / [`apply_saved_scenario`] / [`delete_saved_scenario`] —
//!   manage the `saved_scenarios` bundle list in the persisted config.
//! * [`export_scenario`] / [`import_scenario`] / [`import_scenario_json`] —
//!   share a bundle as a JSON file between machines (no dialog plugin needed).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::Serialize;

use fonos_core::llm::ServiceConfig;
use fonos_core::scenarios::{self, ClassifiedModels, ModelPlan, SavedScenario};

use super::AppState;

// ── /v1/models scanning ─────────────────────────────────────────────────────

/// Result of probing a server's `/v1/models` endpoint.
#[derive(Serialize)]
pub struct ScanResult {
    /// Whether the server answered at all (any HTTP status counts as reachable).
    pub reachable: bool,
    /// Round-trip latency in milliseconds (0 when unreachable).
    pub latency_ms: u64,
    /// Model ids advertised by the server.
    pub models: Vec<String>,
}

/// Parse model ids from an OpenAI-style `{ "data": [...] }` body or a flat array.
fn parse_model_ids(json: &serde_json::Value) -> Vec<String> {
    let arr = json["data"].as_array().or_else(|| json.as_array());
    match arr {
        Some(items) => items
            .iter()
            .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        None => Vec::new(),
    }
}

/// GET `<base>/v1/models` with a bounded timeout. Returns `(reachable, latency_ms, models)`.
async fn fetch_models(base_url: &str, api_key: &str, timeout: Duration) -> (bool, u64, Vec<String>) {
    let base = base_url.trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(_) => return (false, 0, Vec::new()),
    };
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let t0 = Instant::now();
    match req.send().await {
        Ok(resp) => {
            let ms = t0.elapsed().as_millis() as u64;
            if !resp.status().is_success() {
                // Reachable but errored (e.g. 401) — no model list available.
                return (true, ms, Vec::new());
            }
            let json: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
            (true, ms, parse_model_ids(&json))
        }
        Err(_) => (false, 0, Vec::new()),
    }
}

/// Probe a server's `/v1/models` endpoint (3s timeout).
#[tauri::command]
pub async fn scan_models(base_url: String, api_key: String) -> Result<ScanResult, String> {
    let (reachable, latency_ms, models) =
        fetch_models(&base_url, &api_key, Duration::from_secs(3)).await;
    Ok(ScanResult { reachable, latency_ms, models })
}

/// The step-2 probe result: connectivity + classified candidates + measured TTS
/// RTFs + a default [`ModelPlan`].
#[derive(Serialize)]
pub struct ScenarioProbe {
    /// Whether the server answered.
    pub reachable: bool,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
    /// All advertised model ids.
    pub models: Vec<String>,
    /// STT / LLM / TTS candidate buckets.
    pub classified: ClassifiedModels,
    /// Measured real-time factor per TTS candidate that responded.
    pub tts_rtfs: BTreeMap<String, f64>,
    /// The auto-assigned default plan.
    pub plan: ModelPlan,
}

/// Scan, classify, measure TTS speeds, and build a default plan for a server.
#[tauri::command]
pub async fn scenario_probe(
    base_url: String,
    api_key: String,
    voice: Option<String>,
) -> Result<ScenarioProbe, String> {
    let (reachable, latency_ms, models) =
        fetch_models(&base_url, &api_key, Duration::from_secs(5)).await;
    let classified = scenarios::classify_models(&models);

    let mut tts_rtfs = BTreeMap::new();
    if reachable {
        let voice = voice.unwrap_or_else(|| "default".to_string());
        // Bound the work: measure at most a handful of TTS candidates.
        for m in classified.tts.iter().take(4) {
            let svc = ServiceConfig {
                provider: String::new(),
                api_key: api_key.clone(),
                model: m.clone(),
                base_url: base_url.clone(),
                stt_api: String::new(),
            };
            if let Some(rtf) = super::doctor::measure_tts_rtf(&svc, &voice).await {
                tts_rtfs.insert(m.clone(), rtf);
            }
        }
    }

    let plan = scenarios::build_plan(&classified, &tts_rtfs);
    Ok(ScenarioProbe { reachable, latency_ms, models, classified, tts_rtfs, plan })
}

// ── saved-scenario management ────────────────────────────────────────────────

/// Persist the config after mutating it under the state lock.
fn save_config(state: &AppState) -> Result<(), String> {
    let guard = state.config.lock().map_err(|e| e.to_string())?;
    guard.save().map_err(|e| format!("failed to save config: {e}"))
}

/// Snapshot the live config into a new sectioned [`SavedScenario`], append it,
/// and persist. The `include_*` flags choose which sections are captured; the
/// dictation section (when included) now carries `config.workflows` /
/// `config.widgets` verbatim (Workbench P2 Task 11) rather than reading
/// `modes.json` — the engine-world overlays are the user's real customization
/// footprint, and modes.json hasn't been read at snapshot time since.
#[tauri::command]
pub fn save_scenario(
    state: tauri::State<'_, AppState>,
    name: String,
    include_models: bool,
    include_dictation: bool,
    include_speech: bool,
    include_vocab: bool,
    include_hotkeys: bool,
) -> Result<SavedScenario, String> {
    let scenario = {
        let mut guard = state.config.lock().map_err(|e| e.to_string())?;
        let scenario = scenarios::snapshot_current(
            &guard,
            name.trim(),
            include_models,
            include_dictation,
            include_speech,
            include_vocab,
            include_hotkeys,
        );
        guard.saved_scenarios.push(scenario.clone());
        scenario
    };
    save_config(&state)?;
    Ok(scenario)
}

/// Apply a saved scenario by id: restore the sections it carries. Core mutates
/// the config in full — profiles/assignments + speech + vocab + hotkeys +
/// dictation config fields, upserting `user_workflows`/`user_widgets` (and
/// converting a legacy `user_modes` map, if the scenario predates Workbench
/// P2 Task 11, into `llm.*` processor widgets) into `config.workflows` /
/// `config.widgets` — `modes.json` is never touched here. When a hotkeys
/// section is applied we emit `hotkey:reload` so the global hotkey manager
/// re-registers the new bindings live (same path Settings uses).
#[tauri::command]
pub fn apply_saved_scenario(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let hotkeys_applied = {
        let mut guard = state.config.lock().map_err(|e| e.to_string())?;
        let scenario = guard
            .saved_scenarios
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| format!("saved scenario '{id}' not found"))?;
        let hotkeys_applied = scenario.hotkeys.is_some();
        scenarios::apply_saved(&mut guard, &scenario);
        hotkeys_applied
    };
    save_config(&state)?;

    // Re-register global hotkeys if the applied scenario carried a hotkeys
    // section (config fields are already saved above).
    if hotkeys_applied {
        use tauri::Emitter;
        let _ = app.emit("hotkey:reload", ());
    }
    Ok(())
}

/// Delete a saved scenario by id.
#[tauri::command]
pub fn delete_saved_scenario(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let mut guard = state.config.lock().map_err(|e| e.to_string())?;
        guard.saved_scenarios.retain(|s| s.id != id);
    }
    save_config(&state)
}

// ── import / export ──────────────────────────────────────────────────────────

/// Write a scenario JSON blob to `~/Downloads/fonos-scenario-<slug>.json` and
/// return the full path. The frontend builds `scenario_json` (optionally with
/// api keys stripped) so key-handling policy stays in one place.
#[tauri::command]
pub fn export_scenario(scenario_json: String, name: String) -> Result<String, String> {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not resolve a Downloads directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("fonos-scenario-{}.json", scenarios::scenario_slug(&name)));
    std::fs::write(&file, scenario_json).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().to_string())
}

/// Validate an imported scenario JSON string (giving it a fresh id), append it
/// to `saved_scenarios`, persist, and return it. Not auto-applied.
#[tauri::command]
pub fn import_scenario_json(
    state: tauri::State<'_, AppState>,
    json: String,
) -> Result<SavedScenario, String> {
    let scenario = scenarios::parse_saved_scenario(&json)?;
    {
        let mut guard = state.config.lock().map_err(|e| e.to_string())?;
        guard.saved_scenarios.push(scenario.clone());
    }
    save_config(&state)?;
    Ok(scenario)
}

/// Read a scenario JSON file from `path`, then import it via [`import_scenario_json`].
#[tauri::command]
pub fn import_scenario(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<SavedScenario, String> {
    let json = std::fs::read_to_string(path.trim())
        .map_err(|e| format!("could not read file: {e}"))?;
    import_scenario_json(state, json)
}

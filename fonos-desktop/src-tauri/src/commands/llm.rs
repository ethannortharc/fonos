//! LLM commands — thin Tauri wrappers around fonos_core::llm and fonos_core::model_caps.

use serde::Serialize;
use super::AppState;
use fonos_core::modes;
use fonos_core::model_caps;
use fonos_core::llm::{ServiceConfig, process_text};

#[derive(Serialize)]
pub struct LlmResult {
    pub original: String,
    pub processed: String,
    pub mode: String,
    pub mode_name: String,
    pub latency_ms: u64,
    pub auto_paste: bool,
    pub auto_press_enter: bool,
}

/// Process text through the configured LLM using the specified mode.
#[tauri::command]
pub async fn process_with_llm(
    state: tauri::State<'_, AppState>,
    text: String,
    mode: String,
) -> Result<LlmResult, String> {
    if text.is_empty() {
        return Ok(LlmResult { original: text, processed: String::new(), mode, mode_name: String::new(), latency_ms: 0, auto_paste: true, auto_press_enter: false });
    }

    // Look up mode definition
    let all_modes = modes::all_modes();
    let mode_def = all_modes.get(&mode)
        .ok_or_else(|| format!("Unknown mode: '{}'. Available: {}", mode, all_modes.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")))?;

    // Raw mode — skip LLM
    if mode_def.system.is_none() && mode_def.user_template.is_none() {
        return Ok(LlmResult {
            original: text.clone(), processed: text, mode,
            mode_name: mode_def.name.clone(), latency_ms: 0,
            auto_paste: mode_def.auto_paste, auto_press_enter: mode_def.auto_press_enter,
        });
    }

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();

    // Check mode's LLM model override; fall back to global LLM profile
    let profile_id = if !mode_def.model.is_empty() {
        &mode_def.model
    } else {
        &config.llm_profile
    };
    if profile_id.is_empty() {
        return Err("No LLM profile configured. Go to Settings > Model Registry to add one.".into());
    }

    let profile = config.model_profiles.iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| format!("LLM profile '{}' not found", profile_id))?
        .clone();

    let provider = profile["provider"].as_str().unwrap_or("openai").to_string();
    let api_key = profile["api_key"].as_str().unwrap_or("").to_string();
    let model = profile["model"].as_str().unwrap_or("gpt-4o").to_string();
    let base_url = profile["base_url"].as_str().unwrap_or("").to_string();

    if api_key.is_empty() && provider != "ollama" && provider != "omlx" {
        return Err(format!("No API key in profile '{}'. Edit the profile in Settings.", profile_id));
    }

    let caps = model_caps::get_caps(&model);

    eprintln!("fonos: LLM mode={} ({}) provider={} model={} caps={}",
        mode, mode_def.name, provider, model,
        caps.as_ref().map(|c| format!("sys:{} lang:{}", c.follows_system_prompt, c.preserves_language)).unwrap_or("unprobed".into()));

    let service = ServiceConfig { provider, api_key, model: model.clone(), base_url };
    let translate_target = config.translate_target.clone();

    let t0 = std::time::Instant::now();
    let resp = process_text(&text, mode_def, &service, caps.as_ref(), &translate_target)
        .await
        .map_err(|e| e.to_string())?;
    let latency_ms = t0.elapsed().as_millis() as u64;

    eprintln!("fonos: LLM response ({}ms, {}+{}tok): {}", latency_ms,
        resp.tokens_in, resp.tokens_out, resp.text.chars().take(80).collect::<String>());

    // Record LLM event to stats DB
    if let Ok(db) = state.db.lock() {
        let _ = fonos_core::stats::record_event(
            &db, "llm", &text, &resp.text, 0.0,
            latency_ms as i64, &mode, &model, "", "",
            resp.tokens_in, resp.tokens_out, "",
        );
    }

    Ok(LlmResult {
        original: text,
        processed: resp.text,
        mode: mode.clone(),
        mode_name: mode_def.name.clone(),
        latency_ms,
        auto_paste: mode_def.auto_paste,
        auto_press_enter: mode_def.auto_press_enter,
    })
}

/// Probe a model's capabilities and cache results.
#[tauri::command]
pub async fn probe_model(
    state: tauri::State<'_, AppState>,
) -> Result<model_caps::ModelCaps, String> {
    use fonos_core::llm::{call_openai_compatible, call_anthropic};

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let profile_id = &config.llm_profile;
    if profile_id.is_empty() {
        return Err("No LLM profile configured".into());
    }

    let profile = config.model_profiles.iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or("LLM profile not found")?
        .clone();

    let provider = profile["provider"].as_str().unwrap_or("openai");
    let api_key = profile["api_key"].as_str().unwrap_or("");
    let model = profile["model"].as_str().unwrap_or("gpt-4o");
    let base_url = profile["base_url"].as_str().unwrap_or("");

    eprintln!("fonos: probing model {} ...", model);

    // Test 1: system prompt following
    let test1_msgs = vec![
        serde_json::json!({"role": "system", "content": "Reply with exactly one word: BLUE"}),
        serde_json::json!({"role": "user", "content": "What is your favorite color?"}),
    ];
    let resp1 = match provider {
        "anthropic" => call_anthropic(api_key, model, &test1_msgs, 0.0, 256).await,
        _ => call_openai_compatible(api_key, model, base_url, &test1_msgs, 0.0, 256, provider).await,
    };
    let follows_system = resp1.map(|r| r.text.trim().to_uppercase().contains("BLUE")).unwrap_or(false);

    // Test 2: language preservation
    let test2_msgs = vec![
        serde_json::json!({"role": "system", "content": "Clean the text. Remove filler words. Keep original language. Output only cleaned text."}),
        serde_json::json!({"role": "user", "content": "\"嗯，这个测试可以吗？\""}),
    ];
    let resp2 = match provider {
        "anthropic" => call_anthropic(api_key, model, &test2_msgs, 0.0, 256).await,
        _ => call_openai_compatible(api_key, model, base_url, &test2_msgs, 0.0, 256, provider).await,
    };
    let preserves_language = resp2.map(|r| r.text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))).unwrap_or(false);

    let caps = model_caps::ModelCaps {
        model_id: model.to_string(),
        follows_system_prompt: follows_system,
        preserves_language,
        probed_at: format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
    };

    eprintln!("fonos: probe result — follows_system={} preserves_language={}", follows_system, preserves_language);
    model_caps::store_caps(caps.clone()).map_err(|e| e.to_string())?;

    Ok(caps)
}

/// Query a provider's /v1/models endpoint and return available model IDs.
#[tauri::command]
pub async fn list_provider_models(
    base_url: String,
    api_key: String,
) -> Result<Vec<serde_json::Value>, String> {
    let url = {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/models", base)
        } else {
            format!("{}/v1/models", base)
        }
    };

    eprintln!("fonos: probing models at {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("http client error: {e}"))?;

    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let resp = req.send().await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body.chars().take(200).collect::<String>()));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("parse error: {e}"))?;

    // OpenAI format: { "data": [ { "id": "model-name", ... }, ... ] }
    // Some servers return a flat array.
    let models = if let Some(arr) = json["data"].as_array() {
        arr.iter().map(|m| {
            serde_json::json!({
                "id": m["id"].as_str().unwrap_or(""),
                "owned_by": m["owned_by"].as_str().unwrap_or(""),
            })
        }).filter(|m| !m["id"].as_str().unwrap_or("").is_empty()).collect()
    } else if let Some(arr) = json.as_array() {
        arr.iter().map(|m| {
            serde_json::json!({
                "id": m["id"].as_str().or_else(|| m["name"].as_str()).unwrap_or(""),
                "owned_by": m["owned_by"].as_str().unwrap_or(""),
            })
        }).filter(|m| !m["id"].as_str().unwrap_or("").is_empty()).collect()
    } else {
        Vec::new()
    };

    eprintln!("fonos: found {} models", models.len());
    Ok(models)
}

/// List all modes (built-in + custom).
#[tauri::command]
pub fn list_modes() -> Result<serde_json::Value, String> {
    let all = modes::all_modes();
    let built_in_keys: Vec<String> = modes::built_in_modes().keys().cloned().collect();

    let list: Vec<serde_json::Value> = all.iter().map(|(id, m)| {
        serde_json::json!({
            "id": id,
            "name": m.name,
            "description": m.description,
            "icon": m.icon,
            "builtin": built_in_keys.contains(id),
            "system": m.system,
            "user_template": m.user_template,
            "temperature": m.temperature,
            "model": m.model,
            "stt_model": m.stt_model,
            "stt_prompt": m.stt_prompt,
            "stt_temperature": m.stt_temperature,
            "max_tokens": m.max_tokens,
            "output_language": m.output_language,
            "auto_paste": m.auto_paste,
            "auto_press_enter": m.auto_press_enter,
        })
    }).collect();

    Ok(serde_json::json!(list))
}

/// Save a custom mode.
#[tauri::command]
pub fn save_custom_mode(
    id: String,
    name: String,
    description: String,
    icon: String,
    system: String,
    user_template: String,
    temperature: f64,
    model: String,
    stt_model: String,
    stt_prompt: String,
    stt_temperature: f64,
    max_tokens: u32,
    output_language: String,
    auto_paste: bool,
    auto_press_enter: bool,
) -> Result<(), String> {
    let mut custom = modes::load_custom_modes();
    custom.insert(id, modes::Mode {
        name,
        description,
        icon: if icon.is_empty() { "⚙️".into() } else { icon },
        system: if system.is_empty() { None } else { Some(system) },
        user_template: if user_template.is_empty() { None } else { Some(user_template) },
        temperature,
        model,
        stt_model,
        stt_prompt,
        stt_temperature,
        max_tokens: if max_tokens == 0 { 4096 } else { max_tokens },
        output_language: if output_language.is_empty() { "auto".into() } else { output_language },
        auto_paste,
        auto_press_enter,
        ..Default::default()
    });
    modes::save_custom_modes(&custom).map_err(|e| e.to_string())
}

/// Delete a custom mode.
#[tauri::command]
pub fn delete_custom_mode(id: String) -> Result<(), String> {
    let mut custom = modes::load_custom_modes();
    custom.remove(&id);
    modes::save_custom_modes(&custom).map_err(|e| e.to_string())
}

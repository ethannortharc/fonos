//! LLM commands — thin Tauri wrappers around fonos_core::llm and fonos_core::model_caps.

use super::AppState;
use fonos_core::model_caps;

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

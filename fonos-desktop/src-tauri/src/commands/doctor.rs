//! Setup Doctor shell checks (issue #30).
//!
//! Merges the pure config-lint findings from [`fonos_core::doctor::lint_config`]
//! with the checks that need network / OS / audio access: endpoint reachability,
//! microphone + accessibility permissions, and a conversation-TTS RTF
//! measurement. The whole run is bounded (~10s worst case) and parallel where
//! it matters (endpoint probes and the RTF synth run concurrently).
//!
//! `run_doctor` returns `Vec<Finding>` (serde-serializable); `apply_doctor_fix`
//! applies one typed `FixAction` and lets the frontend re-run the whole doctor.

use std::time::{Duration, Instant};

use fonos_core::config::AppConfig;
use fonos_core::doctor::{Finding, FixAction, Severity};
use fonos_core::llm::ServiceConfig;

use super::AppState;

/// Strip the scheme from a base URL for compact display (`localhost:8000`).
fn host_of(base_url: &str) -> String {
    base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

// ── endpoint reachability ───────────────────────────────────────────────────

/// Group the STT / LLM / TTS default profiles by base URL so services sharing a
/// host (the common local-server case) collapse into a single probe + row.
fn group_services(config: &AppConfig) -> Vec<(String, ServiceConfig)> {
    let roles = [
        ("STT", fonos_core::services::resolve_service(config, "stt")),
        ("LLM", fonos_core::services::resolve_service(config, "llm")),
        ("TTS", fonos_core::services::resolve_service(config, "tts")),
    ];
    let mut groups: Vec<(Vec<&str>, ServiceConfig)> = Vec::new();
    for (label, svc) in roles {
        // Skip unconfigured roles and Apple on-device (no network endpoint).
        if svc.base_url.trim().is_empty() || svc.provider == "apple" {
            continue;
        }
        if let Some(g) = groups.iter_mut().find(|(_, s)| s.base_url == svc.base_url) {
            g.0.push(label);
        } else {
            groups.push((vec![label], svc));
        }
    }
    groups
        .into_iter()
        .map(|(labels, svc)| (labels.join(" · "), svc))
        .collect()
}

/// Probe a service's `/v1/models` endpoint with a short timeout. Returns the
/// round-trip latency in ms. Any HTTP response (even 4xx auth errors) counts as
/// "reachable"; only network/timeout failures are treated as unreachable.
async fn probe_endpoint(svc: &ServiceConfig) -> Result<u64, String> {
    let base = svc.base_url.trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    if !svc.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", svc.api_key));
    }
    let t0 = Instant::now();
    match req.send().await {
        Ok(_) => Ok(t0.elapsed().as_millis() as u64),
        Err(e) => Err(e.to_string()),
    }
}

/// Probe every configured endpoint group concurrently.
async fn check_endpoints(config: &AppConfig) -> Vec<Finding> {
    let groups = group_services(config);
    let mut handles = Vec::new();
    for (label, svc) in groups {
        handles.push(tokio::spawn(async move {
            let host = host_of(&svc.base_url);
            match probe_endpoint(&svc).await {
                Ok(ms) => Finding {
                    id: format!("endpoint_ok:{host}"),
                    severity: Severity::Pass,
                    message_key: "doctor.endpoint_ok".to_string(),
                    message_params: vec![label, format!("{host} · {ms}ms")],
                    fix: None,
                },
                Err(_) => Finding {
                    id: format!("endpoint_bad:{host}"),
                    severity: Severity::Warn,
                    message_key: "doctor.endpoint_unreachable".to_string(),
                    message_params: vec![label, host],
                    fix: None,
                },
            }
        }));
    }
    let mut out = Vec::new();
    for h in handles {
        if let Ok(f) = h.await {
            out.push(f);
        }
    }
    out
}

// ── permissions ─────────────────────────────────────────────────────────────

/// Microphone + accessibility permission checks. Non-automatable fixes render an
/// "Open System Settings" button via [`FixAction::OpenSettingsPane`].
fn check_permissions() -> Vec<Finding> {
    let mic_ok = super::dictation::has_microphone().unwrap_or(false);
    let ax_ok = crate::injection::accessibility_trusted();

    if mic_ok && ax_ok {
        return vec![Finding {
            id: "permissions_ok".to_string(),
            severity: Severity::Pass,
            message_key: "doctor.permissions_ok".to_string(),
            message_params: Vec::new(),
            fix: None,
        }];
    }

    let mut out = Vec::new();
    if !mic_ok {
        out.push(Finding {
            id: "permission_mic".to_string(),
            severity: Severity::Warn,
            message_key: "doctor.permission_mic".to_string(),
            message_params: Vec::new(),
            fix: Some(FixAction::OpenSettingsPane { pane: "microphone".to_string() }),
        });
    }
    if !ax_ok {
        out.push(Finding {
            id: "permission_accessibility".to_string(),
            severity: Severity::Warn,
            message_key: "doctor.permission_accessibility".to_string(),
            message_params: Vec::new(),
            fix: Some(FixAction::OpenSettingsPane { pane: "accessibility".to_string() }),
        });
    }
    out
}

// ── conversation TTS real-time-factor ───────────────────────────────────────

/// Look for a faster kokoro model on the server that isn't the current one.
async fn find_kokoro_switch(svc: &ServiceConfig, profile_id: &str) -> Option<FixAction> {
    let models = super::llm::list_provider_models(svc.base_url.clone(), svc.api_key.clone())
        .await
        .ok()?;
    let current = svc.model.to_lowercase();
    for m in models {
        let id = m["id"].as_str().unwrap_or("");
        let lower = id.to_lowercase();
        if lower.contains("kokoro") && lower != current {
            return Some(FixAction::SwitchTtsModel {
                profile_id: profile_id.to_string(),
                model: id.to_string(),
            });
        }
    }
    None
}

/// Measure the conversation voice's real-time factor by synthesizing a short
/// fixed phrase and comparing wall-clock time to audio duration. Skips silently
/// when TTS is unconfigured or the synth fails/times out.
/// Cache the last RTF measurement so opening Settings repeatedly doesn't
/// re-synthesize: (model+profile key, measured rtf, when). Re-measured after
/// 30 minutes or when the profile/model changes.
static RTF_CACHE: std::sync::Mutex<Option<(String, f64, Instant)>> = std::sync::Mutex::new(None);
const RTF_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

async fn check_conversation_rtf(config: &AppConfig) -> Vec<Finding> {
    // The conversation reply uses sts_voice_profile, falling back to tts_profile.
    let profile_id = if !config.sts_voice_profile.trim().is_empty() {
        config.sts_voice_profile.trim().to_string()
    } else {
        config.tts_profile.trim().to_string()
    };
    if profile_id.is_empty() {
        return Vec::new();
    }
    let svc = fonos_core::services::resolve_profile(config, &profile_id);
    if svc.base_url.trim().is_empty() || svc.provider == "apple" {
        return Vec::new();
    }

    let voice = {
        let v = config.sts_voice.trim();
        if v.is_empty() { "default".to_string() } else { v.to_string() }
    };
    let phrase = "Fonos conversation speed check, one two three.";

    let cache_key = format!("{profile_id}::{}", svc.model);
    let cached = RTF_CACHE
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .filter(|(k, _, at)| *k == cache_key && at.elapsed() < RTF_CACHE_TTL)
        .map(|(_, rtf, _)| rtf);

    let rtf = if let Some(rtf) = cached {
        rtf
    } else {
        let t0 = Instant::now();
        let synth = tokio::time::timeout(
            Duration::from_secs(8),
            fonos_core::tts::synthesize_wav(&svc, phrase, &voice, 1.0),
        )
        .await;
        let wav = match synth {
            Ok(Ok(w)) => w,
            _ => return Vec::new(), // unreachable/slow/error — skip gracefully
        };
        let wall = t0.elapsed().as_secs_f64();
        let audio = match fonos_core::listen::wav_duration_secs(&wav) {
            Some(d) if d > 0.05 => d,
            _ => return Vec::new(),
        };
        let measured = wall / audio;
        if let Ok(mut g) = RTF_CACHE.lock() {
            *g = Some((cache_key, measured, Instant::now()));
        }
        measured
    };

    if rtf > 1.5 {
        let fix = find_kokoro_switch(&svc, &profile_id).await;
        vec![Finding {
            id: "rtf_slow".to_string(),
            severity: Severity::Advise,
            message_key: "doctor.rtf_slow".to_string(),
            message_params: vec![format!("{rtf:.1}")],
            fix,
        }]
    } else {
        vec![Finding {
            id: "rtf_ok".to_string(),
            severity: Severity::Pass,
            message_key: "doctor.rtf_ok".to_string(),
            message_params: vec![format!("{rtf:.1}× realtime")],
            fix: None,
        }]
    }
}

// ── commands ────────────────────────────────────────────────────────────────

/// Run the full Setup Doctor: config lint + endpoint/permission/RTF probes.
///
/// Order roughly matches the resident card: connectivity, permissions, config
/// lint, then the RTF suggestion. Endpoint probes and the RTF synth run
/// concurrently; the whole run is bounded to about ten seconds.
#[tauri::command]
pub async fn run_doctor(state: tauri::State<'_, AppState>) -> Result<Vec<Finding>, String> {
    // Snapshot config without holding the lock across awaits.
    let config = {
        let guard = state.config.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let modes = fonos_core::modes::all_modes();

    let lint = fonos_core::doctor::lint_config(&config, &modes);
    let (endpoints, rtf) =
        tokio::join!(check_endpoints(&config), check_conversation_rtf(&config));
    let permissions = check_permissions();

    let mut out = Vec::with_capacity(endpoints.len() + permissions.len() + lint.len() + rtf.len());
    out.extend(endpoints);
    out.extend(permissions);
    out.extend(lint);
    out.extend(rtf);
    Ok(out)
}

/// Apply one Setup Doctor [`FixAction`], persisting the result.
///
/// Config-only fixes mutate `AppState.config` (via [`fonos_core::doctor::apply_config_fix`])
/// and save; `PointModeModelToDefault` edits `modes.json`; `OpenSettingsPane`
/// deep-links to System Settings. The frontend re-runs `run_doctor` afterward.
#[tauri::command]
pub fn apply_doctor_fix(state: tauri::State<'_, AppState>, fix: FixAction) -> Result<(), String> {
    match &fix {
        FixAction::PointModeModelToDefault { mode_id } => {
            let mut modes = fonos_core::modes::load_custom_modes();
            if let Some(m) = modes.get_mut(mode_id) {
                m.model.clear();
                fonos_core::modes::save_custom_modes(&modes).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        FixAction::OpenSettingsPane { pane } => {
            super::permissions::open_settings_pane(pane.clone())
        }
        _ => {
            let mut guard = state.config.lock().map_err(|e| e.to_string())?;
            fonos_core::doctor::apply_config_fix(&mut guard, &fix)?;
            let updated = guard.clone();
            drop(guard);
            updated
                .save()
                .map_err(|e| format!("failed to save config: {e}"))?;
            Ok(())
        }
    }
}

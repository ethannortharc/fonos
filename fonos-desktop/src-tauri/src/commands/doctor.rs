//! Setup Doctor shell checks (issue #30).
//!
//! Merges the pure config-lint findings from [`fonos_core::doctor::lint_config`]
//! with the checks that need network / OS / audio access: endpoint reachability,
//! microphone + accessibility permissions, and a conversation-TTS RTF
//! measurement. The whole run is bounded (~10s worst case) and parallel where
//! it matters (endpoint probes and the RTF synth run concurrently).
//!
//! A failing endpoint probe additionally runs the OMLX process-without-listener
//! differential (real-device UX finding): a raw TCP refusal on the configured
//! port plus a `pgrep -f omlx` hit distinguishes "the server was never
//! started" from "its Homebrew-launched process is still alive but stopped
//! listening" (e.g. its launcher pointing at a removed Python) — a state that
//! otherwise looks identical from the HTTP probe alone and leaves dictation
//! requests hanging with no way to tell the two apart.
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
                Err(_) => {
                    // Differential (real-device UX finding): a Homebrew-launched
                    // `omlx-server` can be left running by its supervisor even
                    // after the actual server process died (e.g. its launcher
                    // points at a removed Python) — the port stops listening but
                    // `pgrep` still finds a matching process. That state reads
                    // identically to "server not started" from the HTTP probe
                    // alone, so a dictation request against it just sits in
                    // "Processing" with no way to tell the two apart. Enrich the
                    // same failing-endpoint row with a distinct message instead
                    // of adding a second row, so the warning count doesn't
                    // double-count one failure.
                    let message_key = if omlx_stale_process_decision(
                        tcp_port_refused(&host).await,
                        omlx_process_running().await,
                    ) {
                        "doctor.omlx_stale_process"
                    } else {
                        "doctor.endpoint_unreachable"
                    };
                    Finding {
                        id: format!("endpoint_bad:{host}"),
                        severity: Severity::Warn,
                        message_key: message_key.to_string(),
                        message_params: vec![label, host],
                        fix: None,
                    }
                }
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

// ── OMLX process-without-listener differential ──────────────────────────────

/// Pure decision: is this the "stale OMLX process" state? Both signals must
/// hold — a bare TCP refusal alone just means "nothing's there" (could be a
/// typo'd port, a server never started, …), and a matching process alone just
/// means *some* `omlx`-named process is running (could be a client, a
/// different instance). Only the combination pins it on "the launcher started
/// something, but that something isn't the listener anymore." Extracted so the
/// decision is unit-testable without a real socket or `pgrep`.
fn omlx_stale_process_decision(tcp_refused: bool, process_exists: bool) -> bool {
    tcp_refused && process_exists
}

/// Raw TCP connect to `host:port` (bypassing HTTP entirely) with a short
/// timeout, true only on an explicit connection refusal — the OS-level
/// signature of "nothing is bound to this port," as opposed to a DNS failure,
/// a firewalled drop (which times out, not refuses), or a TLS/HTTP-level
/// problem that a live process could still produce. `host_port` is already
/// `host:port` (see [`host_of`]); malformed input degrades to `false`.
async fn tcp_port_refused(host_port: &str) -> bool {
    match tokio::time::timeout(
        Duration::from_millis(800),
        tokio::net::TcpStream::connect(host_port),
    )
    .await
    {
        Ok(Err(e)) => e.kind() == std::io::ErrorKind::ConnectionRefused,
        _ => false, // connected fine, or the connect attempt itself timed out
    }
}

/// True when a process whose command line matches `omlx` is running
/// (`pgrep -f omlx`). Best-effort: any failure to run `pgrep` at all (not on
/// PATH, unsupported platform, …) degrades silently to `false` rather than
/// erroring the whole doctor run.
async fn omlx_process_running() -> bool {
    tokio::task::spawn_blocking(|| {
        std::process::Command::new("pgrep")
            .arg("-f")
            .arg("omlx")
            .output()
            .map(|out| out.status.success() && !out.stdout.is_empty())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
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

/// Cache measured RTFs so opening Settings / re-probing a scenario repeatedly
/// doesn't re-synthesize. Keyed by `base_url::model::voice`, re-measured after
/// 30 minutes.
static RTF_CACHE: std::sync::Mutex<std::collections::BTreeMap<String, (f64, Instant)>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());
const RTF_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

/// Measure a TTS model's real-time factor (wall-clock synth time ÷ audio
/// seconds) by synthesizing one short fixed phrase. Shared by the Setup Doctor
/// and the scenario probe (issue #29). Returns `None` when the service is
/// unconfigured, Apple on-device, or the synth fails / times out. Results are
/// cached ~30 min per `(base_url, model, voice)`.
pub(crate) async fn measure_tts_rtf(svc: &ServiceConfig, voice: &str) -> Option<f64> {
    if svc.base_url.trim().is_empty() || svc.provider == "apple" {
        return None;
    }
    let phrase = "Fonos conversation speed check, one two three.";
    let key = format!("{}::{}::{}", svc.base_url.trim_end_matches('/'), svc.model, voice);

    let cached = RTF_CACHE
        .lock()
        .ok()
        .and_then(|g| g.get(&key).copied())
        .filter(|(_, at)| at.elapsed() < RTF_CACHE_TTL)
        .map(|(rtf, _)| rtf);
    if let Some(rtf) = cached {
        return Some(rtf);
    }

    let t0 = Instant::now();
    let synth = tokio::time::timeout(
        Duration::from_secs(8),
        fonos_core::tts::synthesize_wav(svc, phrase, voice, 1.0),
    )
    .await;
    let wav = match synth {
        Ok(Ok(w)) => w,
        _ => return None, // unreachable / slow / error — skip gracefully
    };
    let wall = t0.elapsed().as_secs_f64();
    let audio = match fonos_core::listen::wav_duration_secs(&wav) {
        Some(d) if d > 0.05 => d,
        _ => return None,
    };
    let rtf = wall / audio;
    if let Ok(mut g) = RTF_CACHE.lock() {
        g.insert(key, (rtf, Instant::now()));
    }
    Some(rtf)
}

/// Measure the conversation voice's real-time factor and advise a faster model
/// when it stutters. Skips silently when TTS is unconfigured or the synth fails.
///
/// Resolves the probe voice from the `call.default` widget's
/// [`super::call_widget::CallProps`] — mirroring `resolve_call_cfg`'s TTS
/// branch — instead of the deprecated `sts_voice_profile`/`sts_voice` config
/// fields (Workbench P2 Task 14). The call composite is the only remaining
/// reader of "the conversation voice" now that the walkie/STS page is
/// retired, so probing its actually-resolved voice keeps this check honest
/// even when `call.default` has been tuned away from those legacy fields'
/// values. `call.default` is a built-in `effective_widgets` never removes,
/// but the lookup still degrades to "skip" rather than panic if that ever
/// stops holding.
async fn check_conversation_rtf(config: &AppConfig) -> Vec<Finding> {
    let widgets = fonos_core::workflow::engine::effective_widgets(config);
    let Some(call_props) = widgets
        .iter()
        .find(|w| w.id == "call.default")
        .and_then(|w| serde_json::from_value::<super::call_widget::CallProps>(w.props.clone()).ok())
    else {
        return Vec::new();
    };

    // Same voice_profile→global-"tts" convention as resolve_call_cfg's TTS
    // branch, keeping a concrete profile_id around for the SwitchTtsModel fix.
    let profile_id = if !call_props.voice_profile.trim().is_empty() {
        call_props.voice_profile.trim().to_string()
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
        let v = call_props.voice.trim();
        if v.is_empty() { "default".to_string() } else { v.to_string() }
    };

    let rtf = match measure_tts_rtf(&svc, &voice).await {
        Some(rtf) => rtf,
        None => return Vec::new(),
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

    let lint = fonos_core::doctor::lint_config(&config);
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
/// and save; `OpenSettingsPane` deep-links to System Settings. The frontend
/// re-runs `run_doctor` afterward.
///
/// `PointModeModelToDefault` (which used to edit `modes.json` here) was
/// retired in Workbench P2 Task 11 along with the `mode_model_missing` check
/// that produced it — `lint_config` no longer knows about `modes.json` at all.
#[tauri::command]
pub fn apply_doctor_fix(state: tauri::State<'_, AppState>, fix: FixAction) -> Result<(), String> {
    match &fix {
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

#[cfg(test)]
mod omlx_stale_process_tests {
    use super::omlx_stale_process_decision;

    #[test]
    fn both_signals_true_is_stale() {
        assert!(omlx_stale_process_decision(true, true));
    }

    #[test]
    fn refused_without_matching_process_is_not_stale() {
        // Port just isn't in use by anything OMLX-related — a plain
        // "unreachable" finding, not the stale-launcher one.
        assert!(!omlx_stale_process_decision(true, false));
    }

    #[test]
    fn matching_process_without_refusal_is_not_stale() {
        // Some omlx-ish process is running, but the port responded (or the
        // TCP probe merely timed out rather than refusing) — not this state.
        assert!(!omlx_stale_process_decision(false, true));
    }

    #[test]
    fn neither_signal_is_not_stale() {
        assert!(!omlx_stale_process_decision(false, false));
    }
}

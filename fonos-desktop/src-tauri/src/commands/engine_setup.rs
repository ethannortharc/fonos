//! Engine-setup commands (onboarding P3): two-layer engine detection,
//! hardware/disk facts, and install/start/pull orchestration. Pure logic
//! lives in [`fonos_core::engine_setup`]; this module is the shell.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

// ---------------------------------------------------------------------------
// Re-entrancy guard for engine setup (mirrors workflow_exec.rs pattern)
// ---------------------------------------------------------------------------

/// True while a setup run is in flight. Re-entrant triggers are dropped so
/// overlapping runs can't race (e.g., double-fire detached-ack launching two
/// brew installs and two engine serves).
static SETUP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// RAII reset for [`SETUP_IN_FLIGHT`]: clears the flag on scope exit (including
/// early returns and the `run_setup` await point), so a failed or empty run
/// never wedges the trigger.
struct SetupInFlightGuard;
impl Drop for SetupInFlightGuard {
    fn drop(&mut self) {
        SETUP_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

impl SetupInFlightGuard {
    /// Attempt to claim the in-flight guard: `None` if a setup is already in
    /// progress.
    fn try_acquire() -> Option<SetupInFlightGuard> {
        SETUP_IN_FLIGHT
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| SetupInFlightGuard)
    }
}

// ---------------------------------------------------------------------------
// Engine specs and detection
// ---------------------------------------------------------------------------

/// Static facts for one supported local engine.
struct EngineSpec {
    key: &'static str,
    url: &'static str,
    /// `pgrep -f` pattern for a running-but-not-listening process.
    process: &'static str,
    /// Binaries whose presence on PATH means "installed".
    binaries: &'static [&'static str],
    /// App-bundle paths whose existence means "installed".
    app_paths: &'static [&'static str],
    /// True when another engine in this table listens on the same default
    /// `url` (omlx and vllm both default to `:8000`). A shared-port engine's
    /// HTTP probe alone can't tell the two apart, so `running` additionally
    /// requires the brand-specific process match.
    shared_port: bool,
}

/// Homebrew formula used to install oMLX — kept as one constant so a rename
/// is a one-line change. Consumed by [`install_action`].
pub(crate) const OMLX_BREW_FORMULA: &str = "omlx";

const ENGINES: &[EngineSpec] = &[
    EngineSpec {
        key: "omlx",
        url: "http://localhost:8000",
        process: "omlx",
        binaries: &["omlx", "omlx-server"],
        app_paths: &[],
        shared_port: true,
    },
    EngineSpec {
        key: "lmstudio",
        url: "http://localhost:1234",
        process: "LM Studio",
        binaries: &["lms"],
        app_paths: &["/Applications/LM Studio.app"],
        shared_port: false,
    },
    EngineSpec {
        key: "ollama",
        url: "http://localhost:11434",
        process: "ollama",
        binaries: &["ollama"],
        app_paths: &["/Applications/Ollama.app"],
        shared_port: false,
    },
    EngineSpec {
        key: "vllm",
        url: "http://localhost:8000",
        process: "vllm",
        binaries: &["vllm"],
        app_paths: &[],
        shared_port: true,
    },
];

/// True when `name` resolves on PATH (`which <name>`). Best-effort.
async fn binary_on_path(name: &'static str) -> bool {
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("which")
            .arg(name)
            .output()
            .map(|out| out.status.success() && !out.stdout.is_empty())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// Two-layer detection result for one engine.
///
/// omlx and vllm both default to `http://localhost:8000` (`shared_port`
/// engines): whichever one actually owns the port answers `/v1/models` for
/// both, so an HTTP-only probe would make both report `running: true`. For
/// those engines `running` additionally requires the brand-specific process
/// probe (`pgrep -f <process>`) to match — a generic port-8000 responder
/// with no matching brand process means neither omlx nor vllm claims
/// `running`. This does not block a *different* engine from later taking
/// over that port; the same scan/probe flow re-evaluates both signals each
/// call, so a takeover is picked up on the next detection pass.
#[derive(Serialize)]
pub struct EngineDetection {
    /// Engine key: omlx / lmstudio / ollama / vllm.
    pub engine: String,
    /// A live HTTP listener answered `/v1/models` (and, for shared-port
    /// engines, a matching brand process is also running — see struct doc).
    pub running: bool,
    /// Installed on this machine (binary on PATH, app bundle, or a live
    /// process) even if not currently listening.
    pub installed: bool,
    /// Default base URL for the engine.
    pub url: String,
}

/// Pure decision for the `running` verdict from the two independent
/// signals. Shared-port engines (omlx, vllm) require both the HTTP probe
/// and the brand-specific process match, since either one alone can't
/// distinguish "this engine is running" from "the other engine on this
/// port is running." Non-shared-port engines only need the HTTP probe —
/// their port isn't contested, so a process check would be redundant.
fn running_verdict(shared_port: bool, http_ok: bool, process_ok: bool) -> bool {
    if shared_port {
        http_ok && process_ok
    } else {
        http_ok
    }
}

async fn detect_one(spec: &'static EngineSpec) -> EngineDetection {
    let (http_ok, _ms, _models) =
        super::scenarios::fetch_models(spec.url, "", Duration::from_secs(2)).await;
    let mut installed = http_ok;
    if !installed {
        for b in spec.binaries {
            if binary_on_path(b).await {
                installed = true;
                break;
            }
        }
    }
    if !installed {
        installed = spec.app_paths.iter().any(|p| std::path::Path::new(p).exists());
    }
    // Shared-port engines need this probe's result for `running_verdict`
    // regardless of whether `installed` is already settled, so run it
    // whenever either consumer needs it — never more than once either way.
    let process_matches = if spec.shared_port || !installed {
        super::doctor::process_running(spec.process).await
    } else {
        false
    };
    if !installed {
        installed = process_matches;
    }
    let running = running_verdict(spec.shared_port, http_ok, process_matches);
    EngineDetection {
        engine: spec.key.to_string(),
        running,
        installed,
        url: spec.url.to_string(),
    }
}

/// Probe all four local engines in parallel: running (HTTP) + installed
/// (PATH / app bundle / process).
///
/// `futures_util` isn't a dependency of this crate, so parallelism here uses
/// the same `tokio::spawn` + collect pattern as `doctor::check_endpoints`
/// rather than `join_all`.
#[tauri::command]
pub async fn engine_detect() -> Result<Vec<EngineDetection>, String> {
    let handles: Vec<_> = ENGINES.iter().map(|spec| tokio::spawn(detect_one(spec))).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(det) = h.await {
            out.push(det);
        }
    }
    Ok(out)
}

/// Hardware facts + derived tier. `tier` serializes lowercase.
#[derive(Serialize)]
pub struct HardwareInfo {
    /// Total physical memory in bytes.
    pub mem_bytes: u64,
    /// CPU/chip brand string ("Apple M4 Pro", "AMD Ryzen …").
    pub chip: String,
    /// An `nvidia-smi` binary is on PATH.
    pub has_nvidia_gpu: bool,
    /// Derived recommendation tier.
    pub tier: fonos_core::engine_setup::HardwareTier,
}

// Only the macOS sysctl path calls this; gate it so Linux builds stay
// warning-free.
#[cfg(target_os = "macos")]
fn cmd_stdout(cmd: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Read memory size, chip brand, and NVIDIA presence; classify the tier.
#[tauri::command]
pub async fn detect_hardware() -> Result<HardwareInfo, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        let (mem_bytes, chip) = (
            cmd_stdout("sysctl", &["-n", "hw.memsize"])
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            cmd_stdout("sysctl", &["-n", "machdep.cpu.brand_string"]).unwrap_or_default(),
        );
        #[cfg(not(target_os = "macos"))]
        let (mem_bytes, chip) = (
            std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|t| fonos_core::engine_setup::parse_meminfo_total_bytes(&t))
                .unwrap_or(0),
            std::fs::read_to_string("/proc/cpuinfo")
                .ok()
                .and_then(|t| {
                    t.lines()
                        .find(|l| l.starts_with("model name"))
                        .and_then(|l| l.split(':').nth(1))
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_default(),
        );
        let has_nvidia_gpu = std::process::Command::new("which")
            .arg("nvidia-smi")
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
        let tier = fonos_core::engine_setup::classify_tier(mem_bytes, &chip, has_nvidia_gpu);
        Ok(HardwareInfo { mem_bytes, chip, has_nvidia_gpu, tier })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Free disk space on the user's home volume, in KB (`df -k ~`).
#[derive(Serialize)]
pub struct DiskInfo {
    /// Available kilobytes.
    pub available_kb: u64,
}

/// Check available disk space (used by the pre-execution review card).
#[tauri::command]
pub async fn check_disk_space() -> Result<DiskInfo, String> {
    tokio::task::spawn_blocking(|| {
        let home = dirs::home_dir().ok_or("no home dir")?;
        let out = std::process::Command::new("df")
            .arg("-k")
            .arg(&home)
            .output()
            .map_err(|e| format!("df: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        fonos_core::engine_setup::parse_df_available_kb(&text)
            .map(|available_kb| DiskInfo { available_kb })
            .ok_or_else(|| "could not parse df output".to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

// ---------------------------------------------------------------------------
// install → start → wait → pull orchestration (`engine_setup` command)
// ---------------------------------------------------------------------------

/// The confirmed setup plan from the review card (Task 8 emits it).
#[derive(Deserialize)]
pub struct SetupPlanDto {
    /// Engine key: omlx / ollama (lmstudio/vllm never orchestrate installs).
    pub engine: String,
    /// Run the install stage.
    pub install: bool,
    /// Run the start stage.
    pub start: bool,
    /// Ollama models to pull (empty for other engines).
    pub pulls: Vec<String>,
    /// Base URL to wait on after starting.
    pub base_url: String,
}

/// Emit an `engine:setup` event to the main window (JSON payload as a string,
/// mirroring `diarize.rs`'s `diarize:download` wiring).
fn emit_setup(app: &tauri::AppHandle, payload: serde_json::Value) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("engine:setup", payload.to_string());
    }
}

/// Emit a terminal `engine:setup` error carrying the machine-readable
/// `failed_stage` the frontend Review card needs for its downgrade path, then
/// restore the tray to its real status (any in-flight pull progress is now
/// void). Controller ruling: every stage failure is both an event and a
/// machine-readable outcome.
fn emit_error(app: &tauri::AppHandle, engine: &str, failed_stage: &str, message: String) {
    emit_setup(
        app,
        serde_json::json!({ "stage": "error", "engine": engine, "failed_stage": failed_stage, "message": message }),
    );
    crate::tray::refresh_tray_status(app, None);
}

/// Run a blocking subprocess to completion, surfacing a trimmed stderr on
/// failure. Called only from `spawn_blocking`.
fn run_shell(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{cmd} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// What the install stage should do for a given engine.
enum InstallAction {
    /// Run this program with these args on a blocking thread.
    Automated { program: String, args: Vec<String> },
    /// No automated path — surface these steps to the user (a non-error
    /// terminal outcome, not an `Err`). Covers unmanaged engines
    /// (lmstudio/vllm) and a machine with no Homebrew.
    Manual { message: String },
}

/// Resolve a usable `brew` executable: the two standard Homebrew prefixes
/// (Apple-Silicon `/opt/homebrew`, Intel `/usr/local`) first, then a PATH
/// lookup. `None` means Homebrew is absent — callers surface manual steps
/// rather than failing. Does blocking I/O; call from `spawn_blocking`.
fn resolve_brew() -> Option<String> {
    for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    let out = std::process::Command::new("which").arg("brew").output().ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// Decide the install action for an engine. omlx/ollama install via Homebrew
/// (Linux ollama uses the official installer script); a missing brew or an
/// unmanaged engine downgrades to [`InstallAction::Manual`] instead of an
/// error. Does blocking I/O (`resolve_brew`); call from `spawn_blocking`.
fn install_action(engine: &str) -> InstallAction {
    match engine {
        "omlx" => match resolve_brew() {
            Some(brew) => InstallAction::Automated {
                program: brew,
                args: vec!["install".into(), OMLX_BREW_FORMULA.into()],
            },
            None => InstallAction::Manual {
                message: format!(
                    "Homebrew not found. Install oMLX manually, then re-run setup: brew install {OMLX_BREW_FORMULA}"
                ),
            },
        },
        "ollama" => {
            #[cfg(target_os = "macos")]
            {
                match resolve_brew() {
                    Some(brew) => InstallAction::Automated {
                        program: brew,
                        args: vec!["install".into(), "ollama".into()],
                    },
                    None => InstallAction::Manual {
                        message: "Homebrew not found. Install Ollama from https://ollama.com/download, then re-run setup.".into(),
                    },
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                // Official installer script (documented install path on Linux).
                InstallAction::Automated {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        "curl -fsSL https://ollama.com/install.sh | sh".into(),
                    ],
                }
            }
        }
        other => InstallAction::Manual {
            message: format!(
                "{other} has no automated installer; install it manually and re-run setup."
            ),
        },
    }
}

/// Detached start command per engine. The child is spawned and *not* waited
/// on — the wait stage polls the HTTP listener instead.
fn start_cmd(engine: &str) -> Option<(&'static str, Vec<&'static str>)> {
    match engine {
        "omlx" => Some(("omlx-server", vec![])),
        "ollama" => Some(("ollama", vec!["serve"])),
        _ => None,
    }
}

/// Monotonic progress gate. Ollama's `/api/pull` reports per-blob pct that
/// restarts at 0 for each layer; tracking the running max means the bar (and
/// the tray) only ever move forward. Returns `Some(pct)` when `pct` advances
/// the max (emit it), `None` otherwise (suppress — this doubles as the emit
/// throttle the review requested). Controller ruling.
fn advance_progress(max: &mut u8, pct: u8) -> Option<u8> {
    if pct > *max {
        *max = pct;
        Some(pct)
    } else {
        None
    }
}

/// The confirmed install → start → wait → pull orchestration for one engine.
/// Runs detached (see [`engine_setup`]); every stage transition, progress
/// tick, and terminal outcome (`done` / `error` / `manual`) is reported
/// through `engine:setup` events, so the caller never blocks on it.
async fn run_setup(app: tauri::AppHandle, plan: SetupPlanDto) {
    // ---- install --------------------------------------------------------
    if plan.install {
        emit_setup(&app, serde_json::json!({ "stage": "install", "engine": plan.engine }));
        let engine = plan.engine.clone();
        // `install_action` (brew resolution) and `run_shell` both block, so
        // resolve *and* run on a blocking thread. `Ok(None)` = installed,
        // `Ok(Some(msg))` = manual steps, `Err` = install failed.
        let outcome = tokio::task::spawn_blocking(move || match install_action(&engine) {
            InstallAction::Automated { program, args } => {
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                run_shell(&program, &arg_refs).map(|_| None)
            }
            InstallAction::Manual { message } => Ok(Some(message)),
        })
        .await;
        match outcome {
            Ok(Ok(None)) => {}
            Ok(Ok(Some(message))) => {
                emit_setup(
                    &app,
                    serde_json::json!({ "stage": "manual", "engine": plan.engine, "message": message }),
                );
                return; // can't start an engine we didn't install
            }
            Ok(Err(e)) => return emit_error(&app, &plan.engine, "install", e),
            Err(e) => return emit_error(&app, &plan.engine, "install", format!("join: {e}")),
        }
    }

    // ---- start (detached child; the wait stage confirms it came up) -----
    if plan.start {
        emit_setup(&app, serde_json::json!({ "stage": "start", "engine": plan.engine }));
        let engine = plan.engine.clone();
        let res = tokio::task::spawn_blocking(move || match start_cmd(&engine) {
            Some((cmd, args)) => std::process::Command::new(cmd)
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("start {cmd}: {e}")),
            None => Err(format!("no start command for {engine}")),
        })
        .await;
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return emit_error(&app, &plan.engine, "start", e),
            Err(e) => return emit_error(&app, &plan.engine, "start", format!("join: {e}")),
        }
    }

    // ---- wait for the listener (also covers "already running" cheaply) --
    emit_setup(&app, serde_json::json!({ "stage": "wait", "engine": plan.engine }));
    let mut up = false;
    for _ in 0..30 {
        let (reachable, _, _) =
            super::scenarios::fetch_models(&plan.base_url, "", Duration::from_secs(2)).await;
        if reachable {
            up = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    if !up {
        return emit_error(
            &app,
            &plan.engine,
            "wait",
            format!("{} did not come up on {}", plan.engine, plan.base_url),
        );
    }

    // ---- pull (ollama only; monotonic progress drives the tray) ---------
    for model in &plan.pulls {
        emit_setup(&app, serde_json::json!({ "stage": "pull", "engine": plan.engine, "model": model, "pct": 0 }));
        let app2 = app.clone();
        let engine2 = plan.engine.clone();
        let model2 = model.clone();
        let mut max_pct: u8 = 0;
        let res = fonos_core::engine_setup::ollama_pull(&plan.base_url, model, move |p| {
            if let Some(pct) = p.pct {
                if let Some(shown) = advance_progress(&mut max_pct, pct) {
                    emit_setup(
                        &app2,
                        serde_json::json!({ "stage": "pull", "engine": engine2, "model": model2, "pct": shown }),
                    );
                    // Rust-side direct tray wiring (mirrors diarize P2-T5).
                    crate::tray::refresh_tray_status(
                        &app2,
                        Some((crate::tray::TrayRow::Llm, shown.min(100))),
                    );
                }
            }
        })
        .await;
        if let Err(e) = res {
            return emit_error(&app, &plan.engine, "pull", format!("pull {model}: {e}"));
        }
    }

    emit_setup(&app, serde_json::json!({ "stage": "done", "engine": plan.engine }));
    crate::tray::refresh_tray_status(&app, None);
}

/// Orchestrate install → start → wait → pull for a confirmed setup plan.
///
/// The heavy lifting runs **detached**: the command spawns [`run_setup`] and
/// returns an immediate ack, so the `invoke` never blocks for the minutes an
/// install or model pull can take. Every stage transition, progress tick, and
/// terminal outcome is reported through `engine:setup` events (payload JSON
/// string `{stage, engine, pct?, model?, message?, failed_stage?}`, stage ∈
/// `install|start|wait|pull|manual|done|error`), consumed by Task 8.
///
/// Re-entry is guarded: double-firing the detached-ack command is dropped so
/// overlapping runs can't race (e.g., two brew installs, two engine serves).
#[tauri::command(rename_all = "snake_case")]
pub async fn engine_setup(app: tauri::AppHandle, plan: SetupPlanDto) -> Result<(), String> {
    let Some(_guard) = SetupInFlightGuard::try_acquire() else {
        emit_setup(
            &app,
            serde_json::json!({ "stage": "error", "engine": plan.engine, "failed_stage": "busy", "message": "engine setup already in progress" }),
        );
        return Ok(());
    };
    tauri::async_runtime::spawn(async move {
        let _guard_holder = _guard;
        run_setup(app, plan).await;
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Truth table for `running_verdict`: shared-port engines need BOTH
    /// signals; non-shared engines need only the HTTP probe.
    #[test]
    fn running_verdict_truth_table() {
        // shared-port (omlx/vllm on :8000)
        assert!(running_verdict(true, true, true), "shared TT should claim running");
        assert!(!running_verdict(true, true, false), "shared TF (other brand owns the port) must not claim running");
        assert!(!running_verdict(true, false, true), "shared FT (process up, port not answering) must not claim running");
        assert!(!running_verdict(true, false, false), "shared FF should not claim running");

        // non-shared-port (lmstudio/ollama)
        assert!(running_verdict(false, true, true), "non-shared T* should claim running");
        assert!(running_verdict(false, true, false), "non-shared T* should claim running regardless of process signal");
        assert!(!running_verdict(false, false, true), "non-shared F* must not claim running");
        assert!(!running_verdict(false, false, false), "non-shared F* must not claim running");
    }

    /// `advance_progress` only reports forward movement: equal pct and the
    /// per-layer restart-to-0 are both suppressed, so the emitted stream (and
    /// the tray) is monotonic. Controller carry-forward ruling.
    #[test]
    fn advance_progress_only_moves_forward() {
        let mut max = 0u8;
        // 0 is the already-emitted floor (the pull-start event), not a bump.
        assert_eq!(advance_progress(&mut max, 0), None);
        assert_eq!(advance_progress(&mut max, 5), Some(5));
        assert_eq!(advance_progress(&mut max, 5), None, "equal pct is suppressed");
        assert_eq!(advance_progress(&mut max, 2), None, "a layer restart is suppressed");
        assert_eq!(advance_progress(&mut max, 40), Some(40));
        assert_eq!(advance_progress(&mut max, 100), Some(100));
        assert_eq!(advance_progress(&mut max, 100), None);
        assert_eq!(max, 100);
    }

    /// Engines with no automated installer downgrade to manual steps rather
    /// than erroring (these branches touch no filesystem, so they're
    /// deterministic regardless of the host's Homebrew state).
    #[test]
    fn install_action_is_manual_for_unmanaged_engines() {
        assert!(matches!(install_action("lmstudio"), InstallAction::Manual { .. }));
        assert!(matches!(install_action("vllm"), InstallAction::Manual { .. }));
        assert!(matches!(install_action("nonsense"), InstallAction::Manual { .. }));
    }
}

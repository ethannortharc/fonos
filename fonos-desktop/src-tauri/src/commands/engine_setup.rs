//! Engine-setup commands (onboarding P3): two-layer engine detection,
//! hardware/disk facts, and install/start/pull orchestration. Pure logic
//! lives in [`fonos_core::engine_setup`]; this module is the shell.

use std::time::Duration;

use serde::Serialize;

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
}

/// Homebrew formula used to install oMLX — kept as one constant so a rename
/// is a one-line change.
pub(crate) const OMLX_BREW_FORMULA: &str = "omlx";

const ENGINES: &[EngineSpec] = &[
    EngineSpec {
        key: "omlx",
        url: "http://localhost:8000",
        process: "omlx",
        binaries: &["omlx", "omlx-server"],
        app_paths: &[],
    },
    EngineSpec {
        key: "lmstudio",
        url: "http://localhost:1234",
        process: "LM Studio",
        binaries: &["lms"],
        app_paths: &["/Applications/LM Studio.app"],
    },
    EngineSpec {
        key: "ollama",
        url: "http://localhost:11434",
        process: "ollama",
        binaries: &["ollama"],
        app_paths: &["/Applications/Ollama.app"],
    },
    EngineSpec {
        key: "vllm",
        url: "http://localhost:8000",
        process: "vllm",
        binaries: &["vllm"],
        app_paths: &[],
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
#[derive(Serialize)]
pub struct EngineDetection {
    /// Engine key: omlx / lmstudio / ollama / vllm.
    pub engine: String,
    /// A live HTTP listener answered `/v1/models`.
    pub running: bool,
    /// Installed on this machine (binary on PATH, app bundle, or a live
    /// process) even if not currently listening.
    pub installed: bool,
    /// Default base URL for the engine.
    pub url: String,
}

async fn detect_one(spec: &'static EngineSpec) -> EngineDetection {
    let (running, _ms, _models) =
        super::scenarios::fetch_models(spec.url, "", Duration::from_secs(2)).await;
    let mut installed = running;
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
    if !installed {
        installed = super::doctor::process_running(spec.process).await;
    }
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

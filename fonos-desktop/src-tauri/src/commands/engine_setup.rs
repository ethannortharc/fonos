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
    /// True when another engine in this table listens on the same default
    /// `url` (omlx and vllm both default to `:8000`). A shared-port engine's
    /// HTTP probe alone can't tell the two apart, so `running` additionally
    /// requires the brand-specific process match.
    shared_port: bool,
}

// Not yet consumed — Task 5 wires the omlx install action that uses this
// (mirrors tray.rs's unlock_body pattern before its consumer landed).
#[allow(dead_code)]
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
}

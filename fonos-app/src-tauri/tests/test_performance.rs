//! Performance and resource tests.
//! Covers: QD-01 (hotkey-to-recording latency < 200ms),
//!         QD-03 (cold start < 3s),
//!         QD-04 (server ready time < 120s),
//!         INV-12 (app RSS < 150MB)

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const FONOS_WORKSPACE: &str = "/Users/ethan/Projects/design/fonos";

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port bind failed");
    listener.local_addr().unwrap().port()
}

fn wait_for_health(port: u16, timeout: Duration) -> Option<Duration> {
    let url = format!("http://127.0.0.1:{port}/v1/health");
    let start = Instant::now();
    let deadline = start + timeout;
    while Instant::now() < deadline {
        if let Ok(resp) = reqwest::blocking::get(&url) {
            if resp.status().is_success() {
                return Some(start.elapsed());
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    None
}

// ---------------------------------------------------------------------------
// QD-01: Hotkey-to-recording latency < 200ms
// ---------------------------------------------------------------------------

/// QD-01: The time from a simulated hotkey press to the first audio capture
/// callback must be less than 200ms.
///
/// This is a failing placeholder — it requires hotkey.rs and capture.rs to be
/// implemented with a timestamp-delta mechanism.
#[test]
#[cfg(not(feature = "ci"))]
fn test_hotkey_to_recording_latency() {
    // --- FAILING PLACEHOLDER ---
    panic!(
        "QD-01 [NOT IMPLEMENTED]: hotkey and audio capture modules not yet available. \
         Implement hotkey.rs and audio/capture.rs, then measure the timestamp delta \
         between press callback and first PCM chunk to verify < 200ms."
    );

    // Reference implementation sketch:
    //
    // use fonos_app::hotkey::HotkeyManager;
    // use fonos_app::audio::capture;
    // use std::sync::{Arc, Mutex};
    //
    // let timestamps: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    // let ts_clone = timestamps.clone();
    //
    // let press_time = Arc::new(Mutex::new(None::<Instant>));
    // let press_clone = press_time.clone();
    //
    // let mut manager = HotkeyManager::new_default();
    // manager.on_press(move || {
    //     *press_clone.lock().unwrap() = Some(Instant::now());
    // });
    // manager.register().unwrap();
    //
    // let _capture = capture::start_capture(move |_chunk| {
    //     ts_clone.lock().unwrap().push(Instant::now());
    // }).unwrap();
    //
    // manager.simulate_press();
    // thread::sleep(Duration::from_millis(500));
    //
    // let press_at = press_time.lock().unwrap().expect("QD-01: press event never fired");
    // let first_chunk = timestamps.lock().unwrap()[0];
    // let latency_ms = (first_chunk - press_at).as_millis();
    //
    // assert!(
    //     latency_ms < 200,
    //     "QD-01: hotkey-to-recording latency {}ms exceeds 200ms threshold", latency_ms
    // );
}

// ---------------------------------------------------------------------------
// QD-03: App cold start < 3s
// ---------------------------------------------------------------------------

/// QD-03: The Tauri app binary should start and emit a readiness signal within
/// 3 seconds. Measured from process spawn to the Tauri "ready" log line or
/// IPC response.
///
/// This is a failing placeholder — it requires a built Tauri binary.
#[test]
#[cfg(not(feature = "ci"))]
fn test_cold_start_time() {
    // --- FAILING PLACEHOLDER ---
    panic!(
        "QD-03 [NOT IMPLEMENTED]: Tauri binary not available. \
         Build with `cargo tauri build --debug`, then measure the time from \
         process spawn to the first successful tray-icon or IPC handshake."
    );

    // Reference sketch:
    //
    // let start = Instant::now();
    // let mut child = Command::new("./target/debug/fonos-app")
    //     .stdout(Stdio::piped())
    //     .spawn()
    //     .expect("QD-03: failed to spawn Fonos app");
    //
    // // Read stdout until "tauri ready" or similar marker.
    // // ...
    //
    // let elapsed = start.elapsed();
    // child.kill().unwrap();
    //
    // assert!(
    //     elapsed.as_secs_f64() < 3.0,
    //     "QD-03: cold start took {:.2}s, exceeds 3s threshold", elapsed.as_secs_f64()
    // );
}

// ---------------------------------------------------------------------------
// QD-04: Server ready time < 120s
// ---------------------------------------------------------------------------

/// QD-04: The Python Fonos server should pass its health check within 120
/// seconds of being spawned.
///
/// Skipped in CI — requires the Python fonos_service to be available via `uv`.
#[test]
#[cfg(not(feature = "ci"))]
fn test_server_ready_time() {
    if !std::path::Path::new(FONOS_WORKSPACE).exists() {
        eprintln!("SKIP: fonos workspace not found");
        return;
    }

    let port = free_port();
    let mut child = Command::new("uv")
        .args([
            "run",
            "uvicorn",
            "fonos_service.server:app",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .current_dir(FONOS_WORKSPACE)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIP: could not spawn server — {e}");
            return;
        }
    };

    let elapsed = wait_for_health(port, Duration::from_secs(120));
    let _ = child.kill();
    let _ = child.wait();

    match elapsed {
        Some(d) => {
            assert!(
                d.as_secs_f64() < 120.0,
                "QD-04: server ready in {:.1}s — should be < 120s",
                d.as_secs_f64()
            );
            println!("QD-04: server ready in {:.1}s", d.as_secs_f64());
        }
        None => {
            panic!("QD-04: server did not pass health check within 120s");
        }
    }
}

// ---------------------------------------------------------------------------
// INV-12: App RSS < 150MB
// ---------------------------------------------------------------------------

/// INV-12: After startup, the Tauri app's resident set size (RSS) should
/// remain below 150MB (excluding the Python server process).
///
/// This is a failing placeholder — it requires a running Tauri app PID.
#[test]
#[cfg(not(feature = "ci"))]
fn test_memory_footprint() {
    // --- FAILING PLACEHOLDER ---
    panic!(
        "INV-12 [NOT IMPLEMENTED]: Tauri binary not available. \
         Build the app, spawn it, read its PID, then query RSS via \
         `ps -o rss= -p <PID>` and assert < 153_600 KB (150MB)."
    );

    // Reference sketch:
    //
    // let mut child = Command::new("./target/debug/fonos-app")
    //     .spawn()
    //     .expect("INV-12: failed to spawn Fonos app");
    //
    // thread::sleep(Duration::from_secs(3)); // allow startup
    //
    // let pid = child.id();
    // let output = Command::new("ps")
    //     .args(["-o", "rss=", "-p", &pid.to_string()])
    //     .output()
    //     .expect("INV-12: failed to query RSS");
    //
    // let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
    //     .trim()
    //     .parse()
    //     .expect("INV-12: failed to parse RSS");
    //
    // let rss_mb = rss_kb / 1024;
    // child.kill().unwrap();
    //
    // assert!(
    //     rss_mb < 150,
    //     "INV-12: app RSS {}MB exceeds 150MB threshold", rss_mb
    // );
}

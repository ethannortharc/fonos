//! Linux system echo cancellation via PulseAudio/PipeWire `module-echo-cancel`.
//!
//! `module-echo-cancel` (WebRTC AEC) creates a virtual sink `fonos_ec_sink` and
//! a virtual source `fonos_ec_src`. Audio played into the sink is passed through
//! to the real output *and* used as the echo reference to cancel it from the
//! source (the real mic). Routing the call's TTS to the sink and capturing from
//! the source therefore removes the assistant's own voice from the mic — the
//! platform-correct equivalent of macOS VPIO.
//!
//! Because [`crate::audio::playback::AudioPlayback`] and the cpal mic both open
//! the *default* device, the pragmatic v1 route is to switch the system default
//! sink/source to the ec pair for the call's duration and restore the prior
//! defaults (and unload the module) when the call ends. [`EchoCancelGuard`]
//! performs the teardown in `Drop`, so every loop-exit path — hangup, timeout,
//! error, or `call_stop` (which flags the loop to unwind) — restores routing.
//!
//! Works on PulseAudio and on PipeWire (its `pipewire-pulse` layer implements
//! `module-echo-cancel`). All commands go through the `pactl` CLI so there is no
//! native binding to maintain.
//!
//! Caveat: switching the *default* sink/source means other apps' audio also
//! routes through the ec pair for the call's duration. This is acceptable — the
//! module passes audio straight through to the real devices; it only adds the
//! echo-cancellation processing.

use std::process::Command;

/// Virtual source name (the echo-cancelled mic) the call captures from.
pub const SOURCE_NAME: &str = "fonos_ec_src";
/// Virtual sink name (the echo-cancelled output) the call plays TTS into.
pub const SINK_NAME: &str = "fonos_ec_sink";

/// Run a `pactl` subcommand, returning trimmed stdout or an error string.
fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| format!("pactl {args:?} failed to run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pactl {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Restores system audio routing and unloads `module-echo-cancel` on drop.
///
/// Idempotent: [`teardown`](Self::teardown) guards against a double unload so an
/// explicit `drop` followed by scope-end drop is safe.
pub struct EchoCancelGuard {
    module_id: String,
    prev_sink: Option<String>,
    prev_source: Option<String>,
    torn_down: bool,
}

impl EchoCancelGuard {
    fn teardown(&mut self) {
        if self.torn_down {
            return;
        }
        self.torn_down = true;
        // Restore the previous defaults first (best-effort), then unload.
        if let Some(sink) = &self.prev_sink {
            let _ = pactl(&["set-default-sink", sink]);
        }
        if let Some(source) = &self.prev_source {
            let _ = pactl(&["set-default-source", source]);
        }
        match pactl(&["unload-module", &self.module_id]) {
            Ok(_) => eprintln!(
                "fonos: linux AEC — module {} unloaded, routing restored",
                self.module_id
            ),
            Err(e) => eprintln!(
                "fonos: linux AEC — unload-module {} failed: {e}",
                self.module_id
            ),
        }
    }
}

impl Drop for EchoCancelGuard {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// Load `module-echo-cancel` and switch the default sink/source to the ec pair.
///
/// On success the caller plays TTS to the default output (now the ec sink) and
/// captures from the ec source. On any failure, whatever succeeded is rolled
/// back (the local guard's `Drop` runs as the `Err` unwinds) and an error is
/// returned so the caller can fall back to the plain cpal + envelope-gating
/// path.
pub fn setup() -> Result<EchoCancelGuard, String> {
    // Snapshot current defaults so we can restore them. An absent/empty value
    // just means "nothing to restore for that one".
    let prev_sink = pactl(&["get-default-sink"]).ok().filter(|s| !s.is_empty());
    let prev_source = pactl(&["get-default-source"]).ok().filter(|s| !s.is_empty());

    let module_id = pactl(&[
        "load-module",
        "module-echo-cancel",
        "aec_method=webrtc",
        &format!("source_name={SOURCE_NAME}"),
        &format!("sink_name={SINK_NAME}"),
        "sink_properties=device.description=fonos-echo-cancel",
        "source_properties=device.description=fonos-echo-cancel",
    ])?;
    if module_id.parse::<u64>().is_err() {
        return Err(format!("load-module returned unexpected id: '{module_id}'"));
    }

    // From here on, hold a guard so any early return rolls the load back.
    let guard = EchoCancelGuard {
        module_id,
        prev_sink,
        prev_source,
        torn_down: false,
    };

    // Switch defaults; on failure `guard` drops here, undoing the load + any
    // partial default change.
    pactl(&["set-default-sink", SINK_NAME])?;
    pactl(&["set-default-source", SOURCE_NAME])?;

    eprintln!(
        "fonos: linux AEC — module-echo-cancel loaded (module {}), defaults → {}/{}",
        guard.module_id, SINK_NAME, SOURCE_NAME
    );
    Ok(guard)
}

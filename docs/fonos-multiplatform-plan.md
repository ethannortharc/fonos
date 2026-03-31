# Fonos Multi-Platform Architecture Plan

**Date**: 2026-03-30 (revised)

## Design Principles

1. **`fonos-core` has ZERO platform code.** It only defines "what" (text, config, HTTP response), never "how to get it" (mic capture, key events, window management).
2. **Platform code lives inside the app that consumes it.** `fonos-desktop` has `platform/`, `fonos-ios` has `Platform/`. They share nothing — desktop platform APIs (CGEvent, xdotool, SendInput) and iOS platform APIs (AVAudioSession, UIKit) have zero overlap.
3. **No separate `fonos-platform` crate.** It would just be a bag of `#[cfg]` conditionals consumed by one app. Keep it simple.

## Directory Structure

```
fonos/
├── Cargo.toml                        # workspace
├── fonos-core/                       # pure business logic, ZERO platform code
│   ├── Cargo.toml                    #   deps: reqwest, rusqlite, serde, tokio
│   └── src/
│       ├── lib.rs
│       ├── config.rs                 #   load/save (dirs crate, auto per-platform)
│       ├── storage.rs                #   Entry/Container CRUD, FTS5, migration
│       ├── modes.rs                  #   dictation mode definitions
│       ├── llm.rs                    #   OpenAI/Anthropic/Google/OpenRouter client
│       ├── audio.rs                  #   WAV encoding (no capture)
│       ├── agent/                    #   AgentProcessor, SkillRegistry, safety
│       ├── meetings/                 #   chunker, summary, export
│       └── stats.rs                  #   usage tracking
│
├── fonos-desktop/                    # Tauri 2 app (macOS + Linux + Windows)
│   ├── src-tauri/
│   │   ├── Cargo.toml               #   deps: fonos-core, tauri, cpal, enigo
│   │   └── src/
│   │       ├── main.rs              #   app setup, hotkey wiring (platform-agnostic)
│   │       ├── commands/            #   thin Tauri commands → fonos-core
│   │       ├── platform/            #   ALL platform-specific code
│   │       │   ├── mod.rs           #   trait defs + re-export current platform
│   │       │   ├── audio.rs         #   cpal capture + rodio playback (one file, cross-platform)
│   │       │   ├── hotkey.rs        #   hotkey backend (cross-platform via rdev/handy-keys)
│   │       │   ├── macos.rs         #   CGEvent, AXUIElement, ScreenCaptureKit, osascript
│   │       │   ├── linux.rs         #   xdotool/wtype/dotool fallback chain, SIGUSR2
│   │       │   └── windows.rs       #   SendInput, Win32 API
│   │       └── skills/              #   built-in skills (shell, clipboard, etc.)
│   ├── src/                         #   React/TypeScript frontend (fully cross-platform)
│   ├── public/                      #   float.html, agent-panel.html, etc.
│   └── swift/                       #   macOS Swift helpers (fonos-audio-capture, fonos-stt-apple)
│
├── fonos-ios/                        # SwiftUI app
│   └── Sources/
│       ├── Platform/                #   AVAudioSession, Speech framework, UIKit
│       └── Bridge/                  #   UniFFI bindings to fonos-core
│
└── docs/
```

## What's Already Cross-Platform (No Changes)

| Component | Location | Why |
|-----------|----------|-----|
| LLM client | `fonos-core/src/llm.rs` | reqwest HTTP calls |
| Config | `fonos-core/src/config.rs` | `dirs` crate auto-resolves per OS |
| Storage | `fonos-core/src/storage.rs` | rusqlite (bundled SQLite) |
| Modes | `fonos-core/src/modes.rs` | pure data + JSON |
| Agent processor | `fonos-core/src/agent/` | pure logic, no platform deps |
| Meetings | `fonos-core/src/meetings/` | audio encoding, export |
| Mic capture | `audio.rs` (cpal) | cpal is cross-platform |
| Audio playback | `playback.rs` (rodio) | rodio is cross-platform |
| Clipboard | `skills/clipboard.rs` (arboard) | arboard is cross-platform |
| Frontend | `src/**/*.tsx` | React + Tauri IPC |

## What Needs Platform Abstraction

### 1. Text Injection — `platform/mod.rs` trait

```rust
pub trait TextInjector: Send + Sync {
    fn inject(&self, text: &str) -> Result<(), String>;
    fn press_enter(&self);
    fn simulate_paste(&self) -> Result<(), String>;
    fn simulate_copy(&self) -> Result<(), String>;
}
```

| Platform | Implementation | Reference |
|----------|---------------|-----------|
| macOS | CGEvent Cmd+V, AXUIElement fallback | Current `injection.rs` |
| Windows | enigo SendInput Ctrl+V | Handy pattern |
| Linux | Runtime-detect: wtype (Wayland) → xdotool (X11) → dotool (fallback) | Handy's `clipboard.rs` chain |

Handy insight: On Linux, detect Wayland vs X11 at runtime (`WAYLAND_DISPLAY` env var), then check which tools are installed via `which`. Use `wl-copy` for clipboard writes on Wayland (better Unicode than Tauri's clipboard plugin).

### 2. Selection Grab/Replace — extend `TextInjector`

```rust
pub trait SelectionHandler: Send + Sync {
    fn grab_selection(&self) -> Result<SelectionContext, String>;
    fn replace_selection(&self, text: &str, target_app: Option<&str>) -> Result<(), String>;
    fn frontmost_app_name(&self) -> String;
}
```

| Platform | Copy | Paste | Activate App |
|----------|------|-------|-------------|
| macOS | CGEvent Cmd+C | CGEvent Cmd+V | `osascript activate` |
| Windows | SendInput Ctrl+C | SendInput Ctrl+V | `SetForegroundWindow` |
| Linux | xdotool/wtype Ctrl+C | xdotool/wtype Ctrl+V | `wmctrl -a` / Wayland: best-effort |

### 3. Hotkeys

| Platform | API | Notes |
|----------|-----|-------|
| macOS | CGEventTap (current) | Works well, supports modifier-only |
| Windows | `RegisterHotKey` + message loop | Standard Win32 |
| Linux | Tauri global-shortcut plugin + SIGUSR2 fallback | Wayland lacks global hotkey support; SIGUSR2 lets external scripts trigger recording |

Handy insight: Support SIGUSR2 on Unix (`kill -USR2 <pid>` toggles recording). This is critical for Wayland desktops and external automation. Cheap to add — just a signal handler thread.

### 4. System Audio Capture (meetings only)

| Platform | API |
|----------|-----|
| macOS | ScreenCaptureKit (current Swift helper) |
| Windows | WASAPI loopback mode |
| Linux | PulseAudio monitor source / Pipewire |

Lower priority — meetings work mic-only on all platforms. System audio is a nice-to-have.

### 5. Skills

| Skill | macOS | Windows | Linux |
|-------|-------|---------|-------|
| Shell | `sh -c` | `cmd /c` or PowerShell | `bash -c` |
| Scripting | `osascript` | PowerShell | `bash` |
| App Control | AppleScript | Win32 COM | `wmctrl`/`xdotool` |
| System Info | `sw_vers`, `sysctl` | `systeminfo`, WMI | `/proc`, `uname` |
| Clipboard | arboard ✅ | arboard ✅ | arboard ✅ |

## Handy Learnings to Adopt

### 1. SmoothedVad for meetings
Current meeting mode sends fixed 10-second chunks. Adding Silero VAD + smoothing would:
- Skip silence → fewer unnecessary STT calls
- Onset gating → prevent false starts from keyboard noise
- Hangover → prevent mid-word cutoffs
- Pre-fill buffer → don't lose the start of speech

**Dependency**: `vad-rs` + bundled `silero_vad_v4.onnx` (~2MB)

### 2. SIGUSR2 external trigger
```rust
#[cfg(unix)]
fn setup_signal_handler(tx: mpsc::Sender<RecordingEvent>) {
    use signal_hook::iterator::Signals;
    let mut signals = Signals::new(&[signal_hook::consts::SIGUSR2]).unwrap();
    std::thread::spawn(move || {
        for _ in signals.forever() {
            let _ = tx.send(RecordingEvent::Toggle);
        }
    });
}
```
Usage: `kill -USR2 $(pgrep fonos-app)` from any script, window manager binding, or Input Leap action.

### 3. Native sample rate + rubato resampling
Current approach forces 16kHz in cpal config. Better: capture at the device's native rate (avoids Bluetooth codec issues), then resample to 16kHz in software via rubato (FFT-based, higher quality than current linear interpolation).

### 4. enigo for cross-platform key simulation
Replace CGEvent direct calls with enigo where possible. enigo wraps platform-specific APIs (CGEvent on macOS, SendInput on Windows, XTest/uinput on Linux) behind a single API. Keep the Linux CLI tool fallback chain for Wayland.

## Release Matrix

| Crate | Artifact | Platforms |
|-------|----------|-----------|
| `fonos-desktop` | `.dmg` (macOS), `.msi` (Windows), `.AppImage`/`.deb` (Linux) | All desktop |
| `fonos-ios` | `.ipa` (App Store) | iOS |
| `fonos-core` | library (not released standalone) | — |

## Workspace Cargo.toml

```toml
[workspace]
members = [
    "fonos-core",
    "fonos-desktop/src-tauri",
]
```

## Implementation Order

### Phase 1: Restructure (macOS still works, no new platforms)
1. Rename `fonos-app/` → `fonos-desktop/`
2. Create `fonos-desktop/src-tauri/src/platform/mod.rs` with trait definitions
3. Move macOS code into `platform/macos.rs` (CGEvent, AXUIElement, osascript, ScreenCaptureKit)
4. Move cpal/rodio into `platform/audio.rs`
5. Wire `main.rs` + `commands/` to call traits
6. Add SIGUSR2 signal handler (Unix only)
7. **Test**: everything identical on macOS

### Phase 2: Linux
1. Implement `platform/linux.rs` — xdotool/wtype injection chain, runtime detection
2. Test hotkeys via Tauri global-shortcut plugin
3. Test mic capture (cpal + ALSA host)
4. Skip system audio (mic-only meetings)
5. **Test**: dictation + agent on Linux

### Phase 3: Windows
1. Implement `platform/windows.rs` — SendInput, RegisterHotKey
2. Test mic capture (cpal + WASAPI)
3. **Test**: dictation + agent on Windows

### Phase 4: Enhancements (all platforms)
1. SmoothedVad for meeting mode
2. Rubato resampling (replace linear interpolation)
3. System audio on Windows (WASAPI loopback) and Linux (PulseAudio/Pipewire)

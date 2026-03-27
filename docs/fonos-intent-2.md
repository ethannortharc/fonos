# Fonos — Intent 2: Voice Assistant App (Tauri)

## Goal

Build a macOS menu bar application that provides WhisperFlow-style voice dictation, TTS playback, voice management, and service monitoring — all powered by the Fonos voice API server (Intent 1). One unified GUI. Users press a hotkey, speak, and text appears at their cursor. The app manages the Python voice server as a child process.

## Dependency

- **Fonos Intent 1** must be completed and passing all invariants. This intent consumes its API at `http://localhost:{FONOS_PORT}`.

## Environment

- **Framework**: Tauri 2.x (Rust backend + HTML/JS/CSS frontend)
- **Platform**: macOS arm64 (Apple Silicon), minimum macOS 13
- **Build**: `cargo tauri build` → `.dmg` installer
- **Frontend**: Vanilla HTML/JS + Tailwind CSS (CDN). No React/Vue — keep bundle minimal.
- **System APIs** (via Rust backend):
  - `CoreAudio` (audio capture from default input device)
  - `CGEvent` / `AXUIElement` (cursor text injection via Accessibility API)
  - `CGEvent` (global hotkey registration)
  - `NSStatusBar` (menu bar icon)

## Architecture

```
fonos-app/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs                # Tauri entry, app setup, tray
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── dictation.rs       # Start/stop recording, get transcript
│   │   │   ├── tts.rs             # TTS playback controls
│   │   │   ├── voices.rs          # Voice CRUD (proxy to API)
│   │   │   └── server.rs          # Python server lifecycle
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs         # CoreAudio microphone capture → PCM stream
│   │   │   └── playback.rs        # Audio output (TTS result playback)
│   │   ├── hotkey.rs              # Global hotkey registration + dispatch
│   │   ├── injection.rs           # Accessibility API text injection
│   │   ├── server_manager.rs      # Spawn/monitor/kill Python server subprocess
│   │   └── config.rs              # Persistent settings (hotkey, voice, mode, port)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── resources/
│       └── tray_icon.png          # 22x22 menu bar icon
├── src/
│   ├── index.html                 # Main window (menu bar popover)
│   ├── styles.css                 # Tailwind utilities + custom
│   ├── app.js                     # App shell, routing, state
│   ├── views/
│   │   ├── dictation.js           # Dictation mode UI (waveform, transcript)
│   │   ├── voices.js              # Voice library + clone UI
│   │   ├── tts.js                 # TTS playground (type → speak)
│   │   ├── settings.js            # Hotkey config, mode select, server control
│   │   └── status.js              # Server health, latency, memory
│   └── lib/
│       ├── api.js                 # Tauri invoke wrappers
│       └── audio-viz.js           # Simple waveform canvas (mic input level)
├── tests/
│   ├── rust/
│   │   ├── test_server_manager.rs # Subprocess lifecycle tests
│   │   ├── test_capture.rs        # Audio capture mock tests
│   │   └── test_config.rs         # Config persistence tests
│   └── e2e/
│       ├── playwright.config.ts
│       ├── test_dictation_flow.ts  # Record → transcript appears in UI
│       ├── test_voice_mgmt.ts      # Clone → list → delete → verify UI
│       ├── test_tts_playback.ts    # Type text → TTS → audio plays
│       └── test_settings.ts        # Change settings → persisted on restart
├── package.json                    # Playwright + Tailwind dev deps
└── README.md
```

## User Flows

### Flow 1: Voice Dictation (primary)

1. User presses global hotkey (default: `Cmd+Shift+Space`, configurable).
2. Menu bar icon turns red — recording active. A small floating indicator appears near cursor.
3. User speaks naturally.
4. Audio streams via WebSocket to Fonos API (`/v1/audio/transcriptions/stream`).
5. Interim transcription text appears in the floating indicator (live preview).
6. User releases hotkey (or presses again to toggle).
7. Final transcription is injected at the current cursor position via Accessibility API.
8. Menu bar icon returns to normal.

### Flow 2: TTS Playback

1. User selects text in any app → triggers TTS hotkey (default: `Cmd+Shift+S`).
2. Selected text is read from clipboard / Accessibility API.
3. Text sent to `/v1/audio/speech` with current voice setting.
4. Audio plays through default output device.
5. Playback controls (pause/stop) accessible from menu bar popover.

### Flow 3: Voice Clone

1. User opens Fonos popover → Voice Library tab.
2. Clicks "Clone Voice" → file picker for reference audio (3-10s).
3. Uploads to `/v1/audio/voices/clone` → progress indicator.
4. New voice appears in library → user can preview with sample text.
5. Voice selectable as default for TTS playback.

### Flow 4: Menu Bar Popover

Click menu bar icon → popover panel with tabs:
- **Dictation**: Current mode indicator, last transcript, waveform viz
- **Voices**: Voice library grid, clone button, preview/delete
- **TTS**: Text input, voice selector, speed slider, generate + play
- **Status**: Server health, STT/TTS latency gauges, memory, uptime
- **Settings**: Hotkey configuration, dictation mode, audio device select, server auto-start toggle

## Dictation Modes

| Mode | Behavior | Post-processing |
|------|----------|-----------------|
| **Raw** | Direct transcript injection | None — exact STT output |
| **Clean** | Inject with formatting | LLM post-process: remove fillers, fix punctuation, capitalize |
| **Command** | Parse voice commands | Pattern match: "delete that", "new line", "select all", etc. |
| **Translate** | Speak Chinese → inject English (or reverse) | STT in source lang → LLM translate → inject |

Mode is selectable from popover or via a modifier key held during hotkey.

## Invariants (automated, must all pass)

1. **App launches**: `cargo tauri build --debug` succeeds. App opens without crash. Tray icon appears in menu bar.

2. **Server lifecycle**: App spawns Python Fonos server on launch. `GET http://localhost:9880/v1/health` returns 200 within 120s. On app quit, Python process is terminated (no orphan).

3. **Server crash recovery**: Kill the Python process externally → app detects within 5s → auto-restarts → health check passes again within 120s.

4. **Hotkey registration**: Global hotkey is registered on launch. Pressing it emits a Tauri event that the frontend receives. (Tested via simulated CGEvent in Rust test.)

5. **Audio capture**: `capture.rs` opens default input device, captures 2s of audio, produces valid PCM buffer (16kHz, 16bit, mono, non-zero samples). (Requires mic permission — CI skippable with `#[cfg(not(ci))]`.)

6. **API proxy roundtrip**: Frontend calls `invoke("transcribe_file", {path: "test.wav"})` → Rust backend POSTs to Fonos API → returns transcript string to frontend. (Tests run with Fonos server up.)

7. **Voice CRUD via UI**: Playwright: open Voices tab → count voices → clone a voice (upload fixture) → count increases by 1 → delete it → count returns to original.

8. **TTS via UI**: Playwright: open TTS tab → enter text → click Generate → audio playback indicator appears → playback completes.

9. **Settings persistence**: Playwright: change hotkey combo in Settings → restart app → hotkey setting preserved. (Config stored in `~/Library/Application Support/com.fonos.app/config.json`.)

10. **Text injection**: `injection.rs` can write a test string to a focused text field via AXUIElement. (Tested by opening TextEdit, focusing it, injecting, then reading back via AX API.)

11. **No orphan processes**: After `cargo tauri build --debug` app is force-killed (SIGKILL), no `python` process with "fonos" in cmdline remains after 5s.

12. **Memory footprint**: Tauri app RSS < 150MB (excluding the Python server).

## Quality Dimensions (automated, measured)

| Dimension | Metric | Target | Measurement |
|-----------|--------|--------|-------------|
| Hotkey-to-recording | Time from keypress to first audio chunk sent | < 200ms | Rust timestamp in hotkey handler vs first WS send |
| Recording-to-injection | Time from key release to text at cursor | < 4s (for 5s utterance) | End-to-end timer in Rust |
| App cold start | Time from `open Fonos.app` to tray icon visible | < 3s (excl. model load) | Timer in main.rs |
| Server ready | Time from app launch to health check pass | < 120s | Timer in server_manager.rs |
| UI responsiveness | Popover open → content rendered | < 300ms | Playwright performance.now() |
| Bundle size | `.dmg` size (without model weights) | < 30MB | `ls -lh` |

## Preferences (human review queue)

1. **Dictation UX feel**: Press-and-hold hotkey should feel instantaneous. The floating indicator should appear within one frame of keypress. Live transcript should update smoothly, not flicker. Release → text injection should feel like typing, not pasting.

2. **Floating indicator design**: Small, unobtrusive, positioned near cursor. Shows live waveform + interim text. Disappears after injection. Should not steal focus from the active app.

3. **Cursor injection fidelity**: Text injected via Accessibility API should appear correctly in: Terminal, VS Code/Cursor, Safari address bar, Slack message box, Notes, TextEdit, Obsidian. (Manual test matrix.)

4. **Menu bar popover polish**: Should feel native macOS — proper vibrancy, rounded corners, smooth show/hide animation. Tabs should switch without layout jank.

5. **Voice library UX**: Voice cards with waveform preview. Clone progress should show percentage. Preview button should play a sample sentence in the cloned voice.

6. **Tray icon states**: Clearly distinct visual states — idle (monochrome), recording (red/pulsing), processing (animated), error (yellow). Should be legible on both light and dark menu bars.

7. **First-run experience**: On first launch, prompt for Microphone and Accessibility permissions with clear explanations of why each is needed. If denied, show which features are unavailable.

## Implementation Notes

### Server Management

- Spawn Python server via `std::process::Command`: `conda run -n fonos python -m uvicorn fonos.server:app --port {port}`.
- Health check poll every 2s until ready. Exponential backoff on failure.
- Capture stdout/stderr into a ring buffer (last 500 lines) for the Status tab.
- On crash (process exit non-zero): log the last 50 lines, auto-restart up to 3 times, then show error in UI.

### Audio Capture

- Use `coreaudio-rs` crate for mic access.
- Capture at 16kHz 16bit mono PCM.
- Ring buffer of 30s max. Send chunks every 200ms to WebSocket.
- VAD hint: send silence detection to frontend for the waveform viz, but let server-side FunASR VAD handle the real endpoint detection.

### Text Injection

- Primary method: `AXUIElementSetAttributeValue` with `kAXValueAttribute` on the focused element.
- Fallback: simulate keyboard events via `CGEventCreateKeyboardEvent` (paste from clipboard).
- Detection: try AX first, if the focused element doesn't support `kAXValueAttribute`, fall back to clipboard paste.
- Must restore original clipboard content after paste fallback.

### Global Hotkey

- Use `CGEventTapCreate` with `kCGEventKeyDown`/`kCGEventKeyUp` event mask.
- Match configurable modifier+key combo.
- Must work even when Fonos is not the frontmost app.
- Registration must survive sleep/wake cycles.

### Config Persistence

- `~/Library/Application Support/com.fonos.app/config.json`
- Schema:
```json
{
  "hotkey_dictation": "cmd+shift+space",
  "hotkey_tts": "cmd+shift+s",
  "dictation_mode": "clean",
  "default_voice": "default",
  "tts_speed": 1.0,
  "server_port": 9880,
  "server_auto_start": true,
  "audio_input_device": "default",
  "audio_output_device": "default",
  "show_floating_indicator": true
}
```

### LLM Post-Processing (Clean/Translate modes)

- The `clean` and `translate` modes need an LLM call for post-processing.
- Default: call Fonos API's future `/v1/chat/completions` proxy endpoint (not in Intent 1 scope).
- Fallback: configurable external LLM endpoint (OpenAI-compat `base_url` in settings).
- If no LLM configured, `clean` mode falls back to `raw`, `translate` mode shows a "configure LLM" prompt.

## Verification Strategy

**Automated (Ratchet executor loop):**

- `cargo test` — Rust unit tests for server_manager, config, audio capture (mocked), hotkey event parsing, injection logic.
- `cargo tauri build --debug` — compilation gate.
- Playwright e2e — UI flows with Fonos API server running. Server started in conftest/globalSetup. Tests cover voice CRUD, TTS generation, settings persistence, dictation transcript display.

**Human review queue (async):**

- Dictation hotkey feel across 5 apps (Terminal, Cursor, Safari, Slack, Notes).
- Cursor injection correctness in each app.
- Floating indicator positioning and disappearance.
- Tray icon visibility on light + dark menu bars.
- Voice clone quality (A/B test with reference).
- First-run permission flow.

```bash
# Full verification
cd fonos-app
cargo test                                    # Rust tests
npm install && npx playwright test            # E2E UI tests
cargo tauri build --debug                     # Build gate
# Then: human review of preferences 1-7
```

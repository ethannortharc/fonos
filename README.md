# Fonos

**A voice-first AI assistant for macOS and Linux.** Hold a hotkey, talk, and Fonos transcribes your speech, runs it through the AI model and prompt *you* choose, and drops the result wherever you need it — your clipboard, the cursor, a notebook, or a live meeting transcript.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB)

<!--
  TODO: add a screenshot or short demo GIF here — it makes a huge difference for a voice app.
  Drop the file in e.g. assets/demo.gif, then replace the line below with:  ![Fonos](assets/demo.gif)
-->
<p align="center"><sub>↑ screenshot / demo GIF goes here</sub></p>

Fonos is **open source and provider-agnostic**: bring your own API keys for OpenAI, Anthropic, Google, OpenRouter and more — or run everything locally with Ollama or LM Studio. Your keys and your transcripts stay on your machine.

## Features

### 🎙️ Dictation
Press-and-hold to record, release to process. Each *mode* is a transcription plus an optional LLM pass with its own prompt.

- Built-in modes: **Raw** (verbatim), **Polish** (natural writing), **Formal**, **Translate**, **Clean** (filler removal).
- Unlimited **custom modes** — your own system prompt, template, temperature, model override, and auto-paste behavior.
- Live activity feed with model + latency, animated waveform, and a horizontal mode selector.

### 📓 Voice Notes
Organize recordings into notebooks.

- **Quick Note** catch-all plus unlimited custom notebooks, each with its own STT/LLM model, processing mode, and prompt.
- Bind up to 3 notebooks to dedicated hotkeys; capture from a compact floating panel.
- Export any notebook to Markdown or JSON.

### 👥 Meeting Capture
Real-time transcription with separate speaker channels.

- **Dual capture** — your mic and system audio (remote participants, via ScreenCaptureKit) are transcribed independently.
- Live timestamped transcript panel, plus an **AI summary** with key points, decisions, and checkable action items when you stop.

### 🤖 Agent
Voice-driven AI conversations with tool execution.

- Hold a hotkey to talk; responses stream with thinking indicators.
- Skill execution with allow/block command lists; optional spoken (TTS) replies.

### 🔊 Voice (TTS & cloning)
- Type or paste text, pick a voice, adjust speed (0.5×–2×), and synthesize.
- **Clone a voice** from a 3–10s recording or an uploaded clip.

### 📊 Stats & Recent
- Usage over 7 / 30 / 90 days: words dictated, session counts, estimated time saved.
- A unified, filterable timeline across dictation, agent, notes, and meetings.

## Install

### macOS
Download the latest **`.dmg`** from [Releases](https://github.com/ethannortharc/fonos/releases/latest), open it, and drag Fonos to Applications. **Apple Silicon, macOS 13.0+.** Signed and notarized — no Gatekeeper workaround needed.

### Linux
Download the **`.deb`** or **`.rpm`** (amd64 / arm64) from [Releases](https://github.com/ethannortharc/fonos/releases/latest):

```bash
sudo apt install ./fonos_*.deb    # Debian / Ubuntu
sudo dnf install ./fonos-*.rpm    # Fedora / RHEL
```

Text injection (paste-at-cursor) needs **`xdotool`** (`sudo apt install xdotool`). On Wayland it works via XWayland.

## Build from source

**Prerequisites**

- [Rust](https://rustup.rs) (stable) and the Tauri CLI — `cargo install tauri-cli --version "^2"`
- [Node.js](https://nodejs.org) 20+
- **macOS:** Xcode Command Line Tools (`xcode-select --install`) — provides `swiftc` for the Speech / ScreenCaptureKit helpers
- **Linux:** the system packages listed in [`.github/workflows/build-linux.yml`](.github/workflows/build-linux.yml) (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libasound2-dev`, …)

**Run it**

```bash
git clone https://github.com/ethannortharc/fonos.git
cd fonos/fonos-desktop
npm ci
cargo tauri dev          # hot-reloading dev build
```

**Package a release** — `.app` / `.dmg` on macOS, `.deb` / `.rpm` on Linux:

```bash
cargo tauri build
```

The compiled macOS helper binaries are checked in, so builds work out of the box. To rebuild them after editing the Swift sources:

```bash
./src-tauri/swift/build.sh   # macOS only
```

## Providers

Configure any mix in **Settings → Models** — set global defaults, then override per-mode or per-notebook.

| Provider | Type | Notes |
|----------|------|-------|
| **OpenAI** | STT · TTS · LLM | Whisper, GPT-4o, TTS-1 |
| **OpenRouter** | STT · LLM | Audio-capable models (Gemini, Voxtral, GPT-Audio) via chat completions |
| **Anthropic** | LLM | Claude models |
| **Google** | LLM | Gemini models |
| **Ollama** | STT · LLM | Local (`localhost:11434`) |
| **LM Studio** | LLM | Local (`localhost:1234`) |
| **OMLX** | STT · LLM | Local (`localhost:8000`) |
| **Custom** | Any | Any OpenAI-compatible endpoint |

STT supports two API paths: **Whisper** multipart upload, or **chat-completions** base64 audio for multimodal models.

## Keyboard shortcuts

All remappable in **Settings → Hotkeys**.

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+Space` | Dictation (hold) |
| `Cmd+Shift+S` | Text-to-speech |
| `Cmd+Shift+A` | Agent (hold) |
| `Cmd+Shift+G` | Toggle agent panel |
| `Option+N` | Note panel (hold) |
| `Cmd+Shift+M` | Toggle meeting capture |
| `Option+1/2/3` | Quick notebooks |

## Repository layout

Fonos is a monorepo. This README covers the **desktop app**.

| Path | What it is |
|------|------------|
| [`fonos-desktop/`](fonos-desktop) | The Tauri desktop app — Rust backend + React / TypeScript UI. |
| [`fonos-core/`](fonos-core) | Platform-independent Rust crate: provider clients (STT/TTS/LLM), modes, meetings, storage, agent, stats. Shared by the apps. |
| [`fonos-ios/`](fonos-ios) | SwiftUI companion app for iOS (app + keyboard extension + widget + App Intents). |

## Tech stack

**Desktop:** Tauri 2 · Rust · React 19 · TypeScript · Vite · Tailwind CSS · SQLite (`rusqlite`). macOS speech and system-audio capture run through small Swift helpers built on `Speech` and `ScreenCaptureKit`.

## Contributing

Issues and pull requests are welcome. Run the tests with:

```bash
cargo test                              # Rust (core + desktop)
cd fonos-desktop && npm run test:e2e    # Playwright end-to-end
```

Some desktop tests need microphone / accessibility permissions. Please keep changes focused and match the surrounding style.

## License

[MIT](LICENSE) © Ethan

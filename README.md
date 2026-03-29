# Fonos

A voice-first AI assistant for macOS. Record, transcribe, process, and organize speech with customizable modes, multi-provider support, and real-time meeting capture.

## Install

Download **Fonos_0.0.1_aarch64.dmg** from the [latest release](https://github.com/holonexai/fonos/releases), open it, and drag Fonos to Applications. Requires macOS 13.0+ on Apple Silicon.

Signed and notarized by Apple. No Gatekeeper workaround needed.

## Quick Start

1. Open Fonos. It sits in the menu bar with a floating pill overlay.
2. Hold **Cmd+Shift+Space** to record. Release to stop.
3. Your speech is transcribed and processed by the selected mode.
4. The result is copied to your clipboard (or routed to your configured destination).

## Features

### Dictation

The primary interface. Press-and-hold to record, release to process.

- **Mode drum-roller** — swipe horizontally to switch modes. The center mode is active.
- **Activity feed** — live timeline showing recordings, transcripts, LLM results, and errors with model info and latency.
- **Waveform visualization** — animated bars during recording.

Built-in modes:

| Mode | Description |
|------|-------------|
| **Raw** | Direct transcription, no processing |
| **Polish** | Natural writing with emotion and tone preserved |
| **Formal** | Professional business writing |
| **Translate** | Translate to a configured target language |
| **Clean** | Speech-to-writing with filler removal |

Create unlimited custom modes with your own system prompts, templates, temperature, model overrides, and auto-paste behavior.

### Voice Notes

Organize voice recordings into notebooks.

- **Quick Note** — default catch-all notebook, always available.
- **Custom notebooks** — create as many as you need, each with its own STT model, LLM model, processing mode (raw / light polish / summarize), and prompt.
- **Notebook hotkeys** — bind up to 3 notebooks to dedicated shortcuts for one-tap capture.
- **Export** — Markdown or JSON export per notebook.
- **Floating note panel** — a compact overlay with a drum-roller notebook selector. Hold the hotkey to record, release to save.

### Meeting Capture

Real-time transcription with separate speaker channels.

- **Dual audio capture** — microphone (your voice) and system audio (remote participants via ScreenCaptureKit) are transcribed independently.
- **Speaker labels** — "Me" for mic, "Audio" for system audio. Each 10-second chunk is transcribed on its own for clean results.
- **Live transcript panel** — draggable floating panel (520px, right-aligned) showing timestamped speaker entries as they arrive.
- **AI summary** — when you stop the meeting, an LLM generates a summary with key points, action items, and decisions.
- **Action item checkboxes** — rendered inline in the summary view. Check them off as you complete tasks.

### Agent

Voice-driven AI conversations with tool execution.

- Hold **Cmd+Shift+A** to speak to the agent.
- Responses stream in real-time with thinking indicators.
- Configurable system prompt, temperature, max tokens.
- Skill execution with allowed/blocked command lists.
- Optional TTS — the agent can speak its responses.

### Voice (TTS)

Text-to-speech synthesis and voice cloning.

- Type or paste text, select a voice, adjust speed (0.5x - 2.0x), and generate.
- **Clone voices** — record a 3-10 second sample or upload an audio file to create a custom voice.

### Stats

Track your usage over 7, 30, or 90 day periods.

- **Daily word count** — bar chart of STT words per day.
- **Session count** — today and period totals.
- **Time saved** — estimated time savings from voice input vs. typing.
- **Breakdown** — STT words, TTS words, LLM tokens.

### Recent

A unified timeline of all entries across dictation, agent, notes, and meetings. Filter by source type. Paginated with 20 entries per page.

## Keyboard Shortcuts

All shortcuts are configurable in Settings > Hotkeys.

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+Space` | Dictation (hold to talk) |
| `Cmd+Shift+S` | Text-to-speech |
| `Cmd+Shift+A` | Agent (hold to talk) |
| `Cmd+Shift+G` | Toggle agent panel |
| `Option+N` | Note panel (hold to talk) |
| `Cmd+Shift+M` | Toggle meeting capture |
| `Option+1/2/3` | Quick notebook shortcuts |

## Floating Pill

When the main window is not focused, a compact floating pill overlay stays on screen:

- Shows the current mode with a drum-roller selector.
- Displays recording state with a red pulse, waveform, and elapsed timer.
- Click to switch modes or access settings.

## Providers

Fonos supports multiple STT, TTS, and LLM providers. Configure them in Settings > Models.

| Provider | Type | Notes |
|----------|------|-------|
| **OpenAI** | STT, TTS, LLM | Whisper, GPT-4o, TTS-1 |
| **OpenRouter** | STT, LLM | 18 audio-capable models via chat completions (Gemini, Voxtral, GPT-Audio) |
| **Anthropic** | LLM | Claude models |
| **Google** | LLM | Gemini models |
| **Ollama** | STT, LLM | Local models (localhost:11434) |
| **LM Studio** | LLM | Local models (localhost:1234) |
| **OMLX** | STT, LLM | Local models (localhost:8000) |
| **Custom** | Any | Any OpenAI-compatible endpoint |

### Dual-Path STT

When adding a model with STT capability, choose the API path:

- **Whisper** (default) — multipart upload to `/v1/audio/transcriptions`. Works with OpenAI, Ollama, any Whisper-compatible server.
- **Chat Completions** — base64 audio in chat messages. Works with OpenRouter, Gemini, Voxtral, GPT-Audio, and other multimodal models.

## Settings

### Models
Register model profiles with provider, API key, base URL, and capabilities (STT/TTS/LLM). Set global defaults for each service. Per-mode and per-notebook overrides are supported.

### Dictation
Configure built-in and custom modes. Each mode shows its processing pipeline: STT model (step 1) and LLM model (step 2). Set STT language and translation target language.

### Notes
Per-notebook configuration: processing mode, STT/LLM model overrides, and custom processing prompts.

### Meeting
Audio source selection, STT/LLM model for transcription and summary, and summary prompt template.

### Hotkeys
Remap all keyboard shortcuts. Click a hotkey field and press your desired key combination.

### Agent
LLM model, system prompt, temperature, max tokens, allowed/blocked commands, and TTS toggle.

## License

Private repository. All rights reserved.

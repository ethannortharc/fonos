# Fonos iOS — Intent: Voice Dictation App

## Goal

Build an iOS app that provides Fonos-style voice dictation with LLM post-processing and multi-destination text routing. Users tap to record, speech is transcribed via configurable STT providers (Apple Speech, OpenAI Whisper, local Fonos server), processed through selectable modes (raw, polish, formal, translate), and routed to any app — Messages, WeChat, Slack, clipboard, or custom URL schemes. No TTS. No voice cloning. Pure dictation-to-action pipeline.

## Dependency

- **Fonos Intent 1** (optional): local STT server at `http://{host}:9880` for on-premises transcription. App also supports cloud STT (OpenAI, Apple) without it.
- **LLM provider** (optional): OpenAI-compatible endpoint for post-processing modes beyond "raw".

## Environment

- **Framework**: SwiftUI, minimum iOS 17
- **Language**: Swift 6
- **Build**: Xcode 16+, Swift Package Manager for dependencies
- **Target**: iPhone (primary), iPad (adaptive layout)
- **System APIs**:
  - `AVAudioEngine` (microphone capture)
  - `Speech` framework (on-device Apple STT, optional)
  - `URLSession` (HTTP API calls to STT/LLM providers)
  - `UIActivityViewController` / custom share targets (text routing)
  - `WidgetKit` (Lock Screen / Home Screen quick-record widget)
  - `AppIntents` (Siri Shortcuts integration)
  - `Live Activities` (recording indicator on Dynamic Island / Lock Screen)

## Architecture

```
fonos-ios/
├── FonosApp/
│   ├── FonosApp.swift               # App entry, scene config
│   ├── Models/
│   │   ├── AppConfig.swift           # Persistent settings (UserDefaults + Codable)
│   │   ├── Mode.swift                # Dictation mode definitions
│   │   ├── ModelProfile.swift        # STT/LLM provider profiles
│   │   ├── Destination.swift         # Text routing targets
│   │   └── DictationSession.swift    # Single recording session state
│   ├── Services/
│   │   ├── AudioCaptureService.swift # AVAudioEngine mic capture → PCM buffer
│   │   ├── STTService.swift          # Protocol + implementations:
│   │   │   ├── AppleSTT.swift        #   On-device Apple Speech
│   │   │   ├── WhisperSTT.swift      #   OpenAI Whisper API
│   │   │   └── FonosSTT.swift        #   Local Fonos server
│   │   ├── LLMService.swift          # OpenAI-compatible chat completions
│   │   └── TextRouter.swift          # Route text to destinations
│   ├── Views/
│   │   ├── DictationView.swift       # Main recording UI (mic button, waveform, result)
│   │   ├── ModePicker.swift          # Horizontal drum-roller mode selector
│   │   ├── HistoryView.swift         # Past dictations with search/filter
│   │   ├── DestinationPicker.swift   # Where to send processed text
│   │   ├── SettingsView.swift        # Providers, modes, destinations config
│   │   │   ├── ProvidersSection.swift
│   │   │   ├── ModesSection.swift
│   │   │   └── DestinationsSection.swift
│   │   └── Components/
│   │       ├── WaveformView.swift    # Live audio level visualization
│   │       ├── RecordButton.swift    # Large circular mic button with states
│   │       └── ActivityCard.swift    # Single dictation result card
│   ├── Extensions/
│   │   └── AudioBuffer+WAV.swift     # PCM → WAV encoding
│   └── Resources/
│       └── Assets.xcassets
├── FonosWidget/
│   ├── FonosWidget.swift             # Home Screen / Lock Screen quick-record
│   └── FonosLiveActivity.swift       # Dynamic Island recording indicator
├── FonosIntents/
│   └── DictateIntent.swift           # Siri Shortcut: "Dictate with Fonos"
├── Tests/
│   ├── STTServiceTests.swift         # Mock API response parsing
│   ├── LLMServiceTests.swift         # Mode prompt construction + response parsing
│   ├── TextRouterTests.swift         # URL scheme generation, clipboard
│   ├── ModeTests.swift               # Mode definitions, template substitution
│   └── ConfigTests.swift             # Settings persistence round-trip
├── Package.swift                     # SPM dependencies (if any)
└── README.md
```

## User Flows

### Flow 1: Quick Dictation (primary)

1. User opens app → large mic button centered on screen.
2. Taps mic → recording starts. Waveform animates. Dynamic Island shows recording indicator.
3. User speaks naturally.
4. Taps again (or lifts finger in hold-to-record mode) → recording stops.
5. Audio sent to configured STT provider → transcript appears.
6. If mode ≠ raw: transcript processed through LLM → processed text replaces transcript.
7. User taps destination (Messages, clipboard, WeChat, etc.) → text routed.

### Flow 2: Widget Quick-Record

1. User taps Fonos widget on Home Screen or Lock Screen.
2. App opens directly into recording state (auto-start).
3. Same flow as Quick Dictation from step 3.

### Flow 3: Siri Shortcut

1. User says "Hey Siri, dictate with Fonos" or runs custom Shortcut.
2. App opens, starts recording immediately.
3. After processing, text can be passed back to the Shortcut for further automation.

### Flow 4: History Review

1. User swipes to History tab.
2. List of past dictation sessions: date, mode, input text, processed output, destination.
3. Tap to view detail, re-send to another destination, or copy.
4. Search by content, filter by mode or date.

## Dictation Modes

Matching desktop Fonos modes. Each mode defines STT params + optional LLM post-processing.

| Mode | Icon | LLM | Behavior |
|------|------|-----|----------|
| **Raw** | 📝 | No | Direct STT output, no processing |
| **Polish** | ✨ | Yes | Remove fillers, natural writing, preserve tone + language |
| **Formal** | 👔 | Yes | Professional business writing, neutral tone |
| **Translate** | 🌐 | Yes | Translate to configured target language |
| **Custom** | ⚙️ | Yes | User-defined system prompt + template |

Custom modes are created in Settings with the same fields as desktop: system prompt, user template (`{text}` placeholder), temperature, max tokens.

## Text Routing Destinations

Processed text can be sent to multiple target types:

| Destination | Mechanism | Example |
|-------------|-----------|---------|
| **Clipboard** | `UIPasteboard.general` | Default — always available |
| **Messages** | `MFMessageComposeViewController` or URL scheme `sms:` | Send to specific contact or number |
| **WeChat** | Universal Link / URL scheme `weixin://` | Share to contact or Moments |
| **Telegram** | URL scheme `tg://msg?text=...` | Share to chat |
| **Slack** | Deep link `slack://channel?team=...&id=...` | Post to channel |
| **Email** | `MFMailComposeViewController` or `mailto:` | Compose with text as body |
| **Notion** | Notion API (`POST /v1/pages`) | Append to configured database |
| **Custom URL** | User-defined URL template with `{text}` placeholder | Any app that accepts URL schemes |
| **Share Sheet** | `UIActivityViewController` | System share to any app |

Users configure their preferred destinations in Settings. The main view shows configured destinations as quick-action buttons below the processed text.

## STT Providers

| Provider | Type | Notes |
|----------|------|-------|
| **Apple Speech** | On-device | Free, offline-capable, `SFSpeechRecognizer` |
| **OpenAI Whisper** | Cloud | Requires API key, high accuracy, multilingual |
| **Fonos Server** | Local network | Requires Intent 1 running on Mac, lowest latency on LAN |
| **Custom** | Cloud | Any OpenAI-compatible `/v1/audio/transcriptions` endpoint |

Provider is selectable per-mode (mode can override the global default, same as desktop).

## Config Persistence

`UserDefaults` + `Codable` structs, mirroring desktop schema:

```json
{
  "stt_provider": "openai",
  "stt_language": "auto",
  "llm_provider": "openai",
  "llm_api_key": "sk-...",
  "llm_base_url": "",
  "dictation_mode": "polish",
  "destinations": [
    {"type": "clipboard", "label": "Copy", "enabled": true},
    {"type": "messages", "label": "iMessage", "config": {"recipient": "+1234567890"}, "enabled": true},
    {"type": "url_scheme", "label": "WeChat", "config": {"url": "weixin://..."}, "enabled": true}
  ],
  "model_profiles": [...],
  "modes": [...],
  "record_mode": "tap",
  "auto_send_destination": "",
  "history_retention_days": 30
}
```

## Invariants (automated, must all pass)

1. **App launches**: Xcode build succeeds for iPhone simulator. App opens without crash. Main dictation view appears.

2. **Audio capture**: `AudioCaptureService` opens mic, records 2s, produces valid PCM buffer (16kHz, 16bit, mono, non-zero samples). Mic permission prompt appears on first use.

3. **Apple STT round-trip**: Record audio (simulator: inject test audio) → `AppleSTT` returns non-empty transcript. No crash on permission denied (returns error gracefully).

4. **OpenAI STT round-trip**: POST fixture WAV to mock OpenAI endpoint → `WhisperSTT` parses response → returns transcript string. Handles 401 (bad key), 400 (bad audio), timeout.

5. **LLM processing**: Send test transcript through "polish" mode → `LLMService` builds correct messages array (system + user with `{text}` substituted) → returns processed text. Handles error gracefully (mode falls back to raw on LLM failure).

6. **Mode definitions**: All built-in modes (raw, polish, formal, translate) load correctly. Custom mode with user-defined system prompt serializes/deserializes via Codable round-trip.

7. **Text routing — clipboard**: Process text → route to clipboard → `UIPasteboard.general.string` matches output.

8. **Text routing — URL scheme**: Route to custom URL destination with template `myapp://send?text={text}` → generated URL correctly encodes text.

9. **Config persistence**: Change settings → kill app → relaunch → settings preserved.

10. **History storage**: Complete a dictation session → session appears in history. Delete session → removed. History query by date range returns correct results.

11. **Offline graceful**: With network off: Apple STT still works (on-device). OpenAI STT returns clear error. LLM processing skipped with user-visible message. App does not crash.

12. **Memory footprint**: App RSS < 80MB during active dictation (excluding Apple Speech framework internals).

## Quality Dimensions (automated, measured)

| Dimension | Metric | Target | Measurement |
|-----------|--------|--------|-------------|
| Tap-to-recording | Time from button tap to first audio buffer | < 150ms | Timestamp diff in AudioCaptureService |
| STT latency (Apple) | Time from stop to transcript (5s audio) | < 2s | Timer in STTService |
| STT latency (OpenAI) | Time from stop to transcript (5s audio) | < 4s | Timer in STTService |
| LLM processing | Time for polish mode on 50-word transcript | < 3s | Timer in LLMService |
| End-to-end | Tap stop → text at destination | < 6s | Timer in DictationSession |
| App cold start | Launch to main view interactive | < 1.5s | Xcode Instruments |
| Bundle size | .ipa size | < 15MB | `ls -lh` after archive |

## Preferences (human review queue)

1. **Recording UX feel**: Tap → instant visual feedback (waveform starts, button turns red, haptic). Should feel as responsive as Voice Memos. Hold-to-record option should feel natural with finger release = stop.

2. **Waveform visualization**: Live audio level bars during recording. Smooth animation, 60fps. Should convey "the app is listening" without being distracting.

3. **Mode picker design**: Horizontal drum-roller matching desktop Fonos. Smooth momentum scrolling, current mode prominent, adjacent modes dimmed. Haptic tick on each mode change.

4. **Destination quick-actions**: After processing, destination buttons should be large, tappable, with clear icons. One-tap send without confirmation (for configured destinations). Show brief "Sent!" feedback.

5. **Dark mode polish**: Primary dark theme matching desktop Fonos (#1a1917 background, amber #fbbf24 accents). Light mode supported but dark is default. All text legible, proper contrast ratios.

6. **Dynamic Island / Live Activity**: When recording in background (e.g., user switches to another app), recording indicator visible on Dynamic Island (compact + expanded). Shows elapsed time and waveform.

7. **Widget design**: Minimal — single large mic button. Tap opens app directly into recording. Should match app's visual language.

8. **History cards**: Each session shows: mode icon, first line of text, destination sent to, timestamp. Expandable to full text. Swipe to delete.

## Implementation Notes

### Audio Capture

- Use `AVAudioEngine` with an input node tap at 16kHz 16-bit mono.
- Buffer audio in memory (ring buffer, 60s max).
- On stop: export as WAV for API upload.
- Request `AVAudioSession.Category.record` with `.measurement` mode for clean capture.
- Handle interruptions (phone call, Siri) gracefully — stop recording, save partial.

### STT Provider Protocol

```swift
protocol STTProvider {
    var id: String { get }
    var name: String { get }
    func transcribe(audioData: Data, language: String?) async throws -> String
}
```

All providers conform to this. The app resolves which provider to use based on mode override → global default chain (same as desktop).

### LLM Integration

- Use `URLSession` with streaming for long responses (though dictation text is short).
- Build messages array from mode's system prompt + user template.
- Handle `max_completion_tokens` for OpenAI (same adapter as desktop).
- Temperature restriction for reasoning models (o-series, nano variants) — omit temperature field.
- If LLM fails, show error inline and offer to copy raw transcript.

### Text Router

```swift
protocol TextDestination {
    var id: String { get }
    var label: String { get }
    var icon: String { get }
    func send(text: String) async throws
}
```

Implementations: `ClipboardDestination`, `MessagesDestination`, `URLSchemeDestination`, `ShareSheetDestination`, `NotionDestination`.

For URL scheme destinations, percent-encode the text and substitute into the user's URL template. Verify the URL can be opened via `UIApplication.shared.canOpenURL` before showing the button.

### Data Persistence

- **Config**: `UserDefaults` with `@AppStorage` property wrappers in SwiftUI.
- **History**: Core Data or SwiftData (prefer SwiftData for iOS 17+ simplicity). Entity: `DictationSession(id, date, mode, inputText, outputText, destination, latencyMs, model)`.
- **Model profiles**: Stored in config, same schema as desktop `ModelProfile`.
- **Keychain**: API keys stored in Keychain (not UserDefaults) via `Security` framework.

### Widget & Live Activity

- **Widget**: `WidgetKit` with `AppIntentTimelineProvider`. Static content (just a mic button). On tap → deep link `fonos://record` → app opens in recording state.
- **Live Activity**: Start `ActivityKit` activity when recording begins. Update with elapsed time. End when recording stops. Shows on Dynamic Island and Lock Screen.

### Siri Shortcut

- Define `DictateIntent` conforming to `AppIntent`. Parameters: mode (optional), destination (optional).
- Returns processed text as the intent result (for Shortcut chaining).
- Donate intent after each dictation for Siri suggestions.

## Design Language

Match desktop Fonos aesthetics adapted for iOS:

- **Background**: `#1a1917` (near-black warm)
- **Primary accent**: `#fbbf24` (amber) for active states, buttons
- **Text**: `#fafaf9` (warm white) primary, `rgba(255,255,255,0.3)` secondary
- **Recording state**: `#ef4444` (red) mic button, pulse animation
- **Success state**: `#86efac` (green) checkmark
- **Cards**: `rgba(255,255,255,0.02)` bg, `rgba(255,255,255,0.04)` border
- **Typography**: SF Pro (system), monospace for timestamps/latency
- **Corner radius**: 16pt cards, 12pt buttons, full-round for mic button
- **Haptics**: `.impact(.light)` on mode change, `.success` on text sent, `.warning` on error

## Verification Strategy

**Automated (Ratchet executor loop):**

- `xcodebuild test` — unit tests for STT mock, LLM mock, TextRouter, Mode definitions, Config persistence.
- `xcodebuild build` — compilation gate for iPhone simulator.
- UI tests via XCTest — recording flow (mocked audio), mode selection, destination routing, history.

**Human review queue (async):**

- Recording UX feel on physical device (haptics, latency, waveform).
- Mode picker scrolling smoothness.
- Destination routing to Messages, WeChat, Telegram (requires real apps).
- Dynamic Island behavior during recording.
- Widget tap-to-record flow.
- Dark mode visual polish.

```bash
# Full verification
cd fonos-ios
xcodebuild test -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16'
xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16'
# Then: human review of preferences 1-8
```

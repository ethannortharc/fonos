# Fonos Full UI Redesign

**Date**: 2026-03-22
**Status**: Approved
**Scope**: Complete frontend visual overhaul — app shell, all 5 views, float pill, Settings model CRUD

## Overview

Pure frontend redesign of the Fonos desktop app. No new features, no backend changes (except removing hardcoded local model defaults). Goals:

- Modern, premium aesthetic (Warm Dark theme)
- Sidebar navigation replacing horizontal tabs
- Tabbed Settings with model profile CRUD
- Redesigned float pill
- Consistent design language across all views

## Design Tokens

### Colors

| Token | Value | Usage |
|---|---|---|
| `--bg` | `#1a1917` | Main background |
| `--bg-sidebar` | `#151413` | Sidebar background |
| `--bg-card` | `rgba(255,255,255,0.02)` | Card / container fill |
| `--border` | `rgba(255,255,255,0.05)` | Default border |
| `--border-hover` | `rgba(255,255,255,0.08)` | Hover border |
| `--text-primary` | `#fafaf9` | Headings, values |
| `--text-secondary` | `rgba(255,255,255,0.35)` | Labels, descriptions |
| `--text-muted` | `rgba(255,255,255,0.2)` | Hints, metadata |
| `--accent` | `#fbbf24` | Primary accent (amber) |
| `--accent-gradient` | `#f59e0b -> #d97706` | Buttons, app icon |
| `--accent-bg` | `rgba(245,158,11,0.12)` | Active chip/nav background |
| `--type-stt` | `#fbbf24` | STT indicators |
| `--type-tts` | `#c4b5fd` | TTS indicators (purple) |
| `--type-llm` | `#86efac` | LLM indicators (green) |
| `--danger` | `#ef4444` | Recording, delete |

### Typography

- **Font**: `-apple-system, SF Pro Display, system-ui, sans-serif`
- **Heading**: 16px / 600 weight
- **Body**: 12-13px / 400 weight
- **Label**: 10px / uppercase / 0.5px tracking / secondary color
- **Mono**: `SF Mono, monospace` for hotkeys, durations

### Spacing & Radius

- Sidebar width: 54px
- Content padding: 20px horizontal, 16px vertical
- Card radius: 10px
- Chip radius: 8px
- Button radius: 8px
- Nav item: 38x38px, radius 10px
- Gap between cards: 8-12px

## Components

### App Shell (`App.tsx`)

Replace the horizontal tab bar with a left sidebar:

```
┌──────┬──────────────────────────────┐
│      │ Content Header               │
│  f   ├──────────────────────────────┤
│  🎙  │                              │
│  🔊  │ Content Body                 │
│  ⏱   │                              │
│  📊  │                              │
│      │                              │
│  ⚙   │                              │
└──────┴──────────────────────────────┘
```

- Sidebar: 54px wide, `--bg-sidebar`, border-right `--border`
- App icon at top: 30x30px, amber gradient, `f` lettermark
- Nav items: 38x38px icon buttons, SVG stroke icons (18px, stroke-width 1.8)
- Active: amber stroke + `--accent-bg` background
- Inactive: `rgba(255,255,255,0.22)` stroke
- Settings gear pinned to bottom with `flex: 1` spacer

Navigation items (top to bottom):
1. Microphone (Dictation)
2. Speaker (Voice)
3. Clock (History)
4. Chart (Stats)
5. [spacer]
6. Gear (Settings)

### Dictation View (`Dictation.tsx`)

Layout:
1. Mode chips row (Raw, Clean, Translate, + custom modes)
2. Waveform area (flex-1, centered, rounded container)
3. Record button (centered, 48px round)
4. Status bar (model name left, hotkey hint right)
5. Transcript card (if result)
6. LLM result card (if processed)

Key changes:
- Waveform container: `--bg-card` with `--border`, 12px radius
- Bar visualization instead of sine wave (vertical bars, amber gradient)
- Record button: amber gradient idle, red gradient + stop icon recording
- Mode chips: rounded 8px, `--accent-bg` active, `--bg-card` inactive
- Waveform stroke color: amber (was blue)

### Voice View (`Voice.tsx`)

Layout:
1. Voice chips (selectable) + `+ Clone` dashed chip
2. Textarea (flex-1, for TTS input)
3. Speed slider (custom styled: amber track, amber knob)
4. "Generate & Play" button (full-width, amber gradient)
5. Result info (if generated)

Key changes:
- Clone chip: dashed border in muted amber, opens clone flow
- Custom range slider: replace native `accent-[#0a84ff]` with styled amber slider
- Button: amber gradient primary, not blue

### History View (`History.tsx`)

Layout:
1. Header with title left, filter chips right (All/STT/TTS/LLM)
2. Event list (scrollable)
3. "Load more" at bottom

Key changes:
- Replace type badge chips (`bg-blue-500/20 text-blue-300`) with 6px colored dots
- Use relative timestamps ("Today, 14:32" / "Yesterday, 22:18")
- Cleaner row: dot + timestamp + spacer + metadata (duration, latency)
- Text preview below on its own line
- Expand on click for full detail

### Stats View (`Stats.tsx`)

Layout:
1. Header with title left, period chips right (7d/30d/90d)
2. 3-column summary tiles (words, sessions, time saved)
3. Bar chart card (amber gradient bars)
4. Inline breakdown (dots + text, no card wrapper)

Key changes:
- Summary tiles: `--bg-card`, label/value/sub structure
- Bar chart: amber gradient bars (opacity varies with value)
- Breakdown: inline row with colored dots, not separate card sections
- Chart bar color: amber (was blue)

### Settings View (`Settings.tsx`)

Major restructure with sub-tabs:

**Tab bar**: Models | Services | Hotkeys | Modes | Language
- Active: amber text + amber bottom border (2px)
- Inactive: `--text-secondary`

**Models tab** (new):
- List of model profile cards showing: name, provider · base_url, capability badges
- Capability badges: colored per type (STT=amber, TTS=purple, LLM=green)
- Edit/delete on hover or click
- "+ Add Model" button at bottom

**Add Model flow**:
1. Provider picker: OpenAI, Anthropic, Google, Ollama, LM Studio, OMLX, Custom
2. Auto-fill base_url per provider:
   - OpenAI: `https://api.openai.com`
   - Anthropic: `https://api.anthropic.com`
   - Google: `https://generativelanguage.googleapis.com`
   - Ollama: `http://localhost:11434`
   - LM Studio: `http://localhost:1234`
   - OMLX: `http://localhost:8000`
   - Custom: empty (user fills in)
3. Fields: Name, Model ID, API Key (if needed), Base URL (pre-filled), Capabilities (multi-select: STT/TTS/LLM)
4. Save adds to `config.model_profiles`

**Services tab**: 3 dropdown pickers (STT/TTS/LLM) selecting from model profiles. Each dropdown filters `model_profiles` by matching capability — e.g., the STT dropdown only shows profiles that include `"stt"` in their `capabilities` array. Same for TTS and LLM.

**Hotkeys tab**: same as current, restyled with warm theme

**Modes tab**: same as current, restyled with warm theme

**Language tab**: same as current, restyled with warm theme

### Float Pill (`float.html`)

Three states:

1. **Idle**: `FONOS` text in muted amber, two small amber dots flanking, warm dark background
2. **Recording**: red pulse dot + amber waveform bars + timer, warm dark background with red border hint
3. **Hover toolbar**: language globe + settings gear icons

Key changes:
- Replace cyan (`rgba(0,212,255,...)`) with amber throughout
- Background: `rgba(26,25,23,0.75)` (warm, was cool gray)
- Border: `rgba(255,255,255,0.06)` (was `0.04`)
- Recording border: `rgba(255,69,58,0.15)`
- Waveform color: `rgba(251,191,36,0.5)` (was cyan)

## Backend Change

**`fonos-core/src/config.rs`**: Remove hardcoded local model defaults. Change `model_profiles` default to empty vec:

```rust
model_profiles: vec![],
stt_profile: String::new(),
tts_profile: String::new(),
```

This means first-run users see an empty model registry and must add their own profiles.

## Files Modified

| File | Change |
|---|---|
| `fonos-app/src/App.tsx` | Replace tab bar with sidebar shell |
| `fonos-app/src/views/Dictation.tsx` | Warm theme, bar waveform, amber accents |
| `fonos-app/src/views/Voice.tsx` | Warm theme, custom slider, clone chip |
| `fonos-app/src/views/History.tsx` | Dots, relative time, cleaner rows |
| `fonos-app/src/views/Stats.tsx` | Amber chart, inline breakdown |
| `fonos-app/src/views/Settings.tsx` | Sub-tabs, model CRUD, provider-first add |
| `fonos-app/src/index.css` | CSS custom properties for design tokens |
| `fonos-app/src/float.html` | Warm theme, amber accents (raw HTML/CSS/JS, not React) |
| `fonos-core/src/config.rs` | Remove hardcoded local model defaults |

## Non-Goals

- No new Tauri commands (model CRUD is pure config JSON manipulation)
- No external component libraries (raw Tailwind)
- No state management library (hooks + context only)
- No changes to backend logic, IPC surface, or database

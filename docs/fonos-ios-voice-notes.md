# Fonos iOS — Voice Notes

**Status**: Idea / Pre-implementation
**Date**: 2026-03-28

## Context

The desktop app has a fully working voice note system: a floating panel with notebook selection, per-notebook STT/LLM configuration, hotkey-triggered recording, and SQLite-backed storage with full-text search. The iOS app currently supports dictation only (record, transcribe, process, route to destination) with SwiftData-based `DictationSession` history. This document outlines how to bring voice notes to iOS, keeping the door open for future iCloud sync between platforms.

## What Desktop Voice Notes Do

1. **Record** — User triggers a hotkey; a floating panel appears with notebook selector.
2. **Transcribe** — Audio goes through STT (Apple on-device or HTTP Whisper).
3. **Process** — Transcript is optionally refined by an LLM (raw, light_polish, summarize) with per-notebook prompt configuration.
4. **Store** — Result lands as an `Entry` (source_type = Note) inside a `Container` (type = Notebook) in SQLite.
5. **Browse** — Notes view shows notebooks as a grid; tap to see chronological entries with inline edit, delete, audio playback, and Markdown/JSON export.

Key design decisions on desktop:
- Audio is NOT saved for notes (unlike meetings). Notes are text-only after transcription.
- Each notebook can override the global STT model, LLM model, and processing prompt.
- A "Quick Note" default notebook catches notes when no specific notebook is selected.
- Up to 3 notebooks can be bound to dedicated hotkeys for one-tap capture.

## iOS Voice Notes — Scope

### Core Flow (v1)

```
[Record Button / Widget] --> [STT] --> [Optional LLM polish] --> [Store in Notebook]
                                                                       |
                                                               [Notes Tab in app]
```

### Data Model

Reuse the same conceptual model as desktop so that a future sync layer doesn't require schema translation:

```swift
// Mirrors fonos-core Container
struct NoteContainer: Identifiable {
    let id: UUID
    var title: String
    var containerType: ContainerType   // .notebook
    var metadata: [String: Any]        // per-notebook STT/LLM overrides, processing mode
    var createdAt: Date
    var updatedAt: Date
}

// Mirrors fonos-core Entry
struct NoteEntry: Identifiable {
    let id: UUID
    var createdAt: Date
    var sourceType: SourceType         // .note
    var rawText: String
    var processedText: String?
    var containerId: UUID              // parent notebook
    var mode: String                   // "raw", "light_polish", "summarize"
    var metadata: [String: Any]        // duration_ms, language, etc.
}
```

Storage backend: **SwiftData** (consistent with the existing `DictationSession` model). If iCloud sync is added later, SwiftData supports CloudKit-backed persistent stores with minimal code changes — just switch the `ModelConfiguration` to use a CloudKit container.

### UI Additions

| Screen | Description |
|--------|-------------|
| **Notes Tab** | New tab in the main TabView. Shows notebook grid (same layout as desktop). Tap to open notebook detail with entry list. |
| **Notebook Detail** | Chronological entries. Swipe to delete. Tap to edit inline. Long-press for share/export. |
| **Record Sheet** | Modal or inline recorder within a notebook. Shows waveform, stop button. Auto-dismisses after transcription completes. |
| **Quick Note Widget** | iOS home screen widget (WidgetKit). Tap to launch app directly into recording for the Quick Note notebook. |
| **Notebook Settings** | Per-notebook configuration: processing mode, STT model override, LLM model override, custom prompt. Accessible via notebook context menu. |

### Services to Reuse

The iOS app already has these services that voice notes can use directly:

- **AudioCaptureService** — mic recording, WAV generation
- **STTService** — Apple on-device or HTTP-based transcription
- **LLMService** — text processing with mode-specific prompts
- **KeychainStore** — API key storage

New service needed:

- **NoteService** — CRUD for NoteContainer and NoteEntry via SwiftData. Handles Quick Note fallback, notebook ordering, and export (Markdown/JSON matching desktop format).

### Recording Triggers

| Trigger | Behavior |
|---------|----------|
| Notes tab record button | Record into selected notebook |
| Quick Note widget tap | Launch app, record into Quick Note |
| Siri Shortcut | "Record a note in [notebook]" via FonosIntents |
| Keyboard extension | Optional: long-press mic to switch from dictation to note mode |

## Future: iCloud Sync

Not in v1, but the architecture should not block it. Key considerations:

**Sync-friendly ID scheme**: Use `UUID` for all IDs on iOS. Desktop currently uses SQLite auto-increment `i64`. A future sync layer would need a mapping table or migration to UUIDs on both sides.

**SwiftData + CloudKit path**: SwiftData can sync to CloudKit with `ModelConfiguration(cloudKitDatabase: .private("iCloud.com.holonex.fonos"))`. This gives iOS-to-iOS sync for free. Desktop sync would require a separate bridge (CloudKit JS, or a shared REST API).

**Conflict resolution**: Notes are append-mostly (new entries are created, rarely edited). For the rare edit case, last-write-wins by `updatedAt` timestamp is sufficient.

**Shared export format**: Both desktop and iOS should produce identical Markdown and JSON exports. This serves as a manual sync path before automated sync is built, and as a verification tool after.

**What to avoid now**: Don't bake in CloudKit-specific fields or sync tokens into the base model. Keep the data model clean and add sync metadata as a separate concern later.

## Implementation Order

1. **SwiftData models** — NoteContainer + NoteEntry with basic CRUD
2. **NoteService** — Business logic layer (create notebook, record note, process, store)
3. **Notes Tab UI** — Notebook grid + entry list (read-only first, then edit/delete)
4. **Record flow** — Wire AudioCaptureService -> STTService -> optional LLMService -> NoteService
5. **Per-notebook config** — Processing mode, model overrides in notebook metadata
6. **Quick Note widget** — WidgetKit deep-link to record
7. **Export** — Markdown/JSON matching desktop format
8. **Siri Shortcuts** — FonosIntents integration for voice-triggered notes

## Non-Goals (v1)

- Audio file storage (match desktop: notes are text after transcription)
- Real-time sync between iOS and desktop
- Shared notebooks across devices
- Collaborative editing
- Rich text or image attachments

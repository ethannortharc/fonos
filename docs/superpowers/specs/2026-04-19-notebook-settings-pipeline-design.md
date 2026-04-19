# Notebook Settings & Pipeline Redesign (iOS)

**Date**: 2026-04-19
**Status**: Approved
**Scope**: iOS app only — `fonos-ios/`. No desktop / core changes.

## Overview

The iOS Notebook feature ships and works, but per-notebook settings have three problems:

1. **Pipeline is silently broken.** `NoteViewModel.recordAndStore` passes `language: nil` to STT and `prompt: nil` to LLM. The `LLMServiceNoteAdapter` hardcodes `.polish` regardless of the notebook's `processingMode`. The `customPrompt` field is collected by the UI and persisted, but **never reaches the LLM**. Result: a Chinese-language note can come back in English with no obvious cause.
2. **Settings UI is opaque.** `processingMode` exposes `raw / light_polish / summarize` — but `summarize` doesn't exist in `Mode.swift`, and the user can't see the resulting pipeline at a glance.
3. **No system-level launcher.** `RecordNoteIntent` exists but isn't surfaced as an `AppShortcut`, so users can't say "Hey Siri, record a note in Fonos" or assign it to Back Tap / Action Button without manually wiring a Shortcut.

This spec collapses Notebook configuration to a single `systemPrompt` field (Notebook *is* the mode), wires the pipeline end-to-end with explicit STT and Output language hints, restructures `NotebookSettingsView` around a visual pipeline metaphor, and registers an `AppShortcutsProvider` with dynamic per-notebook options.

## Goals

- Per-notebook config that **actually takes effect** for STT language and LLM behavior.
- Make the pipeline ("STT → LLM → Output") legible at-a-glance, so the user can locate which step misbehaved.
- Remove redundant abstractions: a Notebook is itself a processing recipe; an inner Mode picker is dead weight.
- Surface notebook recording as a Siri / Shortcuts / Back Tap entry point, the closest iOS equivalent of a "system shortcut".

## Non-Goals

- No changes to `fonos-core`, `fonos-desktop`, or shared Rust crates.
- No new STT or LLM providers (only wires the existing `AppleSTT`, `WhisperSTT`, `LLMService`).
- No `Mode` enum changes — Dictation continues to use it. Notebook is decoupled.
- No iOS keyboard extension changes.

## Design

### Data Model

`NoteContainer` (`fonos-ios/FonosApp/Models/NoteContainer.swift`) gains four new fields and reframes the existing `customPrompt` as `systemPrompt`:

```swift
@Model
final class NoteContainer {
    var id: UUID
    var title: String
    var containerType: String
    var systemPrompt: String              // NEW. "" = Raw (no LLM). Replaces customPrompt + processingMode.
    var sttLanguage: String?              // NEW. BCP-47 locale (e.g., "zh-CN"). nil = system default.
    var outputLanguage: String?           // NEW. nil = follow sttLanguage.
    var sttModelOverride: String?         // existing
    var llmModelOverride: String?         // existing
    var showRawInline: Bool = false       // NEW. Controls EntryRow display.
    var siriPhrase: String?               // NEW. nil = "Record to {title}".
    var createdAt: Date
    var updatedAt: Date

    // DEPRECATED — kept for one release for migration backfill, removed in next minor.
    var processingMode: String            // existing — to be removed in v0.3.x
    var customPrompt: String?             // existing — to be removed in v0.3.x
}
```

Design decisions:

- **`systemPrompt` collapses `processingMode` + `customPrompt`.** A Notebook is already a context commitment. The "mode picker inside a per-notebook setting" was redundant — committing to "Polish mode" inside a Polish-only notebook adds no information. One free-text prompt is more honest, more flexible, and removes a broken enum.
- **Empty prompt = Raw.** `systemPrompt.trimmed.isEmpty` is the only "no LLM" signal. No explicit `enableLLM` flag is needed.
- **`outputLanguage` is independent of `sttLanguage`.** This is what fixes "Chinese in, English out": the LLM gets an explicit language directive, separate from what STT heard.
- **`showRawInline` defaults to `false`** to keep existing-user UI identical until they opt in.

### Pipeline Builder

New file: `fonos-ios/FonosApp/Services/NotebookPipeline.swift`

```swift
/// Pure translation from a NoteContainer into the parameters its STT + LLM call need.
/// Single source of truth for both NoteViewModel (runtime) and NotebookSettingsView (chip summary).
enum NotebookPipeline {
    static func resolve(_ n: NoteContainer) -> Resolved {
        let trimmed = n.systemPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let outLang = n.outputLanguage ?? n.sttLanguage
        let llm: NotebookLLMConfig? = trimmed.isEmpty ? nil : NotebookLLMConfig(
            systemPrompt: trimmed,
            outputLanguage: outLang,
            modelOverride: n.llmModelOverride
        )
        return Resolved(sttLanguage: n.sttLanguage,
                        sttModelOverride: n.sttModelOverride,
                        llm: llm)
    }

    struct Resolved {
        let sttLanguage: String?
        let sttModelOverride: String?
        let llm: NotebookLLMConfig?
    }
}

struct NotebookLLMConfig: Equatable {
    let systemPrompt: String
    let outputLanguage: String?
    let modelOverride: String?
}
```

`LLMService` (`fonos-ios/FonosApp/Services/LLMService.swift`) gains a sibling method that does *not* go through the `Mode` enum:

```swift
extension LLMService {
    /// Note-pipeline entry point. Composes a final system prompt by injecting
    /// `outputLanguage` if present, then calls the chat completions endpoint.
    func processNote(text: String, config: NotebookLLMConfig) async throws -> String { ... }
}
```

The output-language injection is the one piece of business logic with multiple valid approaches (prefix vs suffix vs structured tag). It is intentionally implemented as a small private helper `composeSystemPrompt(_:outputLanguage:)` so the strategy can be tuned independently. **Default chosen for v1**: prefix line `"Always respond in {language}."` followed by a blank line and the user's prompt — strongest steering for chat models, easy to read in the editor.

### Recording Flow Changes

`NoteViewModel.recordAndStore` (`fonos-ios/FonosApp/Services/NoteViewModel.swift`) is rewritten to use `NotebookPipeline`:

```swift
func recordAndStore(to containerId: UUID, audioData: Data) async {
    let notebook = noteService.notebook(id: containerId) ?? quickNoteFallback
    let resolved = NotebookPipeline.resolve(notebook)

    do {
        let rawText = try await resolvedSTT.transcribe(
            audioData: audioData,
            language: resolved.sttLanguage          // was: nil
        )

        var processedText: String? = nil
        if let llmConfig = resolved.llm,           // was: if mode != "raw"
           let llmService = resolvedLLMService() {  // returns LLMService now, not adapter
            do {
                processedText = try await llmService.processNote(
                    text: rawText, config: llmConfig
                )
            } catch {
                noteVMLog.warning("LLM processing failed: \(error.localizedDescription)")
            }
        }

        noteService.addEntry(
            to: containerId, rawText: rawText,
            processedText: processedText,
            mode: resolved.llm == nil ? "raw" : "llm",
            language: resolved.sttLanguage
        )
        recordingState = .done
    } catch { ... }
}
```

Notes:
- `mode` parameter on `addEntry` becomes a coarse flag (`"raw"` vs `"llm"`) used only for the EntryRow badge — the real config lives on the parent notebook.
- The `language` field already exists on `NoteEntry`; we now actually populate it.
- `LLMServiceNoteAdapter` is **deleted**. The hardcoded `.polish` was the bug.

### Settings UI

`NotebookSettingsView.swift` is restructured around the **Pipeline-Indexed** layout (mockup B, approved). Same `Color(hex: "#1a1917")` background, same `RoundedRectangle(cornerRadius: 12-14)` cards, same amber `#fbbf24` accent — pure structural reorganization within the existing visual language.

Top of the screen, below the navbar:

```
┌─────────────────────────────────────────┐
│  zh-CN  →  AI  →  中文                  │  ← amber pipeline chip, monospaced
└─────────────────────────────────────────┘
```

Sections (in order):

1. **General** — Notebook Name (existing)
2. **① Speech-to-Text**
   - Language picker (sub-screen): Auto / 中文 (zh-CN) / English (en-US) / 日本語 (ja-JP) / 한국어 (ko-KR) / Français (fr-FR) / Deutsch (de-DE) / Español (es-ES) / More…
   - STT Model picker (existing override, but as a Picker against `config.modelProfiles`, not a free-text field)
3. **② LLM Processing**
   - System Prompt — multi-line `TextField` (10–12 line height, autocorrect + autocapitalization off)
   - Output Language picker — "Same as STT" + same locale list as STT Language
   - LLM Model picker — same Picker pattern as STT Model
4. **③ Display**
   - Show raw transcript inline — `Toggle`, default off
5. **Shortcut**
   - Siri Phrase — read-only row showing `siriPhrase ?? "Record to \(title)"`
   - Open in Shortcuts — amber button, opens `URL(string: "shortcuts://")`
6. **Danger Zone** — Delete Notebook (existing, unchanged)

The pipeline chip is computed by the same `NotebookPipeline.resolve` used at runtime — single source of truth, can never disagree with what the recording flow actually does.

The free-text fields for `sttModelOverride` and `llmModelOverride` are replaced by `Picker` against `config.modelProfiles` (existing in `AppConfig`). This eliminates a category of "I typed the wrong model id" bugs.

### EntryRow Display

`NotebookDetailView.swift` `EntryRow` accepts a `notebook: NoteContainer` parameter. The existing `processed != raw` branch (lines 127–132) is gated on `notebook.showRawInline`:

```swift
if let processed = entry.processedText,
   processed != entry.rawText,
   notebook.showRawInline {
    Text(entry.rawText)
        .font(.system(size: 12))
        .foregroundColor(Color(hex: "#fafaf9").opacity(0.35))
        .lineLimit(2)
}
```

When `showRawInline == false` the row is unchanged from today. A long-press on any EntryRow surfaces a one-shot "View raw transcript" context menu regardless of the toggle, so the data is always reachable without changing the default UI for existing users.

### Shortcuts Integration

New file: `fonos-ios/FonosIntents/FonosAppShortcuts.swift`

```swift
struct FonosAppShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: RecordNoteIntent(),
            phrases: [
                "Record a note in \(.applicationName)",
                "Take a voice note in \(.applicationName)",
                "Record to \(\.$notebookId) in \(.applicationName)"
            ],
            shortTitle: "Record Note",
            systemImageName: "mic.circle"
        )
    }
}
```

`RecordNoteIntent.notebookId` (`fonos-ios/FonosIntents/RecordNoteIntent.swift`) is upgraded so Siri / Shortcuts.app can list real notebook titles instead of asking for raw UUIDs. Two well-known App Intents patterns work here:

- **Preferred — make `Notebook` an `AppEntity`** with a `DefaultEntityQuery`. The parameter becomes `var notebook: NotebookEntity?` and the system renders its display representation natively.
- **Fallback — `DynamicOptionsProvider`** if upgrading to AppEntity is too invasive in v1: keep `notebookId: String?` and provide a list of `(uuid, title)` pairs.

Either pattern requires the `FonosIntents` target to read notebook metadata while the main app may not be running, since Siri can query options ahead of time. Mechanism: persist a small JSON catalog (`notebooks.json` containing `[{id, title}]`) to a **shared App Group container** (e.g., `group.com.fonos.ios`), written by `NoteService` after every `createNotebook` / `renameNotebook` / `deleteNotebook`. The Intents target reads this file synchronously — no SwiftData container needed in the extension.

```swift
// Sketch — exact API (AppEntity vs DynamicOptionsProvider) chosen during implementation.
struct NotebookOptionsProvider: DynamicOptionsProvider {
    func results() async throws -> [String] {
        // Reads from App Group `notebooks.json`. App writes; Intents target reads.
        SharedNotebookCatalog.read().map(\.title)
    }
}
```

After every notebook CRUD, `NoteService` writes the catalog to the App Group **and** calls `FonosAppShortcuts.updateAppShortcutParameters()` so the suggestion list stays fresh. Settings → Shortcut → "Open in Shortcuts" opens the system Shortcuts app (`shortcuts://`) where the user can assign the exposed shortcut to Back Tap or the Action Button — the closest iOS equivalent to a "system keyboard shortcut".

### New Notebook Templates

`NewNotebookSheet` (in `NotesView.swift`) gains a row of template chips below the name field. Picking a template seeds the new `NoteContainer.systemPrompt`:

| Template | systemPrompt seed |
|---|---|
| Raw | `""` |
| Polish | `"Clean up filler words and disfluencies. Preserve the speaker's original meaning and tone."` |
| Meeting Notes | `"Summarize as bullet-point meeting minutes with action items grouped at the bottom."` |
| Translate | `"Translate the text accurately to the target language. Preserve tone."` |
| Blank | `""` (user fills in from scratch) |

Templates are *seeds*, not types — once the notebook exists, only `systemPrompt` matters and the user can edit it freely. Avoids the "template ↔ instance sync" complexity.

### Migration

SwiftData lightweight migration covers the *added* fields (`systemPrompt`, `sttLanguage`, `outputLanguage`, `showRawInline`, `siriPhrase`) automatically because they are optional or have defaults. The risky part is consolidating `processingMode + customPrompt` into `systemPrompt`.

Strategy — **two-release migration**:

- **This release (v0.2.0)**: keep `processingMode` and `customPrompt` on the model (deprecated, no longer read by the runtime). On `NoteService.init`, after the `ModelContainer` opens, run a one-shot backfill:
  ```swift
  let migratedKey = "notebookConfig.migrated.v2"
  if !UserDefaults.standard.bool(forKey: migratedKey) {
      for nb in allNotebooks() where nb.systemPrompt.isEmpty {
          nb.systemPrompt = backfillPrompt(processingMode: nb.processingMode,
                                           customPrompt: nb.customPrompt)
          nb.updatedAt = Date()
      }
      try? modelContainer.mainContext.save()
      UserDefaults.standard.set(true, forKey: migratedKey)
  }
  ```
  Backfill table:
  - `processingMode == "raw"` → `""`
  - `processingMode == "polish"` or `"light_polish"` → Polish template seed
  - `processingMode == "summarize"` → Meeting Notes template seed
  - else if `customPrompt` non-empty → `customPrompt`
  - else → `""`
- **Next release (v0.3.x)**: remove `processingMode` and `customPrompt` from the schema, with a real `SchemaMigrationPlan` step that drops the columns.

This keeps the risky cleanup in a separate release where it's the only schema change, so a problem can be diagnosed without entanglement.

## Testing

Following the existing `fonos-ios/FonosAppTests/NoteINV*` naming.

| File | Status | Cases |
|---|---|---|
| `NoteINV08NotebookConfigTests.swift` | extend | new fields persist; defaults correct (`showRawInline == false`, optionals nil); backfill maps `polish/summarize/raw/customPrompt` → `systemPrompt` correctly; backfill is idempotent |
| `NotebookPipelineTests.swift` | new | empty `systemPrompt` → `llm == nil`; non-empty → llm with `outputLanguage` fallback chain (`outputLanguage ?? sttLanguage ?? nil`); whitespace-only prompt treated as empty |
| `LLMServiceTests.swift` | extend | `processNote(text:config:)` injects `outputLanguage` as prompt prefix; nil `outputLanguage` leaves prompt unchanged; HTTP 401/timeout still maps to existing `LLMError` cases |
| `NoteINV06RecordFlowTests.swift` | extend | injected STT receives `language == notebook.sttLanguage` (not nil); empty prompt → LLM not invoked; non-empty prompt → LLM invoked once with the right config; LLM failure does not block raw save |
| `NoteINV09IntentTests.swift` | extend | `FonosAppShortcuts.appShortcuts` registers `RecordNoteIntent` with the three phrases; `updateAppShortcutParameters` is invoked after `createNotebook` / `deleteNotebook`; `NotebookOptionsProvider` returns current notebook titles |
| `NotebookSettingsViewSnapshotTests.swift` | optional / follow-up | screenshot regression for the new pipeline-indexed layout (deferred — not blocking for v0.2.0) |

Existing UI test in `FonosAppUITests.swift` should still pass; if it asserts on the old `processingMode` Picker labels it needs a one-line update.

## Out of Scope (explicitly)

- Per-notebook *temperature* / *maxTokens* — single global default in v1.
- Per-notebook icon / color — already not part of UI today.
- Cross-device notebook sync — not regressed, not improved.
- Sharing a notebook recipe to another user (export/import templates) — future work.

## Migration / Rollout

- v0.2.0 ships with both old and new fields, runtime reads new only, backfill on first launch.
- v0.3.0 removes deprecated fields with a `SchemaMigrationPlan`.
- No `AppConfig` changes; no Keychain changes; no Provider list changes.
- Existing notebooks continue to work — their settings are translated, not lost.

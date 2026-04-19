# Notebook Settings & Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire per-notebook STT language and LLM prompt config into the iOS recording pipeline, restructure `NotebookSettingsView` around a pipeline metaphor, register `RecordNoteIntent` as an `AppShortcut` so each notebook is reachable from Siri / Back Tap / Action Button.

**Architecture:** Collapse `processingMode + customPrompt` on `NoteContainer` into a single `systemPrompt` (Notebook is itself the mode). Introduce `NotebookPipeline.resolve(_:)` as a pure function consumed by both `NoteViewModel` (runtime) and `NotebookSettingsView` (chip summary) — single source of truth. Persist a `notebooks.json` catalog into the existing App Group `group.com.fonos.ios` so the `FonosIntents` target can list notebooks for `DynamicOptionsProvider` without booting the main app.

**Tech Stack:** Swift 5.9+, SwiftUI, SwiftData, AppIntents (iOS 16+), XCTest + Swift Testing (`@Test`).

**Spec:** `docs/superpowers/specs/2026-04-19-notebook-settings-pipeline-design.md`

---

## File Structure

**Create:**
- `fonos-ios/FonosApp/Models/NotebookTemplate.swift` — seed-prompt constants used by both backfill and the New Notebook sheet
- `fonos-ios/FonosApp/Services/NotebookPipeline.swift` — pure `NoteContainer → (sttLanguage, llmConfig?)` resolver + `NotebookLLMConfig` value type
- `fonos-ios/FonosApp/Services/SharedNotebookCatalog.swift` — App Group JSON read/write helper
- `fonos-ios/FonosApp/Models/SupportedLocale.swift` — curated locale list shown in pickers
- `fonos-ios/FonosIntents/FonosAppShortcuts.swift` — `AppShortcutsProvider` registration
- `fonos-ios/FonosAppTests/NotebookPipelineTests.swift`
- `fonos-ios/FonosAppTests/SharedNotebookCatalogTests.swift`
- `fonos-ios/FonosAppTests/NoteINV14BackfillTests.swift` — backfill behavior
- `fonos-ios/FonosAppTests/NoteINV15LLMNoteTests.swift` — LLMService.processNote

**Modify:**
- `fonos-ios/FonosApp/Models/NoteContainer.swift` — add 5 fields, mark 2 deprecated
- `fonos-ios/FonosApp/Services/NoteService.swift` — add `notebook(id:)`, App Group writes, backfill on init, AppShortcut refresh hook
- `fonos-ios/FonosApp/Services/LLMService.swift` — add `processNote(text:config:)`
- `fonos-ios/FonosApp/Services/NoteViewModel.swift` — rewrite `recordAndStore` via `NotebookPipeline`, delete `LLMServiceNoteAdapter`
- `fonos-ios/FonosApp/Views/NotebookSettingsView.swift` — restructure to numbered pipeline sections + chip
- `fonos-ios/FonosApp/Views/NotebookDetailView.swift` — pass notebook into `EntryRow`, gate raw display
- `fonos-ios/FonosApp/Views/NotesView.swift` — template chips in `NewNotebookSheet`
- `fonos-ios/FonosApp/Views/RecordNoteSheet.swift:107-110` — drop `mode` argument
- `fonos-ios/FonosIntents/RecordNoteIntent.swift` — add `DynamicOptionsProvider` reading from `SharedNotebookCatalog`
- `fonos-ios/FonosAppTests/NoteINV08NotebookConfigTests.swift` — extend for new fields
- `fonos-ios/FonosAppTests/NoteINV06RecordFlowTests.swift` — extend for sttLanguage + LLM gating
- `fonos-ios/FonosAppTests/NoteINV09IntentTests.swift` — extend for AppShortcuts
- `fonos-ios/FonosApp.xcodeproj/project.pbxproj` — register new files and the `FonosIntents` App Group entitlement

---

## Test Run Convention

All tests run via:

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/<TestClassName>
```

If `iPhone 16` is unavailable, fall back to `iPhone 15` or list with `xcrun simctl list devices`.

---

## Task 1: Add NotebookTemplate seed constants

**Files:**
- Create: `fonos-ios/FonosApp/Models/NotebookTemplate.swift`
- Test: covered indirectly by Task 2 backfill tests + Task 13 UI tests

- [ ] **Step 1: Create NotebookTemplate.swift**

```swift
import Foundation

/// Seed prompts shown when creating a new notebook and used to back-fill old
/// `processingMode` values into `systemPrompt` during v0.2.0 migration.
///
/// Templates are seeds, not types — once a notebook is created only its
/// `systemPrompt` matters. Editing a template here does not retroactively change
/// any existing notebook.
enum NotebookTemplate: String, CaseIterable, Identifiable {
    case raw
    case polish
    case meetingNotes
    case translate
    case blank

    var id: String { rawValue }

    /// Display label for the New Notebook sheet chip row.
    var displayName: String {
        switch self {
        case .raw:           return "Raw"
        case .polish:        return "Polish"
        case .meetingNotes:  return "Meeting Notes"
        case .translate:     return "Translate"
        case .blank:         return "Blank"
        }
    }

    /// SF Symbol name for the chip icon.
    var symbolName: String {
        switch self {
        case .raw:           return "waveform"
        case .polish:        return "sparkles"
        case .meetingNotes:  return "list.bullet.rectangle"
        case .translate:     return "globe"
        case .blank:         return "doc"
        }
    }

    /// Initial value for `NoteContainer.systemPrompt` when this template is picked.
    var systemPromptSeed: String {
        switch self {
        case .raw, .blank:
            return ""
        case .polish:
            return "Clean up filler words and disfluencies. Preserve the speaker's original meaning and tone."
        case .meetingNotes:
            return "Summarize as bullet-point meeting minutes with action items grouped at the bottom."
        case .translate:
            return "Translate the text accurately to the target language. Preserve tone."
        }
    }
}
```

- [ ] **Step 2: Add file to Xcode target**

Open `fonos-ios/FonosApp.xcodeproj` in Xcode → drag `NotebookTemplate.swift` into the `Models` group → check the `FonosApp` target. Verify build still passes:

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```

- [ ] **Step 3: Commit**

```bash
git add fonos-ios/FonosApp/Models/NotebookTemplate.swift fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): add NotebookTemplate seed prompts"
```

---

## Task 2: Add new fields to NoteContainer (deprecate old ones)

**Files:**
- Modify: `fonos-ios/FonosApp/Models/NoteContainer.swift`
- Test: `fonos-ios/FonosAppTests/NoteINV08NotebookConfigTests.swift` (extend)

- [ ] **Step 1: Write failing test for new fields**

Append to `NoteINV08NotebookConfigTests.swift` inside the `struct NoteINV08NotebookConfigTests {` body:

```swift
// MARK: - v2 fields

@Test("New v2 fields default to expected values")
func newFieldDefaults() throws {
    let modelContainer = try makeConfigTestContainer()
    let service = NoteService(modelContainer: modelContainer)
    let nb = service.createNotebook(title: "Defaults")
    #expect(nb.systemPrompt == "")
    #expect(nb.sttLanguage == nil)
    #expect(nb.outputLanguage == nil)
    #expect(nb.showRawInline == false)
    #expect(nb.siriPhrase == nil)
}

@Test("v2 fields persist via updateNotebookConfigV2")
func v2FieldsPersist() throws {
    let modelContainer = try makeConfigTestContainer()
    let service = NoteService(modelContainer: modelContainer)
    let nb = service.createNotebook(title: "Persist")
    service.updateNotebookConfigV2(
        nb.id,
        systemPrompt: "Be terse.",
        sttLanguage: "zh-CN",
        outputLanguage: "en-US",
        sttModelOverride: nil,
        llmModelOverride: nil,
        showRawInline: true,
        siriPhrase: "Note to Persist"
    )
    let fetched = try modelContainer.mainContext
        .fetch(FetchDescriptor<NoteContainer>())
        .first(where: { $0.id == nb.id })
    #expect(fetched?.systemPrompt == "Be terse.")
    #expect(fetched?.sttLanguage == "zh-CN")
    #expect(fetched?.outputLanguage == "en-US")
    #expect(fetched?.showRawInline == true)
    #expect(fetched?.siriPhrase == "Note to Persist")
}
```

- [ ] **Step 2: Run test — expect FAIL (fields not declared)**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV08NotebookConfigTests/newFieldDefaults
```
Expected: BUILD FAILED — `value of type 'NoteContainer' has no member 'systemPrompt'`.

- [ ] **Step 3: Update NoteContainer.swift**

Replace the entire body of `final class NoteContainer { … }` with:

```swift
@Model
final class NoteContainer {
    var id: UUID
    var title: String
    var containerType: String

    // MARK: - v2 (active)

    var systemPrompt: String = ""
    var sttLanguage: String?
    var outputLanguage: String?
    var showRawInline: Bool = false
    var siriPhrase: String?

    // MARK: - Existing overrides (kept)

    var sttModelOverride: String?
    var llmModelOverride: String?

    // MARK: - Deprecated v1 (read-only after backfill, removed in v0.3.x)

    var processingMode: String
    var customPrompt: String?

    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        title: String = "",
        containerType: String = "notebook",
        systemPrompt: String = "",
        sttLanguage: String? = nil,
        outputLanguage: String? = nil,
        showRawInline: Bool = false,
        siriPhrase: String? = nil,
        sttModelOverride: String? = nil,
        llmModelOverride: String? = nil,
        processingMode: String = "raw",
        customPrompt: String? = nil,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.title = title
        self.containerType = containerType
        self.systemPrompt = systemPrompt
        self.sttLanguage = sttLanguage
        self.outputLanguage = outputLanguage
        self.showRawInline = showRawInline
        self.siriPhrase = siriPhrase
        self.sttModelOverride = sttModelOverride
        self.llmModelOverride = llmModelOverride
        self.processingMode = processingMode
        self.customPrompt = customPrompt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}
```

- [ ] **Step 4: Add updateNotebookConfigV2 to NoteService**

Append to `NoteService.swift` inside the `final class NoteService {` body, just below `updateNotebookConfig`:

```swift
/// v2 config update — writes the new fields. Old `updateNotebookConfig` is kept
/// for tests and any caller still using `processingMode`.
func updateNotebookConfigV2(
    _ id: UUID,
    systemPrompt: String? = nil,
    sttLanguage: String?? = nil,
    outputLanguage: String?? = nil,
    sttModelOverride: String?? = nil,
    llmModelOverride: String?? = nil,
    showRawInline: Bool? = nil,
    siriPhrase: String?? = nil
) {
    let context = modelContainer.mainContext
    let descriptor = FetchDescriptor<NoteContainer>(
        predicate: #Predicate { $0.id == id }
    )
    guard let nb = try? context.fetch(descriptor).first else { return }
    if let systemPrompt { nb.systemPrompt = systemPrompt }
    if let sttLanguage { nb.sttLanguage = sttLanguage }
    if let outputLanguage { nb.outputLanguage = outputLanguage }
    if let sttModelOverride { nb.sttModelOverride = sttModelOverride }
    if let llmModelOverride { nb.llmModelOverride = llmModelOverride }
    if let showRawInline { nb.showRawInline = showRawInline }
    if let siriPhrase { nb.siriPhrase = siriPhrase }
    nb.updatedAt = Date()
    try? context.save()
}
```

The double-optional `String??` lets callers distinguish "leave unchanged" (nil) from "clear" (`.some(nil)`).

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV08NotebookConfigTests
```
All tests pass, including the original v1 tests (we kept `processingMode` / `customPrompt`).

- [ ] **Step 6: Commit**

```bash
git add fonos-ios/FonosApp/Models/NoteContainer.swift \
        fonos-ios/FonosApp/Services/NoteService.swift \
        fonos-ios/FonosAppTests/NoteINV08NotebookConfigTests.swift
git commit -m "feat(notebook): add v2 config fields (systemPrompt, sttLanguage, outputLanguage, showRawInline, siriPhrase)"
```

---

## Task 3: Backfill v1 → v2 on first launch

**Files:**
- Modify: `fonos-ios/FonosApp/Services/NoteService.swift`
- Create: `fonos-ios/FonosAppTests/NoteINV14BackfillTests.swift`

- [ ] **Step 1: Write failing tests**

Create `fonos-ios/FonosAppTests/NoteINV14BackfillTests.swift`:

```swift
// NoteINV14: One-shot v1→v2 backfill of processingMode/customPrompt into systemPrompt.
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV14BackfillTests

import Testing
import SwiftData
import Foundation
@testable import FonosApp

@MainActor
private func makeContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

@MainActor
struct NoteINV14BackfillTests {

    private let flagKey = "notebookConfig.migrated.v2.test"

    private func resetFlag() {
        UserDefaults.standard.removeObject(forKey: flagKey)
    }

    @Test("backfill maps processingMode='polish' to Polish template seed")
    func backfillPolish() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.polish.systemPromptSeed)
    }

    @Test("backfill maps processingMode='light_polish' to Polish template seed")
    func backfillLightPolish() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "light_polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.polish.systemPromptSeed)
    }

    @Test("backfill maps processingMode='summarize' to Meeting Notes seed")
    func backfillSummarize() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "summarize")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.meetingNotes.systemPromptSeed)
    }

    @Test("backfill maps customPrompt verbatim when processingMode is unknown")
    func backfillCustomPrompt() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "weirdo", customPrompt: "Make haiku.")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "Make haiku.")
    }

    @Test("backfill leaves systemPrompt='' when processingMode='raw'")
    func backfillRaw() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "raw")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "")
    }

    @Test("backfill is idempotent — second run does not overwrite user edits")
    func backfillIdempotent() throws {
        resetFlag()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey)

        // User edits prompt
        nb.systemPrompt = "Edited by user."
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: flagKey) // no-op

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "Edited by user.")
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL (method missing)**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV14BackfillTests
```
Expected: BUILD FAILED — `type 'NoteService' has no member 'runBackfill'`.

- [ ] **Step 3: Add backfill to NoteService**

Append to `NoteService.swift` inside the class:

```swift
// MARK: - v1 → v2 Backfill

private static let backfillFlagKey = "notebookConfig.migrated.v2"

/// Runs once on first NoteService init for a given install. Translates
/// `processingMode` + `customPrompt` into `systemPrompt`. Idempotent — guarded
/// by a UserDefaults flag.
static func runBackfill(modelContainer: ModelContainer, flagKey: String = backfillFlagKey) {
    let defaults = UserDefaults.standard
    if defaults.bool(forKey: flagKey) { return }

    let context = modelContainer.mainContext
    guard let notebooks = try? context.fetch(FetchDescriptor<NoteContainer>()) else {
        defaults.set(true, forKey: flagKey)
        return
    }

    var changed = false
    for nb in notebooks where nb.systemPrompt.isEmpty {
        let seed = backfillPrompt(processingMode: nb.processingMode, customPrompt: nb.customPrompt)
        if !seed.isEmpty {
            nb.systemPrompt = seed
            nb.updatedAt = Date()
            changed = true
        } else if nb.processingMode == "raw" {
            // Mark migrated even though prompt stays empty — prevents re-evaluation.
            nb.updatedAt = nb.updatedAt
        }
    }
    if changed { try? context.save() }
    defaults.set(true, forKey: flagKey)
}

private static func backfillPrompt(processingMode: String, customPrompt: String?) -> String {
    switch processingMode {
    case "raw":
        return ""
    case "polish", "light_polish":
        return NotebookTemplate.polish.systemPromptSeed
    case "summarize":
        return NotebookTemplate.meetingNotes.systemPromptSeed
    default:
        return customPrompt ?? ""
    }
}
```

Then call it from the production initializer (`init(modelContainer:)`):

Replace the existing initializer body with:

```swift
init(modelContainer: ModelContainer) {
    self.modelContainer = modelContainer
    Self.runBackfill(modelContainer: modelContainer)
}
```

(The `convenience init()` already calls `init(modelContainer:)`, so it inherits the backfill.)

- [ ] **Step 4: Add file to Xcode target**

Drag `NoteINV14BackfillTests.swift` into Xcode under `FonosAppTests`. Check the test target.

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV14BackfillTests
```
All 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add fonos-ios/FonosApp/Services/NoteService.swift \
        fonos-ios/FonosAppTests/NoteINV14BackfillTests.swift \
        fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): backfill processingMode/customPrompt into systemPrompt"
```

---

## Task 4: NotebookPipeline pure resolver

**Files:**
- Create: `fonos-ios/FonosApp/Services/NotebookPipeline.swift`
- Create: `fonos-ios/FonosAppTests/NotebookPipelineTests.swift`

- [ ] **Step 1: Write failing tests**

Create `fonos-ios/FonosAppTests/NotebookPipelineTests.swift`:

```swift
// NotebookPipeline: NoteContainer → (sttLanguage, llmConfig?) translation.
// Verifier: auto · Level: unit (pure function, no I/O)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NotebookPipelineTests

import Testing
import Foundation
@testable import FonosApp

@MainActor
struct NotebookPipelineTests {

    @Test("empty systemPrompt → llm == nil (Raw)")
    func emptyPromptRaw() {
        let nb = NoteContainer(title: "x", systemPrompt: "")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm == nil)
    }

    @Test("whitespace-only systemPrompt → llm == nil (Raw)")
    func whitespacePromptRaw() {
        let nb = NoteContainer(title: "x", systemPrompt: "   \n\t  ")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm == nil)
    }

    @Test("non-empty prompt → llm with trimmed prompt")
    func nonEmptyPrompt() {
        let nb = NoteContainer(title: "x", systemPrompt: "  Polish.  ")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.systemPrompt == "Polish.")
    }

    @Test("outputLanguage falls back to sttLanguage when nil")
    func outputLangFallsBackToStt() {
        let nb = NoteContainer(
            title: "x",
            systemPrompt: "Polish.",
            sttLanguage: "zh-CN",
            outputLanguage: nil
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.outputLanguage == "zh-CN")
    }

    @Test("explicit outputLanguage overrides sttLanguage")
    func outputLangOverride() {
        let nb = NoteContainer(
            title: "x",
            systemPrompt: "Translate.",
            sttLanguage: "zh-CN",
            outputLanguage: "en-US"
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.outputLanguage == "en-US")
    }

    @Test("sttLanguage is forwarded directly")
    func sttLangForwarded() {
        let nb = NoteContainer(title: "x", systemPrompt: "", sttLanguage: "ja-JP")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.sttLanguage == "ja-JP")
    }

    @Test("llmModelOverride flows into llm config")
    func modelOverrideFlows() {
        let nb = NoteContainer(
            title: "x",
            systemPrompt: "Polish.",
            llmModelOverride: "gpt-4o-mini"
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.modelOverride == "gpt-4o-mini")
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NotebookPipelineTests
```
Expected: BUILD FAILED — `cannot find 'NotebookPipeline' in scope`.

- [ ] **Step 3: Implement NotebookPipeline**

Create `fonos-ios/FonosApp/Services/NotebookPipeline.swift`:

```swift
import Foundation

/// Pure translation from a NoteContainer into the parameters its STT + LLM
/// invocation actually need. Single source of truth for both the runtime
/// pipeline (NoteViewModel) and the Settings UI summary chip.
enum NotebookPipeline {

    struct Resolved: Equatable {
        let sttLanguage: String?
        let sttModelOverride: String?
        let llm: NotebookLLMConfig?
    }

    static func resolve(_ n: NoteContainer) -> Resolved {
        let trimmed = n.systemPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let llm: NotebookLLMConfig? = trimmed.isEmpty ? nil : NotebookLLMConfig(
            systemPrompt: trimmed,
            outputLanguage: n.outputLanguage ?? n.sttLanguage,
            modelOverride: n.llmModelOverride
        )
        return Resolved(
            sttLanguage: n.sttLanguage,
            sttModelOverride: n.sttModelOverride,
            llm: llm
        )
    }
}

struct NotebookLLMConfig: Equatable, Sendable {
    let systemPrompt: String
    let outputLanguage: String?
    let modelOverride: String?
}
```

- [ ] **Step 4: Add files to Xcode targets**

Drag `NotebookPipeline.swift` into `FonosApp/Services` (target: FonosApp).
Drag `NotebookPipelineTests.swift` into `FonosAppTests` (target: FonosAppTests).

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NotebookPipelineTests
```
All 7 tests pass.

- [ ] **Step 6: Commit**

```bash
git add fonos-ios/FonosApp/Services/NotebookPipeline.swift \
        fonos-ios/FonosAppTests/NotebookPipelineTests.swift \
        fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): NotebookPipeline pure resolver + NotebookLLMConfig"
```

---

## Task 5: LLMService.processNote with output-language injection

**Files:**
- Modify: `fonos-ios/FonosApp/Services/LLMService.swift`
- Create: `fonos-ios/FonosAppTests/NoteINV15LLMNoteTests.swift`

- [ ] **Step 1: Write failing tests**

Create `fonos-ios/FonosAppTests/NoteINV15LLMNoteTests.swift`:

```swift
// NoteINV15: LLMService.processNote composes the system prompt by prepending
// "Always respond in {language}." when an outputLanguage is provided.
//
// Verifier: auto · Level: unit (URLProtocol mock)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV15LLMNoteTests

import Testing
import Foundation
@testable import FonosApp

// MARK: - Mock URLProtocol that captures the request body

final class CapturingURLProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var lastBody: Data?
    nonisolated(unsafe) static var stubResponse: Data = Data(
        #"{"choices":[{"message":{"content":"OK"}}]}"#.utf8
    )

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.lastBody = request.httpBody ?? request.bodyStreamData()
        let resp = HTTPURLResponse(
            url: request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: nil
        )!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.stubResponse)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

extension URLRequest {
    func bodyStreamData() -> Data? {
        guard let stream = httpBodyStream else { return nil }
        stream.open(); defer { stream.close() }
        var data = Data()
        let buf = UnsafeMutablePointer<UInt8>.allocate(capacity: 4096)
        defer { buf.deallocate() }
        while stream.hasBytesAvailable {
            let n = stream.read(buf, maxLength: 4096)
            if n <= 0 { break }
            data.append(buf, count: n)
        }
        return data
    }
}

private func makeService() -> LLMService {
    let cfg = URLSessionConfiguration.ephemeral
    cfg.protocolClasses = [CapturingURLProtocol.self]
    let session = URLSession(configuration: cfg)
    return LLMService(session: session, apiKey: "test", modelID: "gpt-4o", baseURL: "https://example.test")
}

@MainActor
struct NoteINV15LLMNoteTests {

    @Test("processNote prepends 'Always respond in {lang}.' when outputLanguage is set")
    func prependsLanguage() async throws {
        CapturingURLProtocol.lastBody = nil
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish the text.",
            outputLanguage: "zh-CN",
            modelOverride: nil
        )
        _ = try await svc.processNote(text: "hello", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        let messages = try #require(json["messages"] as? [[String: String]])
        let system = try #require(messages.first(where: { $0["role"] == "system" })?["content"])
        #expect(system.hasPrefix("Always respond in zh-CN."))
        #expect(system.contains("Polish the text."))
    }

    @Test("processNote omits language directive when outputLanguage is nil")
    func noLanguageNoDirective() async throws {
        CapturingURLProtocol.lastBody = nil
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish the text.",
            outputLanguage: nil,
            modelOverride: nil
        )
        _ = try await svc.processNote(text: "hello", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        let messages = try #require(json["messages"] as? [[String: String]])
        let system = try #require(messages.first(where: { $0["role"] == "system" })?["content"])
        #expect(system == "Polish the text.")
        #expect(!system.contains("Always respond"))
    }

    @Test("processNote uses modelOverride when provided")
    func usesModelOverride() async throws {
        CapturingURLProtocol.lastBody = nil
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish.",
            outputLanguage: nil,
            modelOverride: "gpt-4o-mini"
        )
        _ = try await svc.processNote(text: "hi", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect((json["model"] as? String) == "gpt-4o-mini")
    }

    @Test("processNote returns the LLM content")
    func returnsContent() async throws {
        CapturingURLProtocol.stubResponse = Data(
            #"{"choices":[{"message":{"content":"清理后的文本。"}}]}"#.utf8
        )
        let svc = makeService()
        let cfg = NotebookLLMConfig(systemPrompt: "Polish.", outputLanguage: "zh-CN", modelOverride: nil)
        let out = try await svc.processNote(text: "啊那个我想说...", config: cfg)
        #expect(out == "清理后的文本。")
    }
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV15LLMNoteTests
```
Expected: BUILD FAILED — `value of type 'LLMService' has no member 'processNote'`.

- [ ] **Step 3: Add processNote to LLMService**

Append to `LLMService.swift` (extension at the bottom of the file):

```swift
// MARK: - Note Pipeline

extension LLMService {
    /// Note-pipeline entry point. Bypasses the `Mode` enum (which is for Dictation)
    /// and uses the per-notebook NotebookLLMConfig directly.
    ///
    /// Output-language injection: when `config.outputLanguage` is non-nil, prepends
    /// `Always respond in {lang}.` followed by a blank line to the system prompt.
    /// Prefix placement gives the strongest steering for chat models.
    func processNote(text: String, config: NotebookLLMConfig) async throws -> String {
        let composedSystem = Self.composeSystemPrompt(
            user: config.systemPrompt, outputLanguage: config.outputLanguage
        )
        let modelToUse = config.modelOverride ?? modelID

        var requestBody: [String: Any] = [
            "model": modelToUse,
            "messages": [
                ["role": "system", "content": composedSystem],
                ["role": "user", "content": text]
            ],
            "max_completion_tokens": 1024
        ]
        // Match the existing pattern: omit temperature for o-series reasoning models.
        if !modelToUse.hasPrefix("o") {
            requestBody["temperature"] = 0.3
        }

        guard let url = URL(string: "\(baseURL)/v1/chat/completions") else {
            throw LLMError.networkUnavailable
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: requestBody)

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch let urlError as URLError {
            switch urlError.code {
            case .timedOut: throw LLMError.timeout
            case .notConnectedToInternet, .networkConnectionLost: throw LLMError.networkUnavailable
            default: throw LLMError.networkUnavailable
            }
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw LLMError.parseError
        }
        switch httpResponse.statusCode {
        case 200: break
        case 401: throw LLMError.authenticationFailed
        default: throw LLMError.serverError(statusCode: httpResponse.statusCode)
        }

        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choices = json["choices"] as? [[String: Any]],
              let first = choices.first,
              let msg = first["message"] as? [String: Any],
              let content = msg["content"] as? String else {
            throw LLMError.parseError
        }
        return content
    }

    static func composeSystemPrompt(user: String, outputLanguage: String?) -> String {
        guard let lang = outputLanguage, !lang.isEmpty else { return user }
        return "Always respond in \(lang).\n\n\(user)"
    }
}
```

- [ ] **Step 4: Add test file to Xcode target**

Drag `NoteINV15LLMNoteTests.swift` into `FonosAppTests` group, target: FonosAppTests.

- [ ] **Step 5: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV15LLMNoteTests
```
All 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add fonos-ios/FonosApp/Services/LLMService.swift \
        fonos-ios/FonosAppTests/NoteINV15LLMNoteTests.swift \
        fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): LLMService.processNote with output-language injection"
```

---

## Task 6: Wire NoteViewModel through NotebookPipeline

**Files:**
- Modify: `fonos-ios/FonosApp/Services/NoteService.swift` — add `notebook(id:)` lookup
- Modify: `fonos-ios/FonosApp/Services/NoteViewModel.swift` — rewrite recordAndStore
- Modify: `fonos-ios/FonosApp/Views/RecordNoteSheet.swift:107-110` — drop `mode` arg
- Modify: `fonos-ios/FonosAppTests/NoteINV06RecordFlowTests.swift` — extend

- [ ] **Step 1: Write failing tests**

Append to `NoteINV06RecordFlowTests.swift`:

```swift
// MARK: - v2 pipeline

private final class CapturingSTT: STTProvider, @unchecked Sendable {
    var lastLanguage: String?
    var stub: String = "raw transcript"
    func transcribe(audioData: Data, language: String?) async throws -> String {
        lastLanguage = language
        return stub
    }
}

private final class CapturingLLM: NoteLLMProvider, @unchecked Sendable {
    var calls: [(String, String?)] = []  // (text, prompt)
    var stub: String = "processed"
    func process(text: String, prompt: String?) async throws -> String {
        calls.append((text, prompt))
        return stub
    }
}

@Test("recordAndStore forwards notebook.sttLanguage to STT")
@MainActor
func sttLanguageForwarded() async throws {
    let mc = try makeRecordFlowContainer()
    let service = NoteService(modelContainer: mc)
    let nb = service.createNotebook(title: "ZH")
    service.updateNotebookConfigV2(nb.id, sttLanguage: .some("zh-CN"))

    let stt = CapturingSTT()
    let vm = NoteViewModel(noteService: service, sttProvider: stt, llmProvider: nil)
    await vm.recordAndStore(to: nb.id, audioData: Data())

    #expect(stt.lastLanguage == "zh-CN")
}

@Test("recordAndStore skips LLM when systemPrompt is empty")
@MainActor
func emptyPromptSkipsLLM() async throws {
    let mc = try makeRecordFlowContainer()
    let service = NoteService(modelContainer: mc)
    let nb = service.createNotebook(title: "Raw")
    // systemPrompt defaults to "" → Raw

    let llm = CapturingLLM()
    let vm = NoteViewModel(noteService: service, sttProvider: CapturingSTT(), llmProvider: llm)
    await vm.recordAndStore(to: nb.id, audioData: Data())

    #expect(llm.calls.isEmpty)
}

@Test("recordAndStore invokes LLM when systemPrompt is non-empty")
@MainActor
func nonEmptyPromptInvokesLLM() async throws {
    let mc = try makeRecordFlowContainer()
    let service = NoteService(modelContainer: mc)
    let nb = service.createNotebook(title: "Polish")
    service.updateNotebookConfigV2(nb.id, systemPrompt: "Polish.")

    let llm = CapturingLLM()
    let vm = NoteViewModel(noteService: service, sttProvider: CapturingSTT(), llmProvider: llm)
    await vm.recordAndStore(to: nb.id, audioData: Data())

    #expect(llm.calls.count == 1)
}
```

If `makeRecordFlowContainer()` does not exist in this file, add at the top:

```swift
@MainActor
private func makeRecordFlowContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV06RecordFlowTests
```
Expected: tests compile but fail (sttLanguage not forwarded, LLM still hardcoded `.polish`).

- [ ] **Step 3: Add notebook(id:) to NoteService**

Append to `NoteService.swift`:

```swift
// MARK: - Lookup

func notebook(id: UUID) -> NoteContainer? {
    let context = modelContainer.mainContext
    let descriptor = FetchDescriptor<NoteContainer>(
        predicate: #Predicate { $0.id == id }
    )
    return try? context.fetch(descriptor).first
}
```

- [ ] **Step 4: Update NoteLLMProvider protocol and add LLMService adapter for processNote**

Replace the `NoteLLMProvider` protocol declaration in `NoteViewModel.swift` with:

```swift
protocol NoteLLMProvider: AnyObject {
    /// Process raw transcript using the notebook's LLM config.
    /// Legacy (text, prompt) signature kept as default-impl convenience.
    func process(text: String, prompt: String?) async throws -> String
}

extension NoteLLMProvider {
    /// Default adapter from NotebookLLMConfig → legacy (text, prompt).
    /// Implementations may override to use the full config.
    func process(text: String, config: NotebookLLMConfig) async throws -> String {
        let prompt = "Always respond in \(config.outputLanguage ?? "").\n\n\(config.systemPrompt)"
        return try await process(text: text, prompt: prompt)
    }
}
```

Replace the existing `LLMServiceNoteAdapter` (at the bottom of `NoteViewModel.swift`) with:

```swift
private final class LLMServiceNoteAdapter: NoteLLMProvider {
    private let service: LLMService
    init(service: LLMService) { self.service = service }

    /// Legacy method — not used in the v2 path, but kept for protocol conformance.
    func process(text: String, prompt: String?) async throws -> String {
        let cfg = NotebookLLMConfig(systemPrompt: prompt ?? "", outputLanguage: nil, modelOverride: nil)
        return try await service.processNote(text: text, config: cfg)
    }

    /// v2 path — direct delegation to LLMService.processNote.
    func process(text: String, config: NotebookLLMConfig) async throws -> String {
        try await service.processNote(text: text, config: config)
    }
}
```

- [ ] **Step 5: Rewrite recordAndStore**

Replace the existing `func recordAndStore(...)` method body in `NoteViewModel.swift` with:

```swift
func recordAndStore(to containerId: UUID, audioData: Data) async {
    let stt = resolvedSTT
    let llm = resolvedLLM

    guard let notebook = noteService.notebook(id: containerId) else {
        recordingState = .error(message: "Notebook not found.")
        return
    }
    let resolved = NotebookPipeline.resolve(notebook)

    do {
        let rawText = try await stt.transcribe(
            audioData: audioData,
            language: resolved.sttLanguage
        )

        var processedText: String? = nil
        if let llmConfig = resolved.llm, let llmProvider = llm {
            do {
                processedText = try await llmProvider.process(text: rawText, config: llmConfig)
            } catch {
                noteVMLog.warning("LLM processing failed, using raw: \(error.localizedDescription)")
            }
        }

        noteService.addEntry(
            to: containerId,
            rawText: rawText,
            processedText: processedText,
            mode: resolved.llm == nil ? "raw" : "llm",
            language: resolved.sttLanguage
        )
        recordingState = .done
    } catch {
        noteVMLog.error("STT failed: \(error.localizedDescription)")
        recordingState = .error(message: error.localizedDescription)
    }
}
```

Also update the legacy 4-arg overload `recordAndStore(to:mode:audioData:)` if any test still calls it — replace with a forwarding wrapper:

```swift
// Backwards-compat for older tests; ignores `mode`.
func recordAndStore(to containerId: UUID, mode: String, audioData: Data) async {
    await recordAndStore(to: containerId, audioData: audioData)
}
```

Update `stopRecording`:

```swift
func stopRecording(to containerId: UUID) {
    guard case .recording = recordingState else { return }
    stopLevelPolling()
    let wavData = audioCapture.stopCapture() ?? Data()
    recordingState = .processing
    Task { await recordAndStore(to: containerId, audioData: wavData) }
}
```

Keep the old `stopRecording(to:mode:)` as a forwarding wrapper for now:

```swift
func stopRecording(to containerId: UUID, mode: String) {
    stopRecording(to: containerId)
}
```

- [ ] **Step 6: Update RecordNoteSheet caller**

Edit `fonos-ios/FonosApp/Views/RecordNoteSheet.swift` lines 107-110:

```swift
Button {
    noteViewModel.stopRecording(to: notebook.id)
    stopTimer()
} label: { ... }
```

(Drop the `mode: notebook.processingMode` argument.)

- [ ] **Step 7: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV06RecordFlowTests
```

- [ ] **Step 8: Build full app to catch other callers of the old 2-arg stopRecording**

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```
Expected: succeeds because we kept the wrapper.

- [ ] **Step 9: Commit**

```bash
git add fonos-ios/FonosApp/Services/NoteService.swift \
        fonos-ios/FonosApp/Services/NoteViewModel.swift \
        fonos-ios/FonosApp/Views/RecordNoteSheet.swift \
        fonos-ios/FonosAppTests/NoteINV06RecordFlowTests.swift
git commit -m "feat(notebook): NoteViewModel pipeline wiring (sttLanguage + systemPrompt take effect)"
```

---

## Task 7: SupportedLocale curated picker list

**Files:**
- Create: `fonos-ios/FonosApp/Models/SupportedLocale.swift`
- Test: covered by Settings UI tests in Task 8

- [ ] **Step 1: Create SupportedLocale.swift**

```swift
import Foundation

/// Curated locale list shown in NotebookSettingsView language pickers.
/// Keeping this short avoids the 700-locale full list and matches the languages
/// users actually need.
struct SupportedLocale: Identifiable, Hashable {
    let id: String          // BCP-47 identifier, e.g. "zh-CN"
    let displayName: String

    static let all: [SupportedLocale] = [
        .init(id: "en-US", displayName: "English (US)"),
        .init(id: "en-GB", displayName: "English (UK)"),
        .init(id: "zh-CN", displayName: "中文 (简体)"),
        .init(id: "zh-TW", displayName: "中文 (繁體)"),
        .init(id: "ja-JP", displayName: "日本語"),
        .init(id: "ko-KR", displayName: "한국어"),
        .init(id: "fr-FR", displayName: "Français"),
        .init(id: "de-DE", displayName: "Deutsch"),
        .init(id: "es-ES", displayName: "Español"),
        .init(id: "es-MX", displayName: "Español (MX)"),
        .init(id: "pt-BR", displayName: "Português (BR)"),
        .init(id: "ru-RU", displayName: "Русский"),
        .init(id: "it-IT", displayName: "Italiano"),
        .init(id: "ar-SA", displayName: "العربية"),
        .init(id: "hi-IN", displayName: "हिन्दी"),
        .init(id: "vi-VN", displayName: "Tiếng Việt")
    ]

    static func displayName(for id: String?) -> String {
        guard let id else { return "Auto" }
        return all.first(where: { $0.id == id })?.displayName ?? id
    }
}
```

- [ ] **Step 2: Add to Xcode FonosApp target.**

- [ ] **Step 3: Commit**

```bash
git add fonos-ios/FonosApp/Models/SupportedLocale.swift fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): add SupportedLocale curated picker list"
```

---

## Task 8: Restructure NotebookSettingsView (pipeline-indexed layout)

**Files:**
- Modify: `fonos-ios/FonosApp/Views/NotebookSettingsView.swift`
- Manual visual verification only (snapshot tests deferred per spec)

- [ ] **Step 1: Replace NotebookSettingsView body**

Overwrite the existing `body` and supporting computed properties with:

```swift
// MARK: - Body

var body: some View {
    ZStack {
        bg.ignoresSafeArea()

        ScrollView {
            VStack(spacing: 0) {
                pipelineChip
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
                    .padding(.bottom, 4)

                List {
                    generalSection
                    sttSection
                    llmSection
                    displaySection
                    shortcutSection
                    dangerZoneSection
                }
                .listStyle(.insetGrouped)
                .scrollContentBackground(.hidden)
                .frame(minHeight: 720)  // List inside ScrollView needs an explicit height
            }
        }
    }
    .navigationTitle("Notebook Settings")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar {
        ToolbarItem(placement: .confirmationAction) {
            Button("Save") {
                saveChanges()
                dismiss()
            }
            .foregroundColor(amber)
        }
    }
    .confirmationDialog(
        "Delete \"\(notebook.title)\"?",
        isPresented: $showDeleteConfirmation,
        titleVisibility: .visible
    ) {
        Button("Delete", role: .destructive) {
            try? noteService.deleteNotebook(notebook.id)
            dismiss()
        }
        Button("Cancel", role: .cancel) {}
    } message: {
        Text("This will permanently delete the notebook and all its notes.")
    }
}

// MARK: - Pipeline chip

private var pipelineChip: some View {
    let resolved = NotebookPipeline.resolve(stagingNotebook())
    let stt = SupportedLocale.displayName(for: resolved.sttLanguage)
    let outLang = resolved.llm.flatMap { $0.outputLanguage }
    let out = SupportedLocale.displayName(for: outLang)
    let middle = resolved.llm == nil ? "Raw" : "AI"

    return HStack(spacing: 8) {
        Text(stt)
        Text("→").foregroundColor(amber.opacity(0.6))
        Text(middle)
        if resolved.llm != nil {
            Text("→").foregroundColor(amber.opacity(0.6))
            Text(out)
        }
    }
    .font(.system(size: 12, design: .monospaced))
    .foregroundColor(textPrimary.opacity(0.85))
    .padding(.horizontal, 14)
    .padding(.vertical, 10)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
        RoundedRectangle(cornerRadius: 10)
            .fill(amber.opacity(0.08))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(amber.opacity(0.2), lineWidth: 1))
    )
}

/// Build a transient NoteContainer reflecting the unsaved Picker/TextField state,
/// so the chip updates as the user edits.
private func stagingNotebook() -> NoteContainer {
    NoteContainer(
        title: name,
        systemPrompt: systemPrompt,
        sttLanguage: sttLanguage.isEmpty ? nil : sttLanguage,
        outputLanguage: outputLanguage.isEmpty ? nil : outputLanguage,
        showRawInline: showRawInline,
        siriPhrase: siriPhrase.isEmpty ? nil : siriPhrase,
        sttModelOverride: sttModelOverride.isEmpty ? nil : sttModelOverride,
        llmModelOverride: llmModelOverride.isEmpty ? nil : llmModelOverride
    )
}
```

- [ ] **Step 2: Replace section computed properties**

Replace the existing `generalSection`, `processingSection`, `modelOverridesSection`, `dangerZoneSection` blocks with:

```swift
// MARK: - Sections

private var generalSection: some View {
    Section {
        TextField("Notebook Name", text: $name)
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
    } header: { sectionHeader("General") }
}

private var sttSection: some View {
    Section {
        Picker("Language", selection: $sttLanguage) {
            Text("Auto").tag("")
            ForEach(SupportedLocale.all) { loc in
                Text(loc.displayName).tag(loc.id)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)

        Picker("STT Model", selection: $sttModelOverride) {
            Text("Default").tag("")
            ForEach(modelProfiles, id: \.id) { profile in
                Text(profile.displayName).tag(profile.id)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)
    } header: { numberedHeader(1, "Speech-to-Text") }
}

private var llmSection: some View {
    Section {
        VStack(alignment: .leading, spacing: 6) {
            Text("System Prompt")
                .font(.system(size: 13))
                .foregroundColor(textPrimary)
            TextEditor(text: $systemPrompt)
                .frame(minHeight: 120)
                .scrollContentBackground(.hidden)
                .background(Color.white.opacity(0.03))
                .cornerRadius(6)
                .foregroundColor(textPrimary)
                .font(.system(size: 14))
                .autocorrectionDisabled()
                .textInputAutocapitalization(.sentences)
        }
        .padding(.vertical, 4)
        .listRowBackground(cardBg)

        Picker("Output Language", selection: $outputLanguage) {
            Text("Same as STT").tag("")
            ForEach(SupportedLocale.all) { loc in
                Text(loc.displayName).tag(loc.id)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)

        Picker("LLM Model", selection: $llmModelOverride) {
            Text("Default").tag("")
            ForEach(modelProfiles, id: \.id) { profile in
                Text(profile.displayName).tag(profile.id)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)
    } header: { numberedHeader(2, "LLM Processing") }
    footer: {
        Text("Leave System Prompt empty to skip LLM processing (Raw mode).")
            .foregroundColor(textDim).font(.system(size: 12))
    }
}

private var displaySection: some View {
    Section {
        Toggle("Show raw transcript inline", isOn: $showRawInline)
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .tint(amber)
    } header: { numberedHeader(3, "Display") }
}

private var shortcutSection: some View {
    Section {
        HStack {
            Text("Siri Phrase").foregroundColor(textPrimary)
            Spacer()
            Text(siriPhrase.isEmpty ? "Record to \(name)" : siriPhrase)
                .foregroundColor(textDim)
                .font(.system(size: 13, design: .monospaced))
        }
        .listRowBackground(cardBg)

        Button {
            if let url = URL(string: "shortcuts://") {
                UIApplication.shared.open(url)
            }
        } label: {
            HStack {
                Spacer()
                Text("Open in Shortcuts")
                    .foregroundColor(amber)
                Spacer()
            }
        }
        .listRowBackground(cardBg)
    } header: { sectionHeader("Shortcut") }
}

private var dangerZoneSection: some View {
    Section {
        Button {
            showDeleteConfirmation = true
        } label: {
            HStack {
                Spacer()
                Text("Delete Notebook").foregroundColor(isQuickNote ? textDim : red)
                Spacer()
            }
        }
        .disabled(isQuickNote)
        .listRowBackground(cardBg)
    } header: { sectionHeader("Danger Zone") }
}

// MARK: - Header builders

private func sectionHeader(_ title: String) -> some View {
    Text(title.uppercased())
        .font(.system(size: 12, weight: .medium))
        .foregroundColor(textDim)
        .textCase(nil)
}

private func numberedHeader(_ n: Int, _ title: String) -> some View {
    HStack(spacing: 8) {
        Text("\(n)")
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .foregroundColor(amber)
            .frame(width: 18, height: 18)
            .background(Circle().fill(amber.opacity(0.15)))
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }
}
```

- [ ] **Step 3: Replace state and init**

Replace the `// MARK: - State` block + `init(notebook:noteService:)` with:

```swift
// MARK: - State

@State private var name: String
@State private var systemPrompt: String
@State private var sttLanguage: String         // "" = Auto
@State private var outputLanguage: String      // "" = Same as STT
@State private var sttModelOverride: String    // "" = Default
@State private var llmModelOverride: String    // "" = Default
@State private var showRawInline: Bool
@State private var siriPhrase: String

@State private var showDeleteConfirmation = false
@Environment(\.dismiss) private var dismiss

private var modelProfiles: [ModelProfile] {
    // Pull from AppConfig — read once on appear via UserDefaults.
    // For v1 keep this simple: read the same key DictationViewModel uses.
    AppConfigStore.load().modelProfiles
}

// MARK: - Init

init(notebook: NoteContainer, noteService: NoteService) {
    self.notebook = notebook
    self.noteService = noteService
    _name = State(initialValue: notebook.title)
    _systemPrompt = State(initialValue: notebook.systemPrompt)
    _sttLanguage = State(initialValue: notebook.sttLanguage ?? "")
    _outputLanguage = State(initialValue: notebook.outputLanguage ?? "")
    _sttModelOverride = State(initialValue: notebook.sttModelOverride ?? "")
    _llmModelOverride = State(initialValue: notebook.llmModelOverride ?? "")
    _showRawInline = State(initialValue: notebook.showRawInline)
    _siriPhrase = State(initialValue: notebook.siriPhrase ?? "")
}
```

If `AppConfigStore` does not exist, replace the `modelProfiles` computed prop with a hardcoded `[]` for now and surface a TODO comment — the Pickers will just show "Default". (This avoids a deep refactor of AppConfig persistence in this task.)

- [ ] **Step 4: Replace saveChanges()**

```swift
private func saveChanges() {
    if name != notebook.title {
        noteService.renameNotebook(notebook.id, to: name)
    }
    noteService.updateNotebookConfigV2(
        notebook.id,
        systemPrompt: systemPrompt,
        sttLanguage: .some(sttLanguage.isEmpty ? nil : sttLanguage),
        outputLanguage: .some(outputLanguage.isEmpty ? nil : outputLanguage),
        sttModelOverride: .some(sttModelOverride.isEmpty ? nil : sttModelOverride),
        llmModelOverride: .some(llmModelOverride.isEmpty ? nil : llmModelOverride),
        showRawInline: showRawInline,
        siriPhrase: .some(siriPhrase.isEmpty ? nil : siriPhrase)
    )
}
```

- [ ] **Step 5: Build and run in simulator**

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```
Then launch the simulator manually:

```bash
xcrun simctl launch booted com.fonos.ios
```

Manually verify in the simulator:
1. Open Notes tab → tap a notebook → tap gear icon
2. Pipeline chip shows: `Auto → Raw` for default notebook
3. Set Language to 中文 (zh-CN), set System Prompt to "Polish.", chip becomes `中文 (简体) → AI → 中文 (简体)`
4. Set Output Language to English (US), chip becomes `中文 (简体) → AI → English (US)`
5. Tap Save, reopen settings — values persist

- [ ] **Step 6: Commit**

```bash
git add fonos-ios/FonosApp/Views/NotebookSettingsView.swift
git commit -m "feat(notebook): pipeline-indexed Settings UI with summary chip"
```

---

## Task 9: EntryRow respects showRawInline

**Files:**
- Modify: `fonos-ios/FonosApp/Views/NotebookDetailView.swift`

- [ ] **Step 1: Pass notebook into EntryRow**

In `NotebookDetailView.swift`, change the `entriesList` call site:

```swift
ForEach(entries, id: \.id) { entry in
    EntryRow(entry: entry, showRawInline: notebook.showRawInline)
        .listRowBackground(Color(hex: "#1a1917"))
        .listRowSeparatorTint(Color(hex: "#fafaf9").opacity(0.08))
}
```

Update `EntryRow`:

```swift
private struct EntryRow: View {
    let entry: NoteEntry
    let showRawInline: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(entry.createdAt, style: .time)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                Spacer()
                modeBadge
            }

            Text(entry.processedText ?? entry.rawText)
                .font(.system(size: 15))
                .foregroundColor(Color(hex: "#fafaf9"))
                .lineLimit(4)
                .multilineTextAlignment(.leading)

            if showRawInline,
               let processed = entry.processedText,
               processed != entry.rawText {
                Text(entry.rawText)
                    .font(.system(size: 12))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.35))
                    .lineLimit(4)
            }
        }
        .padding(.vertical, 8)
        .contextMenu {
            if let processed = entry.processedText, processed != entry.rawText {
                Button {
                    UIPasteboard.general.string = entry.rawText
                } label: {
                    Label("Copy raw transcript", systemImage: "doc.on.doc")
                }
            }
        }
    }

    private var modeBadge: some View {
        Text(entry.mode)
            .font(.system(size: 9, weight: .semibold, design: .monospaced))
            .foregroundColor(Color(hex: "#fbbf24").opacity(0.8))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Capsule().fill(Color(hex: "#fbbf24").opacity(0.1)))
    }
}
```

- [ ] **Step 2: Build**

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```

- [ ] **Step 3: Manual verification**

In the simulator: create notebook with a non-empty prompt, record a quick test note, verify:
- Toggle OFF → only processed text shown
- Toggle ON → raw text appears below in dim color
- Long-press entry → context menu offers "Copy raw transcript" (only when processed differs)

- [ ] **Step 4: Commit**

```bash
git add fonos-ios/FonosApp/Views/NotebookDetailView.swift
git commit -m "feat(notebook): EntryRow gates raw transcript on showRawInline + adds copy-raw menu"
```

---

## Task 10: New Notebook template chips

**Files:**
- Modify: `fonos-ios/FonosApp/Views/NotesView.swift` (NewNotebookSheet)

- [ ] **Step 1: Update NewNotebookSheet**

Replace the existing `private struct NewNotebookSheet` with:

```swift
private struct NewNotebookSheet: View {
    @Binding var title: String
    let onCreate: (String, NotebookTemplate) -> Void
    let onCancel: () -> Void

    @State private var selectedTemplate: NotebookTemplate = .raw

    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917").ignoresSafeArea()
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Notebook Name")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .textCase(.uppercase)
                        TextField("Notebook name", text: $title)
                            .textFieldStyle(.roundedBorder)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Starting Template")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .textCase(.uppercase)
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 10) {
                                ForEach(NotebookTemplate.allCases) { tpl in
                                    templateChip(tpl)
                                }
                            }
                        }
                        Text(selectedTemplate.systemPromptSeed.isEmpty
                             ? "No LLM processing (raw transcripts only)."
                             : selectedTemplate.systemPromptSeed)
                            .font(.system(size: 12))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .padding(.top, 4)
                    }

                    Spacer()
                }
                .padding(20)
            }
            .navigationTitle("New Notebook")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onCancel() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { onCreate(title, selectedTemplate) }
                        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private func templateChip(_ tpl: NotebookTemplate) -> some View {
        let isSelected = tpl == selectedTemplate
        return Button {
            selectedTemplate = tpl
        } label: {
            HStack(spacing: 6) {
                Image(systemName: tpl.symbolName)
                    .font(.system(size: 12, weight: .medium))
                Text(tpl.displayName)
                    .font(.system(size: 13, weight: .medium))
            }
            .foregroundColor(isSelected ? Color(hex: "#1a1917") : Color(hex: "#fafaf9"))
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isSelected ? Color(hex: "#fbbf24") : Color.white.opacity(0.05))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(isSelected ? Color.clear : Color.white.opacity(0.1), lineWidth: 1)
                    )
            )
        }
        .buttonStyle(.plain)
    }
}
```

- [ ] **Step 2: Update the call site (in NotesView body)**

Find the `.sheet(isPresented: $showNewNotebookSheet, ...)` block and replace its `onCreate` closure:

```swift
NewNotebookSheet(
    title: $newNotebookTitle,
    onCreate: { title, template in
        let nb = noteService.createNotebook(title: title)
        if !template.systemPromptSeed.isEmpty {
            noteService.updateNotebookConfigV2(nb.id, systemPrompt: template.systemPromptSeed)
        }
        showNewNotebookSheet = false
        reloadNotebooks()
    },
    onCancel: { showNewNotebookSheet = false }
)
```

- [ ] **Step 3: Build & manual verify**

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```
In simulator: tap "+ New Notebook" → confirm 5 chips appear horizontally. Pick "Polish" → preview text shows the polish seed. Create → open notebook settings → System Prompt is pre-filled with the polish seed.

- [ ] **Step 4: Commit**

```bash
git add fonos-ios/FonosApp/Views/NotesView.swift
git commit -m "feat(notebook): template chips in New Notebook sheet"
```

---

## Task 11: SharedNotebookCatalog (App Group JSON)

**Files:**
- Create: `fonos-ios/FonosApp/Services/SharedNotebookCatalog.swift`
- Create: `fonos-ios/FonosAppTests/SharedNotebookCatalogTests.swift`
- Modify: `fonos-ios/FonosIntents/FonosIntents.entitlements` (create if missing) — add App Group

- [ ] **Step 1: Write failing tests**

Create `fonos-ios/FonosAppTests/SharedNotebookCatalogTests.swift`:

```swift
// SharedNotebookCatalog: read/write a JSON catalog of (id, title) pairs into a
// shared App Group container so the FonosIntents target can list notebooks.
//
// Verifier: auto · Level: unit (uses a temp directory in lieu of App Group)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/SharedNotebookCatalogTests

import Testing
import Foundation
@testable import FonosApp

@MainActor
struct SharedNotebookCatalogTests {

    private func tempURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("json")
    }

    @Test("write then read returns the same entries")
    func roundTrip() throws {
        let url = tempURL()
        let entries = [
            SharedNotebookCatalog.Entry(id: UUID().uuidString, title: "Work"),
            SharedNotebookCatalog.Entry(id: UUID().uuidString, title: "Personal")
        ]
        try SharedNotebookCatalog.write(entries, to: url)

        let read = SharedNotebookCatalog.read(from: url)
        #expect(read.count == 2)
        #expect(read.map(\.title).sorted() == ["Personal", "Work"])
    }

    @Test("read on missing file returns empty array")
    func readMissingReturnsEmpty() {
        let url = tempURL()
        #expect(SharedNotebookCatalog.read(from: url).isEmpty)
    }

    @Test("write replaces existing file content")
    func writeReplaces() throws {
        let url = tempURL()
        try SharedNotebookCatalog.write([
            SharedNotebookCatalog.Entry(id: "1", title: "A")
        ], to: url)
        try SharedNotebookCatalog.write([
            SharedNotebookCatalog.Entry(id: "2", title: "B")
        ], to: url)

        let read = SharedNotebookCatalog.read(from: url)
        #expect(read.count == 1)
        #expect(read[0].title == "B")
    }
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/SharedNotebookCatalogTests
```
Expected: BUILD FAILED — `cannot find 'SharedNotebookCatalog' in scope`.

- [ ] **Step 3: Implement SharedNotebookCatalog**

Create `fonos-ios/FonosApp/Services/SharedNotebookCatalog.swift`:

```swift
import Foundation
import os.log

private let log = Logger(subsystem: "com.fonos.ios", category: "SharedNotebookCatalog")

/// A small JSON catalog of (notebook id, title) pairs persisted in the shared
/// App Group container so the FonosIntents target can list notebooks for
/// AppShortcuts / DynamicOptionsProvider without booting the main app.
enum SharedNotebookCatalog {

    static let appGroupID = "group.com.fonos.ios"
    static let filename   = "notebooks.json"

    struct Entry: Codable, Equatable, Sendable {
        let id: String
        let title: String
    }

    /// Default URL inside the shared App Group container.
    /// Returns nil if the App Group is not configured for the running target.
    static var defaultURL: URL? {
        FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: appGroupID)?
            .appendingPathComponent(filename)
    }

    static func write(_ entries: [Entry], to url: URL? = defaultURL) throws {
        guard let url else {
            log.warning("App Group container unavailable; catalog not persisted.")
            return
        }
        let data = try JSONEncoder().encode(entries)
        try data.write(to: url, options: [.atomic])
    }

    static func read(from url: URL? = defaultURL) -> [Entry] {
        guard let url, let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Entry].self, from: data)) ?? []
    }
}
```

- [ ] **Step 4: Add files to Xcode**

- Drag `SharedNotebookCatalog.swift` into `FonosApp/Services` and **also** check **`FonosIntents`** target so the Intents extension can read it.
- Drag `SharedNotebookCatalogTests.swift` into `FonosAppTests`.

- [ ] **Step 5: Add App Group to FonosIntents target**

In Xcode → select the `FonosIntents` target → Signing & Capabilities → "+ Capability" → App Groups → enable `group.com.fonos.ios`. This will create `fonos-ios/FonosIntents/FonosIntents.entitlements` with:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.application-groups</key>
    <array>
        <string>group.com.fonos.ios</string>
    </array>
</dict>
</plist>
```

If creating manually, also reference it in build settings: `CODE_SIGN_ENTITLEMENTS = FonosIntents/FonosIntents.entitlements` for the FonosIntents target.

- [ ] **Step 6: Run tests — expect PASS**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/SharedNotebookCatalogTests
```

- [ ] **Step 7: Commit**

```bash
git add fonos-ios/FonosApp/Services/SharedNotebookCatalog.swift \
        fonos-ios/FonosAppTests/SharedNotebookCatalogTests.swift \
        fonos-ios/FonosIntents/FonosIntents.entitlements \
        fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): SharedNotebookCatalog + App Group entitlement for FonosIntents"
```

---

## Task 12: NoteService writes catalog on every CRUD

**Files:**
- Modify: `fonos-ios/FonosApp/Services/NoteService.swift`

- [ ] **Step 1: Add catalog-sync helper**

Append to `NoteService.swift`:

```swift
// MARK: - Catalog sync

/// Write the current notebook list to the shared App Group catalog so the
/// FonosIntents target (AppShortcuts / DynamicOptionsProvider) sees an
/// up-to-date list without booting the main app.
private func syncCatalog() {
    let entries = allNotebooks().map {
        SharedNotebookCatalog.Entry(id: $0.id.uuidString, title: $0.title)
    }
    try? SharedNotebookCatalog.write(entries)
}
```

- [ ] **Step 2: Hook into CRUD methods**

Add `syncCatalog()` as the **last line** of:
- `createNotebook(...)` — before the `return notebook`
- `renameNotebook(...)` — after `try? context.save()`
- `deleteNotebook(...)` — after `try? context.save()`
- `quickNoteNotebook()` — only when the new branch creates Quick Note (after `try? context.save()` in the create branch)

Also call it once at the end of `init(modelContainer:)` so a fresh install seeds the catalog:

```swift
init(modelContainer: ModelContainer) {
    self.modelContainer = modelContainer
    Self.runBackfill(modelContainer: modelContainer)
    syncCatalog()
}
```

- [ ] **Step 3: Build**

```bash
cd fonos-ios && xcodebuild build -scheme FonosApp -destination 'platform=iOS Simulator,name=iPhone 16' -quiet
```

- [ ] **Step 4: Manual verification on simulator**

Launch app, create a notebook named "Test123", then in Terminal:

```bash
xcrun simctl get_app_container booted com.fonos.ios groups | head -3
# Find the group.com.fonos.ios path, e.g.:
#   group.com.fonos.ios   /path/to/group.com.fonos.ios
```

Cat the catalog (replace `<path>` with the printed path):
```bash
cat <path>/notebooks.json
```
Expected: JSON array containing `{"id":"<uuid>","title":"Test123"}` and any other notebooks.

- [ ] **Step 5: Commit**

```bash
git add fonos-ios/FonosApp/Services/NoteService.swift
git commit -m "feat(notebook): NoteService syncs SharedNotebookCatalog on every CRUD"
```

---

## Task 13: AppShortcuts + DynamicOptionsProvider

**Files:**
- Create: `fonos-ios/FonosIntents/FonosAppShortcuts.swift`
- Modify: `fonos-ios/FonosIntents/RecordNoteIntent.swift`
- Modify: `fonos-ios/FonosApp/Services/NoteService.swift` — call `updateAppShortcutParameters`
- Modify: `fonos-ios/FonosAppTests/NoteINV09IntentTests.swift` — extend

- [ ] **Step 1: Write failing tests**

Append to `NoteINV09IntentTests.swift`:

```swift
@Test("FonosAppShortcuts registers RecordNoteIntent with at least 2 phrases")
@MainActor
func appShortcutsRegistration() {
    let shortcuts = FonosAppShortcuts.appShortcuts
    #expect(!shortcuts.isEmpty)
    let phraseCount = shortcuts.flatMap { $0.phrases }.count
    #expect(phraseCount >= 2)
}

@Test("NotebookOptionsProvider returns titles read from SharedNotebookCatalog")
@MainActor
func optionsProviderReadsCatalog() async throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("opts-test-\(UUID().uuidString).json")
    try SharedNotebookCatalog.write([
        SharedNotebookCatalog.Entry(id: "id1", title: "Work"),
        SharedNotebookCatalog.Entry(id: "id2", title: "Personal")
    ], to: url)

    let provider = NotebookOptionsProvider(catalogURL: url)
    let titles = try await provider.results()
    #expect(titles.sorted() == ["Personal", "Work"])
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV09IntentTests
```
Expected: BUILD FAILED — `cannot find 'FonosAppShortcuts' in scope`.

- [ ] **Step 3: Create FonosAppShortcuts**

Create `fonos-ios/FonosIntents/FonosAppShortcuts.swift`:

```swift
import AppIntents

/// AppShortcuts registration so users can say "Hey Siri, record a note in Fonos"
/// and assign per-notebook shortcuts to Back Tap / Action Button via Shortcuts.app.
struct FonosAppShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: RecordNoteIntent(),
            phrases: [
                "Record a note in \(.applicationName)",
                "Take a voice note in \(.applicationName)"
            ],
            shortTitle: "Record Note",
            systemImageName: "mic.circle"
        )
    }
}
```

- [ ] **Step 4: Add NotebookOptionsProvider to RecordNoteIntent**

Modify `fonos-ios/FonosIntents/RecordNoteIntent.swift`. Replace the file with:

```swift
import AppIntents
import Foundation
import UIKit

// MARK: - NotebookOptionsProvider

struct NotebookOptionsProvider: DynamicOptionsProvider {
    /// Override target for tests. Production uses SharedNotebookCatalog.defaultURL.
    let catalogURL: URL?

    init(catalogURL: URL? = SharedNotebookCatalog.defaultURL) {
        self.catalogURL = catalogURL
    }

    func results() async throws -> [String] {
        // For now we expose titles — the perform step looks them up by title.
        // Upgrading to AppEntity would let us return ids with display strings;
        // deferred to a follow-up.
        SharedNotebookCatalog.read(from: catalogURL).map(\.title)
    }
}

// MARK: - RecordNoteIntent

struct RecordNoteIntent: AppIntent {
    static let title: LocalizedStringResource = "Record a Note"
    static let description = IntentDescription(
        "Opens Fonos and starts recording a voice note.",
        categoryName: "Notes"
    )
    static let openAppWhenRun: Bool = true

    @Parameter(
        title: "Notebook",
        description: "Which notebook to record into.",
        default: nil,
        optionsProvider: NotebookOptionsProvider()
    )
    var notebookId: String?

    var title: LocalizedStringResource { Self.title }
    var openAppWhenRun: Bool { Self.openAppWhenRun }
    var intentDescription: String? { "Opens Fonos and starts recording a voice note." }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        var components = URLComponents(string: "fonos://note")!
        if let notebookId, !notebookId.isEmpty {
            // notebookId here is actually a title from DynamicOptionsProvider.
            // Resolve title → uuid via the catalog so the URL scheme stays UUID-based.
            let entries = SharedNotebookCatalog.read()
            if let entry = entries.first(where: { $0.title == notebookId }) {
                components.queryItems = [URLQueryItem(name: "notebook", value: entry.id)]
            } else {
                // Allow a literal UUID in case someone passes one programmatically.
                components.queryItems = [URLQueryItem(name: "notebook", value: notebookId)]
            }
        }
        if let url = components.url {
            await UIApplication.shared.open(url)
        }
        return .result(value: "Fonos note recording started")
    }
}
```

- [ ] **Step 5: Add files to Xcode**

- `FonosAppShortcuts.swift` → target: **FonosIntents** AND **FonosApp** (so `FonosAppShortcuts.updateAppShortcutParameters()` is callable from the main app).
- Verify `RecordNoteIntent.swift` is in FonosIntents target.

- [ ] **Step 6: Wire updateAppShortcutParameters into NoteService**

In `NoteService.swift`, modify `syncCatalog()`:

```swift
private func syncCatalog() {
    let entries = allNotebooks().map {
        SharedNotebookCatalog.Entry(id: $0.id.uuidString, title: $0.title)
    }
    try? SharedNotebookCatalog.write(entries)
    // Notify the system that AppShortcut parameter values changed so Siri/
    // Shortcuts.app refresh their suggestion list.
    if #available(iOS 16.4, *) {
        FonosAppShortcuts.updateAppShortcutParameters()
    }
}
```

Make sure `import AppIntents` is at the top of `NoteService.swift`.

- [ ] **Step 7: Run tests**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:FonosAppTests/NoteINV09IntentTests
```

- [ ] **Step 8: Manual smoke test (Siri)**

Launch the app on a real device or simulator with Siri available. Create a notebook called "Quick Test". Trigger Siri: **"Hey Siri, record a note in Fonos"** — Siri should ask "which notebook?" and "Quick Test" should appear in the suggested options.

- [ ] **Step 9: Commit**

```bash
git add fonos-ios/FonosIntents/FonosAppShortcuts.swift \
        fonos-ios/FonosIntents/RecordNoteIntent.swift \
        fonos-ios/FonosApp/Services/NoteService.swift \
        fonos-ios/FonosAppTests/NoteINV09IntentTests.swift \
        fonos-ios/FonosApp.xcodeproj/project.pbxproj
git commit -m "feat(notebook): AppShortcuts + per-notebook DynamicOptionsProvider"
```

---

## Task 14: End-to-end smoke (real recording in simulator)

**Files:** none — pure verification.

- [ ] **Step 1: Build clean**

```bash
cd fonos-ios && xcodebuild clean build \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -quiet
```

- [ ] **Step 2: Run full test suite**

```bash
cd fonos-ios && xcodebuild test \
  -scheme FonosApp \
  -destination 'platform=iOS Simulator,name=iPhone 16'
```
All `NoteINV*`, `NotebookPipelineTests`, `SharedNotebookCatalogTests`, `LLMServiceTests` pass.

- [ ] **Step 3: Manual end-to-end (the original bug — Chinese in, English out)**

In simulator:
1. Notes tab → New Notebook → name "中文测试" → pick **Polish** template → Create
2. Open settings → set Language to 中文 (zh-CN), set Output Language to 中文 (zh-CN), Save
3. Open notebook → tap mic → speak Chinese for ~5s → stop
4. Verify the saved entry is in **Chinese** (not translated to English)
5. Toggle "Show raw transcript inline" ON in settings → re-open notebook → confirm raw + processed both visible
6. Long-press the entry → "Copy raw transcript" appears → tap → confirm clipboard contains the raw STT output

- [ ] **Step 4: Manual end-to-end (Siri / Shortcuts)**

1. Open Settings.app → Siri & Search → enable Listen for "Hey Siri" if not already
2. Say "Hey Siri, take a voice note in Fonos" — Fonos opens with recording state ready
3. Open Settings.app → Accessibility → Touch → Back Tap → Double Tap → pick "Record a Note" shortcut
4. Background Fonos, double-tap the back of the phone — Fonos comes to foreground in recording state

- [ ] **Step 5: Final commit (only if any small fixes were needed)**

```bash
git add -A
git commit -m "fix(notebook): smoke-test fixes" || true
```

---

## Self-Review

**1. Spec coverage check:**

| Spec section | Implemented in |
|---|---|
| Data Model (5 new fields, 2 deprecated) | Task 2 |
| `NotebookPipeline.resolve` | Task 4 |
| `LLMService.processNote` + output language injection | Task 5 |
| `NoteViewModel` rewrite (no more hardcoded `.polish`) | Task 6 |
| Settings UI restructure (numbered sections, pipeline chip) | Task 8 |
| Curated locale picker | Task 7 |
| `EntryRow` `showRawInline` gate + long-press | Task 9 |
| New Notebook templates | Task 10 + Task 1 (constants) |
| App Group catalog | Task 11 + Task 12 |
| `AppShortcutsProvider` + `DynamicOptionsProvider` | Task 13 |
| `updateAppShortcutParameters` on CRUD | Task 13 |
| v1 → v2 backfill | Task 3 |
| Tests for backfill / pipeline / processNote / record flow / app shortcuts | Tasks 2, 3, 4, 5, 6, 13 |

**2. Placeholder scan:** No `TBD`/`TODO`/`fill in details`. The single soft point is the `AppConfigStore.load()` reference in Task 8 — guarded with a fallback to `[]`. Acceptable for v1; the Picker will degrade gracefully to "Default".

**3. Type consistency:**
- `NotebookLLMConfig` declared in Task 4, used identically in Tasks 5, 6.
- `NotebookPipeline.resolve` returns `Resolved` struct used by both Task 6 (runtime) and Task 8 (UI chip).
- `SharedNotebookCatalog.Entry`, `defaultURL`, `read(from:)`, `write(_:to:)` consistent across Tasks 11–13.
- `updateNotebookConfigV2` parameter list identical in Tasks 2, 8, 10.

No drift detected.

---

## Out of Scope (explicit)

- Snapshot tests for `NotebookSettingsView` — deferred per spec.
- Removing deprecated `processingMode` / `customPrompt` columns — deferred to v0.3.x.
- Upgrading `RecordNoteIntent` to use `AppEntity` instead of `DynamicOptionsProvider` — deferred.
- Per-notebook temperature / maxTokens — single global default in v1.

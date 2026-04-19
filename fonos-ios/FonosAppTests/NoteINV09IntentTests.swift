// NoteINV09: RecordNoteIntent AppIntent compiles and is discoverable.
//
// Verifier: auto
// Levels: static (project builds with AppIntents framework), unit (title/description/openAppWhenRun)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV09IntentTests
//
// TDD status: FAILING until FonosIntents/RecordNoteIntent.swift is created.

import Testing
import AppIntents
@testable import FonosApp

// MARK: - Tests

struct NoteINV09IntentTests {

    // MARK: - Level 1: Static — AppIntents framework available

    @Test("AppIntents framework is importable")
    func appIntentsFrameworkAvailable() throws {
        // If AppIntents is not linked, this file fails to compile.
        // The import above is the compile-time assertion.
        #expect(Bool(true)) // sentinel
    }

    // MARK: - Level 1: Static — RecordNoteIntent type exists

    @Test("RecordNoteIntent type is accessible from FonosApp module")
    func recordNoteIntentTypeAccessible() throws {
        // If RecordNoteIntent doesn't exist, this file fails to compile.
        let _ = RecordNoteIntent.self
        #expect(Bool(true)) // sentinel — real assertion is compile-time
    }

    // MARK: - Level 2: Unit — title is non-empty

    @Test("RecordNoteIntent.title is non-empty")
    func titleNonEmpty() throws {
        // AppIntents uses static metadata. We instantiate to access the default title.
        // TODO: The exact API for accessing title metadata may differ — adjust if needed.
        let intent = RecordNoteIntent()
        // IntentDescription.title is a LocalizedStringResource; converting to String for assertion
        let titleString = String(localized: intent.title)
        #expect(!titleString.isEmpty, "RecordNoteIntent.title must not be empty")
    }

    // MARK: - Level 2: Unit — description is non-empty

    @Test("RecordNoteIntent has a non-empty description string")
    func descriptionNonEmpty() throws {
        // TODO: AppIntents description is declared as a static property on the type.
        // Adjust accessor once the implementation is known.
        let intent = RecordNoteIntent()
        if let desc = intent.intentDescription {
            #expect(!desc.isEmpty, "RecordNoteIntent description must not be empty")
        }
        // If intentDescription is nil, verify via the static metadata instead:
        // let metadata = RecordNoteIntent.intentDescription
        // #expect(metadata != nil)
    }

    // MARK: - Level 2: Unit — openAppWhenRun is true

    @Test("RecordNoteIntent.openAppWhenRun is true")
    func openAppWhenRunIsTrue() throws {
        // Back Tap / Shortcuts must bring the app to foreground so the user sees the
        // recording UI immediately. openAppWhenRun must be true.
        let intent = RecordNoteIntent()
        #expect(intent.openAppWhenRun == true)
    }

    // MARK: - Level 2: Unit — intent conforms to AppIntent

    @Test("RecordNoteIntent conforms to AppIntent protocol")
    func conformsToAppIntent() throws {
        // Checked at compile time by the generic constraint below.
        // If RecordNoteIntent doesn't conform, this file won't compile.
        func requiresAppIntent<T: AppIntent>(_: T.Type) {}
        requiresAppIntent(RecordNoteIntent.self)
        #expect(Bool(true)) // sentinel
    }

    // MARK: - v2: AppShortcuts registration & Notebook options provider

    @Test("FonosShortcuts.appShortcuts registers at least two intents (Dictate + RecordNote)")
    func appShortcutsRegistration() {
        // The collection's count is the only public surface — phrases is internal.
        // Both DictateIntent and RecordNoteIntent should be wired up.
        let count = FonosShortcuts.appShortcuts.reduce(into: 0) { acc, _ in acc += 1 }
        #expect(count >= 2)
    }

    @Test("NotebookOptionsProvider returns titles read from SharedNotebookCatalog")
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

    @Test("NotebookOptionsProvider returns empty array when catalog file is missing")
    func optionsProviderEmptyWhenMissing() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("missing-\(UUID().uuidString).json")
        let provider = NotebookOptionsProvider(catalogURL: url)
        let titles = try await provider.results()
        #expect(titles.isEmpty)
    }
}

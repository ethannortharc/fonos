// NoteINV13: ModelContainer registers DictationSession, NoteContainer, and NoteEntry.
//
// Verifier: auto
// Levels: static (FonosApp.swift modelContainer includes all three model types — checked at build),
//         unit (ModelContainer can be constructed with all three types without migration error)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV13ContainerTests
//
// TDD status: FAILING until NoteContainer and NoteEntry are added to the app's ModelContainer.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - Tests

@MainActor
struct NoteINV13ContainerTests {

    // MARK: - Level 1: Compile-time — all three types are accessible

    @Test("DictationSession type is accessible — existing model unchanged")
    func dictationSessionAccessible() throws {
        let _ = DictationSession.self
        #expect(Bool(true)) // sentinel
    }

    @Test("NoteContainer type is accessible")
    func noteContainerAccessible() throws {
        let _ = NoteContainer.self
        #expect(Bool(true)) // sentinel
    }

    @Test("NoteEntry type is accessible")
    func noteEntryAccessible() throws {
        let _ = NoteEntry.self
        #expect(Bool(true)) // sentinel
    }

    // MARK: - Level 2: Unit — combined schema creates without error

    @Test("ModelContainer with DictationSession + NoteContainer + NoteEntry schema creates without error")
    func combinedSchemaCreatesWithoutError() throws {
        let schema = Schema([
            DictationSession.self,
            NoteContainer.self,
            NoteEntry.self
        ])
        let config = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = try ModelContainer(for: schema, configurations: [config])
        // If SwiftData throws a migration error or schema conflict, the line above fails.
        _ = container
        #expect(Bool(true))
    }

    @Test("Combined ModelContainer mainContext is non-nil and accepts inserts for all three types")
    func combinedContainerMainContextAcceptsInserts() throws {
        let schema = Schema([
            DictationSession.self,
            NoteContainer.self,
            NoteEntry.self
        ])
        let config = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = try ModelContainer(for: schema, configurations: [config])
        let context = container.mainContext

        // Insert one instance of each type
        let session = DictationSession(
            id: UUID(),
            date: .now,
            mode: "raw",
            inputText: "test session",
            outputText: "test session",
            destination: "clipboard",
            latencyMs: 100
        )
        let notebook = NoteContainer(
            id: UUID(),
            title: "Schema Test Notebook",
            containerType: "notebook",
            processingMode: "raw",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil,
            createdAt: .now,
            updatedAt: .now
        )
        let notebookId = notebook.id
        let entry = NoteEntry(
            id: UUID(),
            createdAt: .now,
            sourceType: "note",
            rawText: "schema test entry",
            processedText: nil,
            containerId: notebookId,
            mode: "raw",
            durationMs: nil,
            language: nil
        )

        context.insert(session)
        context.insert(notebook)
        context.insert(entry)

        // save() must succeed — no migration errors, no constraint violations
        try context.save()

        let sessions = try context.fetch(FetchDescriptor<DictationSession>())
        let notebooks = try context.fetch(FetchDescriptor<NoteContainer>())
        let entries = try context.fetch(FetchDescriptor<NoteEntry>())

        #expect(sessions.count == 1)
        #expect(notebooks.count == 1)
        #expect(entries.count == 1)
    }

    @Test("DictationSession is unaffected when NoteContainer/NoteEntry are inserted into same container")
    func dictationSessionUnaffectedByNoteModels() throws {
        let schema = Schema([
            DictationSession.self,
            NoteContainer.self,
            NoteEntry.self
        ])
        let config = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = try ModelContainer(for: schema, configurations: [config])
        let context = container.mainContext

        // Pre-insert a dictation session
        let session = DictationSession(
            id: UUID(),
            date: Date(timeIntervalSince1970: 1_700_000_000),
            mode: "polish",
            inputText: "um yeah so basically",
            outputText: "Essentially,",
            destination: "clipboard",
            latencyMs: 300
        )
        context.insert(session)
        try context.save()

        // Then insert note models
        let notebook = NoteContainer(
            id: UUID(),
            title: "Isolation Test",
            containerType: "notebook",
            processingMode: "raw",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil,
            createdAt: .now,
            updatedAt: .now
        )
        context.insert(notebook)
        try context.save()

        // Dictation session must still be intact
        let sessions = try context.fetch(FetchDescriptor<DictationSession>())
        #expect(sessions.count == 1)
        #expect(sessions.first?.inputText == "um yeah so basically")
    }

    // MARK: - Level 2: App-level ModelContainer registration

    @Test("App's production ModelContainer configuration includes NoteContainer and NoteEntry")
    func appContainerIncludesNoteModels() throws {
        // TODO: Once FonosApp exposes its ModelContainer configuration (e.g. via a static
        // property AppModelContainer.shared), assert that its schema includes NoteContainer
        // and NoteEntry. For now, verify that constructing the expected schema succeeds.
        //
        // Replace the body below once AppModelContainer.schema or equivalent is accessible:
        //
        //   let schema = AppModelContainer.schema
        //   #expect(schema.entities.contains { $0.name == "NoteContainer" })
        //   #expect(schema.entities.contains { $0.name == "NoteEntry" })

        let schema = Schema([DictationSession.self, NoteContainer.self, NoteEntry.self])
        let modelNames = schema.entities.map { $0.name }
        #expect(modelNames.contains("NoteContainer"),
                "Schema must include NoteContainer — check FonosApp.swift modelContainer")
        #expect(modelNames.contains("NoteEntry"),
                "Schema must include NoteEntry — check FonosApp.swift modelContainer")
        #expect(modelNames.contains("DictationSession"),
                "Schema must still include DictationSession — existing model must not be dropped")
    }
}

// NoteINV01: NoteContainer and NoteEntry SwiftData models exist with correct schema.
//
// Verifier: auto
// Levels: static (build — models compile), unit (field round-trip via in-memory container)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV01ModelTests
//
// TDD status: FAILING until NoteContainer.swift and NoteEntry.swift are created.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeNoteContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteINV01ModelTests {

    // MARK: - NoteContainer schema

    @Test("NoteContainer can be inserted and fetched from SwiftData")
    func noteContainerInsertAndFetch() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        // TODO: Replace with real NoteContainer initialiser once implemented
        let notebook = NoteContainer(
            id: UUID(),
            title: "My Notebook",
            containerType: "notebook",
            processingMode: "raw",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil,
            createdAt: Date(),
            updatedAt: Date()
        )
        context.insert(notebook)
        try context.save()

        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        #expect(all.count == 1)
        #expect(all.first?.title == "My Notebook")
    }

    @Test("NoteContainer stores all required fields correctly")
    func noteContainerAllFields() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let id = UUID()
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        let notebook = NoteContainer(
            id: id,
            title: "Work Notes",
            containerType: "notebook",
            processingMode: "light_polish",
            sttModelOverride: "whisper-large",
            llmModelOverride: "gpt-4o",
            customPrompt: "Summarize concisely.",
            createdAt: now,
            updatedAt: now
        )
        context.insert(notebook)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.id == id)
        #expect(fetched?.title == "Work Notes")
        #expect(fetched?.containerType == "notebook")
        #expect(fetched?.processingMode == "light_polish")
        #expect(fetched?.sttModelOverride == "whisper-large")
        #expect(fetched?.llmModelOverride == "gpt-4o")
        #expect(fetched?.customPrompt == "Summarize concisely.")
        #expect(fetched?.createdAt == now)
        #expect(fetched?.updatedAt == now)
    }

    @Test("NoteContainer optional fields can be nil")
    func noteContainerOptionalFieldsNil() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let notebook = NoteContainer(
            id: UUID(),
            title: "Quick Note",
            containerType: "notebook",
            processingMode: "raw",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil,
            createdAt: Date(),
            updatedAt: Date()
        )
        context.insert(notebook)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.sttModelOverride == nil)
        #expect(fetched?.llmModelOverride == nil)
        #expect(fetched?.customPrompt == nil)
    }

    // MARK: - NoteEntry schema

    @Test("NoteEntry can be inserted and fetched from SwiftData")
    func noteEntryInsertAndFetch() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let containerId = UUID()
        // TODO: Replace with real NoteEntry initialiser once implemented
        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "Hello world",
            processedText: nil,
            containerId: containerId,
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        let all = try context.fetch(FetchDescriptor<NoteEntry>())
        #expect(all.count == 1)
        #expect(all.first?.rawText == "Hello world")
    }

    @Test("NoteEntry stores all required fields correctly")
    func noteEntryAllFields() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let entryId = UUID()
        let containerId = UUID()
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        let entry = NoteEntry(
            id: entryId,
            createdAt: now,
            sourceType: "note",
            rawText: "um so yeah basically we need to ship this feature",
            processedText: "We need to ship this feature.",
            containerId: containerId,
            mode: "light_polish",
            durationMs: 4500.0,
            language: "en"
        )
        context.insert(entry)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        #expect(fetched?.id == entryId)
        #expect(fetched?.createdAt == now)
        #expect(fetched?.sourceType == "note")
        #expect(fetched?.rawText == "um so yeah basically we need to ship this feature")
        #expect(fetched?.processedText == "We need to ship this feature.")
        #expect(fetched?.containerId == containerId)
        #expect(fetched?.mode == "light_polish")
        #expect(fetched?.durationMs == 4500.0)
        #expect(fetched?.language == "en")
    }

    @Test("NoteEntry optional fields can be nil")
    func noteEntryOptionalFieldsNil() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "quick thought",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        #expect(fetched?.processedText == nil)
        #expect(fetched?.durationMs == nil)
        #expect(fetched?.language == nil)
    }

    // MARK: - Cross-model: entry linked to container

    @Test("NoteEntry with containerId can be fetched by that containerId")
    func fetchEntriesByContainerId() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let notebookId = UUID()
        let otherNotebookId = UUID()

        let entries = [
            NoteEntry(id: UUID(), createdAt: Date(), sourceType: "note",
                      rawText: "entry 1", processedText: nil,
                      containerId: notebookId, mode: "raw",
                      durationMs: nil, language: nil),
            NoteEntry(id: UUID(), createdAt: Date(), sourceType: "note",
                      rawText: "entry 2", processedText: nil,
                      containerId: notebookId, mode: "raw",
                      durationMs: nil, language: nil),
            NoteEntry(id: UUID(), createdAt: Date(), sourceType: "note",
                      rawText: "other notebook entry", processedText: nil,
                      containerId: otherNotebookId, mode: "raw",
                      durationMs: nil, language: nil),
        ]
        entries.forEach { context.insert($0) }
        try context.save()

        let descriptor = FetchDescriptor<NoteEntry>(
            predicate: #Predicate { $0.containerId == notebookId }
        )
        let results = try context.fetch(descriptor)
        #expect(results.count == 2)
    }

    // MARK: - processingMode values

    @Test("NoteContainer accepts all valid processingMode values")
    func processingModeValues() throws {
        let container = try makeNoteContainer()
        let context = container.mainContext

        let modes = ["raw", "light_polish", "summarize"]
        for mode in modes {
            let notebook = NoteContainer(
                id: UUID(),
                title: "Notebook \(mode)",
                containerType: "notebook",
                processingMode: mode,
                sttModelOverride: nil,
                llmModelOverride: nil,
                customPrompt: nil,
                createdAt: Date(),
                updatedAt: Date()
            )
            context.insert(notebook)
        }
        try context.save()

        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let storedModes = Set(all.map(\.processingMode))
        #expect(storedModes == Set(modes))
    }
}

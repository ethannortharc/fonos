// NoteINV02: NoteService provides full CRUD for NoteContainer and NoteEntry.
//
// Verifier: auto
// Level: unit (in-memory SwiftData ModelContainer)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV02ServiceTests
//
// TDD status: FAILING until NoteService.swift is created.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeNoteServiceContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteINV02ServiceTests {

    // MARK: - Notebook create

    @Test("NoteService.createNotebook returns a NoteContainer with correct title")
    func createNotebook() throws {
        let modelContainer = try makeNoteServiceContainer()
        // TODO: Inject modelContainer into NoteService once its init is defined
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "My First Notebook")
        #expect(notebook.title == "My First Notebook")
        #expect(notebook.containerType == "notebook")
        #expect(notebook.processingMode == "raw") // default
    }

    @Test("NoteService.createNotebook persists the notebook in SwiftData")
    func createNotebookPersists() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        _ = service.createNotebook(title: "Persisted Notebook")

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let found = all.first(where: { $0.title == "Persisted Notebook" })
        #expect(found != nil)
    }

    @Test("NoteService.createNotebook assigns a non-nil UUID id")
    func createNotebookHasUUID() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "UUID Test")
        // id must be a valid UUID (non-zero)
        #expect(notebook.id != UUID(uuidString: "00000000-0000-0000-0000-000000000000"))
    }

    // MARK: - Notebook rename

    @Test("NoteService.renameNotebook updates the title")
    func renameNotebook() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Old Title")
        service.renameNotebook(notebook.id, to: "New Title")

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.title == "New Title")
    }

    @Test("NoteService.renameNotebook updates updatedAt timestamp")
    func renameNotebookUpdatesTimestamp() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Timestamp Test")
        let originalUpdatedAt = notebook.updatedAt

        // Ensure some time passes
        Thread.sleep(forTimeInterval: 0.01)
        service.renameNotebook(notebook.id, to: "Updated Title")

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        // updatedAt should be >= original
        if let updatedAt = fetched?.updatedAt {
            #expect(updatedAt >= originalUpdatedAt)
        }
    }

    // MARK: - Notebook delete

    @Test("NoteService.deleteNotebook removes the notebook from storage")
    func deleteNotebook() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "To Be Deleted")
        try service.deleteNotebook(notebook.id)

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let found = all.first(where: { $0.id == notebook.id })
        #expect(found == nil)
    }

    @Test("NoteService.deleteNotebook leaves other notebooks intact")
    func deleteNotebookLeavesOthersIntact() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebookA = service.createNotebook(title: "Notebook A")
        let notebookB = service.createNotebook(title: "Notebook B")
        try service.deleteNotebook(notebookA.id)

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        #expect(all.count == 1)
        #expect(all.first?.id == notebookB.id)
    }

    // MARK: - Entry create

    @Test("NoteService.addEntry returns a NoteEntry with correct fields")
    func addEntry() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Test Notebook")
        let entry = service.addEntry(
            to: notebook.id,
            rawText: "um yeah so we need to ship this",
            processedText: "We need to ship this.",
            mode: "light_polish",
            durationMs: 3200.0,
            language: "en"
        )

        #expect(entry.rawText == "um yeah so we need to ship this")
        #expect(entry.processedText == "We need to ship this.")
        #expect(entry.containerId == notebook.id)
        #expect(entry.mode == "light_polish")
        #expect(entry.durationMs == 3200.0)
        #expect(entry.language == "en")
        #expect(entry.sourceType == "note")
    }

    @Test("NoteService.addEntry persists the entry in SwiftData")
    func addEntryPersists() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Persistence Test")
        _ = service.addEntry(
            to: notebook.id,
            rawText: "persisted entry",
            processedText: nil,
            mode: "raw",
            durationMs: nil,
            language: nil
        )

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteEntry>())
        #expect(all.count == 1)
        #expect(all.first?.rawText == "persisted entry")
    }

    // MARK: - Entry edit

    @Test("NoteService.updateEntry changes the rawText of an entry")
    func updateEntryText() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Edit Test")
        let entry = service.addEntry(
            to: notebook.id,
            rawText: "original text",
            processedText: nil,
            mode: "raw",
            durationMs: nil,
            language: nil
        )

        service.updateEntry(entry.id, text: "edited text")

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        #expect(fetched?.rawText == "edited text")
    }

    // MARK: - Entry delete

    @Test("NoteService.deleteEntry removes the entry from storage")
    func deleteEntry() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Delete Entry Test")
        let entry = service.addEntry(
            to: notebook.id,
            rawText: "to be deleted",
            processedText: nil,
            mode: "raw",
            durationMs: nil,
            language: nil
        )

        service.deleteEntry(entry.id)

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteEntry>())
        #expect(all.isEmpty)
    }

    @Test("NoteService.deleteEntry leaves other entries intact")
    func deleteEntryLeavesOthers() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Multi Entry")
        let entry1 = service.addEntry(to: notebook.id, rawText: "entry 1",
                                       processedText: nil, mode: "raw",
                                       durationMs: nil, language: nil)
        let entry2 = service.addEntry(to: notebook.id, rawText: "entry 2",
                                       processedText: nil, mode: "raw",
                                       durationMs: nil, language: nil)

        service.deleteEntry(entry1.id)

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteEntry>())
        #expect(all.count == 1)
        #expect(all.first?.id == entry2.id)
    }

    // MARK: - Entries for notebook

    @Test("NoteService.entriesForNotebook returns only entries for that notebook")
    func entriesForNotebook() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebookA = service.createNotebook(title: "Notebook A")
        let notebookB = service.createNotebook(title: "Notebook B")

        _ = service.addEntry(to: notebookA.id, rawText: "A1", processedText: nil,
                              mode: "raw", durationMs: nil, language: nil)
        _ = service.addEntry(to: notebookA.id, rawText: "A2", processedText: nil,
                              mode: "raw", durationMs: nil, language: nil)
        _ = service.addEntry(to: notebookB.id, rawText: "B1", processedText: nil,
                              mode: "raw", durationMs: nil, language: nil)

        let aEntries = service.entriesForNotebook(notebookA.id)
        #expect(aEntries.count == 2)

        let bEntries = service.entriesForNotebook(notebookB.id)
        #expect(bEntries.count == 1)
    }

    @Test("NoteService.entriesForNotebook returns entries sorted by createdAt descending")
    func entriesForNotebookSortedDescending() throws {
        let modelContainer = try makeNoteServiceContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Sorted Notebook")
        // Add entries with explicit timestamps via the model directly to control ordering
        let context = modelContainer.mainContext
        let base = Date(timeIntervalSince1970: 1_700_000_000)

        let entryOld = NoteEntry(
            id: UUID(), createdAt: base, sourceType: "note",
            rawText: "oldest", processedText: nil,
            containerId: notebook.id, mode: "raw",
            durationMs: nil, language: nil
        )
        let entryNew = NoteEntry(
            id: UUID(), createdAt: base.addingTimeInterval(3600), sourceType: "note",
            rawText: "newest", processedText: nil,
            containerId: notebook.id, mode: "raw",
            durationMs: nil, language: nil
        )
        context.insert(entryOld)
        context.insert(entryNew)
        try context.save()

        let entries = service.entriesForNotebook(notebook.id)
        #expect(entries.count == 2)
        // First entry must be the most recent
        #expect(entries[0].rawText == "newest")
        #expect(entries[1].rawText == "oldest")
    }
}

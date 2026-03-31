// NoteINV07: Quick Note default notebook — auto-created on first launch, cannot be deleted.
//
// Verifier: auto
// Level: unit (in-memory SwiftData ModelContainer)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV07QuickNoteTests
//
// TDD status: FAILING until NoteService.quickNoteNotebook() and deletion guard are implemented.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeQuickNoteContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteINV07QuickNoteTests {

    // MARK: - Quick Note creation

    @Test("NoteService.quickNoteNotebook returns a non-nil NoteContainer")
    func quickNoteNotebookNotNil() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        let quickNote = service.quickNoteNotebook()
        #expect(quickNote != nil)
    }

    @Test("NoteService.quickNoteNotebook has title 'Quick Note'")
    func quickNoteNotebookTitle() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        let quickNote = service.quickNoteNotebook()
        // TODO: Confirm exact title string with design — "Quick Note" is the spec default
        #expect(quickNote?.title == "Quick Note")
    }

    @Test("NoteService.quickNoteNotebook is idempotent — two calls return same notebook")
    func quickNoteNotebookIdempotent() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        let first = service.quickNoteNotebook()
        let second = service.quickNoteNotebook()
        #expect(first?.id == second?.id)

        // Only one Quick Note notebook should exist in storage
        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let quickNoteCount = all.filter { $0.title == "Quick Note" }.count
        #expect(quickNoteCount == 1)
    }

    @Test("NoteService.quickNoteNotebook persists across service instances sharing the same container")
    func quickNoteNotebookPersistsAcrossInstances() throws {
        let modelContainer = try makeQuickNoteContainer()

        let service1 = NoteService(modelContainer: modelContainer)
        let firstId = service1.quickNoteNotebook()?.id

        let service2 = NoteService(modelContainer: modelContainer)
        let secondId = service2.quickNoteNotebook()?.id

        #expect(firstId == secondId)
    }

    // MARK: - Quick Note delete protection

    @Test("Deleting Quick Note notebook throws an error")
    func deleteQuickNoteThrows() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        let quickNote = service.quickNoteNotebook()
        guard let id = quickNote?.id else {
            Issue.record("quickNoteNotebook() returned nil — cannot test delete protection")
            return
        }

        // TODO: Confirm the exact error type thrown — NoteServiceError.cannotDeleteQuickNote is assumed
        #expect(throws: NoteServiceError.self) {
            try service.deleteNotebook(id)
        }
    }

    @Test("Quick Note notebook remains in storage after failed delete attempt")
    func quickNoteRemainsAfterDeleteAttempt() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        let quickNote = service.quickNoteNotebook()
        guard let id = quickNote?.id else {
            Issue.record("quickNoteNotebook() returned nil")
            return
        }

        try? service.deleteNotebook(id) // ignore error — we're testing side effects

        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let stillExists = all.contains(where: { $0.id == id })
        #expect(stillExists)
    }

    @Test("Deleting a regular notebook does not throw")
    func deleteRegularNotebookDoesNotThrow() throws {
        let modelContainer = try makeQuickNoteContainer()
        let service = NoteService(modelContainer: modelContainer)

        _ = service.quickNoteNotebook() // ensure Quick Note exists
        let regularNotebook = service.createNotebook(title: "Regular Notebook")

        // This must not throw
        #expect(throws: Never.self) {
            try service.deleteNotebook(regularNotebook.id)
        }
    }
}

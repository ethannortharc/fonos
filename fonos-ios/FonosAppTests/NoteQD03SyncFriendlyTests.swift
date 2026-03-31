// NoteQD03: Data model uses UUID IDs and is sync-friendly.
// All IDs are UUID type; no sync tokens or CloudKit metadata in base models.
//
// Verifier: auto
// Level: unit (type introspection via Mirror + SwiftData fetch)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteQD03SyncFriendlyTests
//
// TDD status: FAILING until NoteContainer.swift and NoteEntry.swift are created.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeSyncTestContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteQD03SyncFriendlyTests {

    // MARK: - UUID types

    @Test("NoteContainer.id is of type UUID")
    func noteContainerIdIsUUID() throws {
        let modelContainer = try makeSyncTestContainer()
        let context = modelContainer.mainContext

        let notebook = NoteContainer(
            id: UUID(),
            title: "UUID Test Notebook",
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
        // If id is not UUID, this type check fails
        guard let id = fetched?.id else {
            Issue.record("NoteContainer.id is nil after fetch")
            return
        }
        // Verify it is UUID by checking Mirror label — at compile time the type is already UUID
        let mirror = Mirror(reflecting: id)
        let isUUID = mirror.subjectType == UUID.self
        #expect(isUUID,
                "NoteContainer.id must be UUID, got \(mirror.subjectType)")
    }

    @Test("NoteEntry.id is of type UUID")
    func noteEntryIdIsUUID() throws {
        let modelContainer = try makeSyncTestContainer()
        let context = modelContainer.mainContext

        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "uuid entry",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        guard let id = fetched?.id else {
            Issue.record("NoteEntry.id is nil after fetch")
            return
        }
        let mirror = Mirror(reflecting: id)
        let isUUID = mirror.subjectType == UUID.self
        #expect(isUUID,
                "NoteEntry.id must be UUID, got \(mirror.subjectType)")
    }

    @Test("NoteEntry.containerId is of type UUID")
    func noteEntryContainerIdIsUUID() throws {
        let modelContainer = try makeSyncTestContainer()
        let context = modelContainer.mainContext

        let notebookId = UUID()
        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "containerId type test",
            processedText: nil,
            containerId: notebookId,
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        guard let containerId = fetched?.containerId else {
            Issue.record("NoteEntry.containerId is nil after fetch")
            return
        }
        let mirror = Mirror(reflecting: containerId)
        let isUUID = mirror.subjectType == UUID.self
        #expect(isUUID)
    }

    @Test("NoteContainer.id round-trips through SwiftData without losing precision")
    func noteContainerIdRoundTrip() throws {
        let modelContainer = try makeSyncTestContainer()
        let context = modelContainer.mainContext

        let originalId = UUID()
        let notebook = NoteContainer(
            id: originalId,
            title: "Round-trip Test",
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
        #expect(fetched?.id == originalId)
    }

    @Test("NoteEntry.id round-trips through SwiftData without losing precision")
    func noteEntryIdRoundTrip() throws {
        let modelContainer = try makeSyncTestContainer()
        let context = modelContainer.mainContext

        let originalId = UUID()
        let entry = NoteEntry(
            id: originalId,
            createdAt: Date(),
            sourceType: "note",
            rawText: "id round-trip",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        #expect(fetched?.id == originalId)
    }

    // MARK: - No sync-specific fields

    @Test("NoteContainer has no CloudKit-specific or sync-token fields")
    func noteContainerHasNoSyncFields() throws {
        let notebook = NoteContainer(
            id: UUID(),
            title: "Sync Check",
            containerType: "notebook",
            processingMode: "raw",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil,
            createdAt: Date(),
            updatedAt: Date()
        )
        let mirror = Mirror(reflecting: notebook)
        let propertyNames = Set(mirror.children.compactMap { $0.label })

        // These are v1 anti-patterns per spec.yaml (no sync-specific fields in v1)
        let forbiddenFields: Set<String> = [
            "ckRecordID", "ckRecord", "syncToken", "changeTag",
            "cloudKitMetadata", "remoteId", "syncStatus", "vectorClock"
        ]
        let violations = propertyNames.intersection(forbiddenFields)
        #expect(violations.isEmpty,
                "NoteContainer must not have CloudKit/sync fields in v1: \(violations)")
    }

    @Test("NoteEntry has no CloudKit-specific or sync-token fields")
    func noteEntryHasNoSyncFields() throws {
        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "sync field check",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        let mirror = Mirror(reflecting: entry)
        let propertyNames = Set(mirror.children.compactMap { $0.label })

        let forbiddenFields: Set<String> = [
            "ckRecordID", "ckRecord", "syncToken", "changeTag",
            "cloudKitMetadata", "remoteId", "syncStatus", "vectorClock"
        ]
        let violations = propertyNames.intersection(forbiddenFields)
        #expect(violations.isEmpty,
                "NoteEntry must not have CloudKit/sync fields in v1: \(violations)")
    }

    // MARK: - UUID uniqueness

    @Test("Each NoteContainer created by NoteService has a unique UUID")
    func noteContainerUUIDsAreUnique() throws {
        let modelContainer = try makeSyncTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        var ids = Set<UUID>()
        for i in 0..<10 {
            let notebook = service.createNotebook(title: "Notebook \(i)")
            ids.insert(notebook.id)
        }
        // All 10 UUIDs must be distinct
        #expect(ids.count == 10)
    }

    @Test("Each NoteEntry added by NoteService has a unique UUID")
    func noteEntryUUIDsAreUnique() throws {
        let modelContainer = try makeSyncTestContainer()
        let service = NoteService(modelContainer: modelContainer)
        let notebook = service.createNotebook(title: "UUID Uniqueness Test")

        var ids = Set<UUID>()
        for i in 0..<10 {
            let entry = service.addEntry(
                to: notebook.id,
                rawText: "entry \(i)",
                processedText: nil,
                mode: "raw",
                durationMs: nil,
                language: nil
            )
            ids.insert(entry.id)
        }
        #expect(ids.count == 10)
    }
}

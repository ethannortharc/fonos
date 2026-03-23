// INV-10: History — dictation sessions stored, queryable, deletable.
// SwiftData persistence with date-range queries.
//
// Verifier: auto
// Level: unit (in-memory SwiftData ModelContainer)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV10HistoryTests

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeInMemoryContainer() throws -> ModelContainer {
    let schema = Schema([DictationSession.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct INV10HistoryTests {

    // MARK: - Insert and fetch

    @Test("Inserting a DictationSession allows it to be fetched back")
    func insertAndFetch() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let session = DictationSession(
            id: UUID(),
            date: .now,
            mode: "polish",
            inputText: "um yeah so basically",
            outputText: "Essentially,",
            destination: "clipboard",
            latencyMs: 320
        )
        context.insert(session)
        try context.save()

        let descriptor = FetchDescriptor<DictationSession>()
        let all = try context.fetch(descriptor)
        #expect(all.count == 1)
        #expect(all.first?.inputText == "um yeah so basically")
    }

    // MARK: - Required fields

    @Test("DictationSession has all required fields: id, date, mode, inputText, outputText, destination, latencyMs")
    func sessionHasRequiredFields() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let id = UUID()
        let date = Date(timeIntervalSince1970: 1_000_000)
        let session = DictationSession(
            id: id,
            date: date,
            mode: "raw",
            inputText: "hello",
            outputText: "hello",
            destination: "messages",
            latencyMs: 150
        )
        context.insert(session)
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<DictationSession>()).first!
        #expect(fetched.id == id)
        #expect(fetched.date == date)
        #expect(fetched.mode == "raw")
        #expect(fetched.inputText == "hello")
        #expect(fetched.outputText == "hello")
        #expect(fetched.destination == "messages")
        #expect(fetched.latencyMs == 150)
    }

    // MARK: - Delete

    @Test("Deleting a DictationSession removes it from storage")
    func deleteSession() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let session = DictationSession(id: UUID(), date: .now, mode: "raw",
                                       inputText: "test", outputText: "test",
                                       destination: "clipboard", latencyMs: 100)
        context.insert(session)
        try context.save()

        context.delete(session)
        try context.save()

        let all = try context.fetch(FetchDescriptor<DictationSession>())
        #expect(all.isEmpty)
    }

    // MARK: - Date-range query

    @Test("Date-range predicate returns only sessions within the range")
    func dateRangeQuery() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let base = Date(timeIntervalSince1970: 1_700_000_000)
        let sessions = [
            DictationSession(id: UUID(), date: base.addingTimeInterval(-3600), mode: "raw",
                             inputText: "before", outputText: "before", destination: "clipboard", latencyMs: 50),
            DictationSession(id: UUID(), date: base, mode: "polish",
                             inputText: "at", outputText: "at", destination: "clipboard", latencyMs: 80),
            DictationSession(id: UUID(), date: base.addingTimeInterval(3600), mode: "formal",
                             inputText: "after", outputText: "after", destination: "clipboard", latencyMs: 90),
        ]
        sessions.forEach { context.insert($0) }
        try context.save()

        let start = base.addingTimeInterval(-60)
        let end = base.addingTimeInterval(60)
        var descriptor = FetchDescriptor<DictationSession>(
            predicate: #Predicate { $0.date >= start && $0.date <= end }
        )
        let results = try context.fetch(descriptor)
        #expect(results.count == 1)
        #expect(results.first?.inputText == "at")
    }

    // MARK: - Mode filter

    @Test("Mode predicate returns only sessions with matching mode")
    func modeFilter() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let sessions = [
            DictationSession(id: UUID(), date: .now, mode: "polish",
                             inputText: "a", outputText: "a", destination: "clipboard", latencyMs: 10),
            DictationSession(id: UUID(), date: .now, mode: "raw",
                             inputText: "b", outputText: "b", destination: "clipboard", latencyMs: 20),
            DictationSession(id: UUID(), date: .now, mode: "polish",
                             inputText: "c", outputText: "c", destination: "clipboard", latencyMs: 30),
        ]
        sessions.forEach { context.insert($0) }
        try context.save()

        let descriptor = FetchDescriptor<DictationSession>(
            predicate: #Predicate { $0.mode == "polish" }
        )
        let polishSessions = try context.fetch(descriptor)
        #expect(polishSessions.count == 2)
    }

    // MARK: - Ordering

    @Test("Multiple sessions are fetchable ordered by date descending")
    func orderedByDateDescending() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let base = Date(timeIntervalSince1970: 1_700_000_000)
        let sessions = (0..<5).map { i in
            DictationSession(id: UUID(),
                             date: base.addingTimeInterval(Double(i) * 60),
                             mode: "raw",
                             inputText: "session \(i)",
                             outputText: "session \(i)",
                             destination: "clipboard",
                             latencyMs: 100)
        }
        sessions.forEach { context.insert($0) }
        try context.save()

        let descriptor = FetchDescriptor<DictationSession>(
            sortBy: [SortDescriptor(\.date, order: .reverse)]
        )
        let ordered = try context.fetch(descriptor)
        #expect(ordered.count == 5)
        // Most recent first
        #expect(ordered[0].date > ordered[1].date)
        #expect(ordered[1].date > ordered[2].date)
    }

    // MARK: - Text search

    @Test("Text search predicate matches sessions containing query in inputText")
    func searchByInputText() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let sessions = [
            DictationSession(id: UUID(), date: .now, mode: "raw",
                             inputText: "meeting agenda for Monday",
                             outputText: "meeting agenda for Monday",
                             destination: "clipboard", latencyMs: 50),
            DictationSession(id: UUID(), date: .now, mode: "raw",
                             inputText: "grocery list milk eggs bread",
                             outputText: "grocery list milk eggs bread",
                             destination: "clipboard", latencyMs: 50),
        ]
        sessions.forEach { context.insert($0) }
        try context.save()

        let query = "meeting"
        let descriptor = FetchDescriptor<DictationSession>(
            predicate: #Predicate { $0.inputText.contains(query) }
        )
        let results = try context.fetch(descriptor)
        #expect(results.count == 1)
        #expect(results.first?.inputText.contains("meeting") == true)
    }

    @Test("Text search predicate matches sessions containing query in outputText")
    func searchByOutputText() throws {
        let container = try makeInMemoryContainer()
        let context = container.mainContext

        let session = DictationSession(id: UUID(), date: .now, mode: "polish",
                                       inputText: "raw input",
                                       outputText: "polished professional output",
                                       destination: "clipboard", latencyMs: 200)
        context.insert(session)
        try context.save()

        let query = "professional"
        let descriptor = FetchDescriptor<DictationSession>(
            predicate: #Predicate { $0.outputText.contains(query) }
        )
        let results = try context.fetch(descriptor)
        #expect(results.count == 1)
    }
}

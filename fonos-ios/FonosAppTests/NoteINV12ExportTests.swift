// NoteINV12: Export produces Markdown and JSON matching the desktop (fonos-core) format.
//
// Verifier: auto
// Level: unit (pure string/data assertions, no I/O)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV12ExportTests
//
// TDD status: FAILING until NoteService.exportMarkdown() and exportJSON() are implemented.

import Testing
import Foundation
@testable import FonosApp

// MARK: - Test fixture builders

private func makeTestNotebook(
    id: UUID = UUID(),
    title: String = "Test Notebook",
    processingMode: String = "raw"
) -> NoteContainer {
    NoteContainer(
        id: id,
        title: title,
        containerType: "notebook",
        processingMode: processingMode,
        sttModelOverride: nil,
        llmModelOverride: nil,
        customPrompt: nil,
        createdAt: Date(timeIntervalSince1970: 1_700_000_000),
        updatedAt: Date(timeIntervalSince1970: 1_700_000_000)
    )
}

private func makeTestEntry(
    id: UUID = UUID(),
    createdAt: Date = Date(timeIntervalSince1970: 1_700_100_000),
    rawText: String = "raw transcript text",
    processedText: String? = nil,
    containerId: UUID = UUID(),
    mode: String = "raw",
    durationMs: Double? = 3000.0,
    language: String? = "en"
) -> NoteEntry {
    NoteEntry(
        id: id,
        createdAt: createdAt,
        sourceType: "note",
        rawText: rawText,
        processedText: processedText,
        containerId: containerId,
        mode: mode,
        durationMs: durationMs,
        language: language
    )
}

// MARK: - Tests

struct NoteINV12ExportTests {

    // MARK: - Markdown format

    @Test("exportMarkdown includes notebook title as H1 heading")
    func markdownIncludesTitle() throws {
        let notebook = makeTestNotebook(title: "My Work Notes")
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [])
        #expect(markdown.contains("# My Work Notes"))
    }

    @Test("exportMarkdown includes each entry's display text")
    func markdownIncludesEntryText() throws {
        let notebook = makeTestNotebook()
        let entry = makeTestEntry(
            rawText: "we need to ship the feature",
            processedText: "We need to ship the feature.",
            mode: "light_polish"
        )
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [entry])
        // For processed entries, markdown should prefer processedText
        #expect(markdown.contains("We need to ship the feature."))
    }

    @Test("exportMarkdown falls back to rawText when processedText is nil")
    func markdownFallsBackToRawText() throws {
        let notebook = makeTestNotebook()
        let entry = makeTestEntry(rawText: "raw only text", processedText: nil, mode: "raw")
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [entry])
        #expect(markdown.contains("raw only text"))
    }

    @Test("exportMarkdown with no entries returns non-empty string with at least the title")
    func markdownEmptyNotebook() throws {
        let notebook = makeTestNotebook(title: "Empty Notebook")
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [])
        #expect(!markdown.isEmpty)
        #expect(markdown.contains("Empty Notebook"))
    }

    @Test("exportMarkdown includes entry timestamp in ISO-8601 or readable date format")
    func markdownIncludesTimestamp() throws {
        let notebook = makeTestNotebook()
        let fixedDate = Date(timeIntervalSince1970: 1_700_100_000)
        let entry = makeTestEntry(createdAt: fixedDate)
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [entry])
        // Timestamp must appear in some recognisable date format
        // 2023-11-15 is the approximate date for unix 1_700_100_000
        #expect(markdown.contains("2023") || markdown.contains("2023-11") || markdown.contains("Nov"))
    }

    @Test("exportMarkdown with multiple entries separates them clearly")
    func markdownSeparatesEntries() throws {
        let notebook = makeTestNotebook()
        let entry1 = makeTestEntry(rawText: "first entry", processedText: nil)
        let entry2 = makeTestEntry(rawText: "second entry", processedText: nil)
        let service = NoteService()
        let markdown = service.exportMarkdown(notebook: notebook, entries: [entry1, entry2])
        #expect(markdown.contains("first entry"))
        #expect(markdown.contains("second entry"))
        // Entries should be separated by a horizontal rule or double newline
        let hasSeparator = markdown.contains("---") || markdown.contains("\n\n")
        #expect(hasSeparator)
    }

    // MARK: - JSON format

    @Test("exportJSON returns valid JSON data")
    func jsonIsValidJSON() throws {
        let notebook = makeTestNotebook()
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [])
        let data = jsonString.data(using: .utf8)!
        #expect(throws: Never.self) {
            _ = try JSONSerialization.jsonObject(with: data)
        }
    }

    @Test("exportJSON includes notebook title in top-level object")
    func jsonIncludesTitle() throws {
        let notebook = makeTestNotebook(title: "JSON Export Test")
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(obj?["title"] as? String == "JSON Export Test")
    }

    @Test("exportJSON includes entries array")
    func jsonIncludesEntriesArray() throws {
        let notebookId = UUID()
        let notebook = makeTestNotebook(id: notebookId)
        let entry = makeTestEntry(containerId: notebookId, rawText: "json entry text")
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [entry])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let entries = obj?["entries"] as? [[String: Any]]
        #expect(entries?.count == 1)
    }

    @Test("exportJSON entry contains rawText field")
    func jsonEntryContainsRawText() throws {
        let notebookId = UUID()
        let notebook = makeTestNotebook(id: notebookId)
        let entry = makeTestEntry(containerId: notebookId, rawText: "the raw text value")
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [entry])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let entries = obj?["entries"] as? [[String: Any]]
        let rawText = entries?.first?["raw_text"] as? String
            ?? entries?.first?["rawText"] as? String
        #expect(rawText == "the raw text value")
    }

    @Test("exportJSON entry contains id field as UUID string")
    func jsonEntryContainsId() throws {
        let notebookId = UUID()
        let entryId = UUID()
        let notebook = makeTestNotebook(id: notebookId)
        let entry = makeTestEntry(id: entryId, containerId: notebookId)
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [entry])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let entries = obj?["entries"] as? [[String: Any]]
        let idString = entries?.first?["id"] as? String
        #expect(idString == entryId.uuidString)
    }

    @Test("exportJSON empty entries list produces entries array with count 0")
    func jsonEmptyEntries() throws {
        let notebook = makeTestNotebook()
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let entries = obj?["entries"] as? [[String: Any]]
        #expect(entries?.count == 0 || entries == nil) // empty array or absent key both acceptable
    }

    @Test("exportJSON notebook id field matches NoteContainer.id")
    func jsonNotebookIdMatches() throws {
        let notebookId = UUID()
        let notebook = makeTestNotebook(id: notebookId)
        let service = NoteService()
        let jsonString = service.exportJSON(notebook: notebook, entries: [])
        let data = jsonString.data(using: .utf8)!
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let idString = obj?["id"] as? String
        #expect(idString == notebookId.uuidString)
    }
}

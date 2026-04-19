import Foundation
import SwiftData

/// Errors thrown by NoteService.
enum NoteServiceError: LocalizedError, Equatable {
    case cannotDeleteQuickNote
    case notebookNotFound
    case entryNotFound

    var errorDescription: String? {
        switch self {
        case .cannotDeleteQuickNote:
            "The Quick Note notebook cannot be deleted."
        case .notebookNotFound:
            "Notebook not found."
        case .entryNotFound:
            "Entry not found."
        }
    }
}

/// Service for creating and managing NoteContainer and NoteEntry objects.
@MainActor
final class NoteService {

    private let modelContainer: ModelContainer

    private static let quickNoteTitle = "Quick Note"

    init(modelContainer: ModelContainer) {
        self.modelContainer = modelContainer
    }

    /// Convenience initializer with an in-memory container (for tests and export-only usage).
    convenience init() {
        let schema = Schema([NoteContainer.self, NoteEntry.self])
        let config = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = try! ModelContainer(for: schema, configurations: [config])
        self.init(modelContainer: container)
    }

    // MARK: - Quick Note

    /// Returns the Quick Note default notebook, creating it if it doesn't exist.
    func quickNoteNotebook() -> NoteContainer? {
        let context = modelContainer.mainContext
        let title = Self.quickNoteTitle
        let descriptor = FetchDescriptor<NoteContainer>(
            predicate: #Predicate { $0.title == title }
        )
        if let existing = try? context.fetch(descriptor).first {
            return existing
        }
        let notebook = NoteContainer(
            id: UUID(),
            title: title,
            containerType: "notebook",
            processingMode: "raw",
            createdAt: Date(),
            updatedAt: Date()
        )
        context.insert(notebook)
        try? context.save()
        return notebook
    }

    // MARK: - Notebook CRUD

    /// Creates a new notebook with the given title and persists it.
    @discardableResult
    func createNotebook(
        title: String,
        containerType: String = "notebook",
        processingMode: String = "raw",
        sttModelOverride: String? = nil,
        llmModelOverride: String? = nil,
        customPrompt: String? = nil
    ) -> NoteContainer {
        let notebook = NoteContainer(
            id: UUID(),
            title: title,
            containerType: containerType,
            processingMode: processingMode,
            sttModelOverride: sttModelOverride,
            llmModelOverride: llmModelOverride,
            customPrompt: customPrompt,
            createdAt: Date(),
            updatedAt: Date()
        )
        let context = modelContainer.mainContext
        context.insert(notebook)
        try? context.save()
        return notebook
    }

    /// Renames a notebook.
    func renameNotebook(_ id: UUID, to newTitle: String) {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteContainer>(
            predicate: #Predicate { $0.id == id }
        )
        guard let notebook = try? context.fetch(descriptor).first else { return }
        notebook.title = newTitle
        notebook.updatedAt = Date()
        try? context.save()
    }

    /// Deletes a notebook. Throws if it's the Quick Note notebook.
    func deleteNotebook(_ id: UUID) throws {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteContainer>(
            predicate: #Predicate { $0.id == id }
        )
        guard let notebook = try? context.fetch(descriptor).first else { return }
        if notebook.title == Self.quickNoteTitle {
            throw NoteServiceError.cannotDeleteQuickNote
        }
        context.delete(notebook)
        try? context.save()
    }

    /// Fetches all notebooks sorted by updatedAt descending.
    func allNotebooks() -> [NoteContainer] {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteContainer>(
            sortBy: [SortDescriptor(\.updatedAt, order: .reverse)]
        )
        return (try? context.fetch(descriptor)) ?? []
    }

    // MARK: - Entry CRUD

    /// Adds a new entry to the notebook identified by `containerId`.
    @discardableResult
    func addEntry(
        to containerId: UUID,
        rawText: String,
        processedText: String? = nil,
        mode: String = "raw",
        durationMs: Double? = nil,
        language: String? = nil
    ) -> NoteEntry {
        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: rawText,
            processedText: processedText,
            containerId: containerId,
            mode: mode,
            durationMs: durationMs,
            language: language
        )
        let context = modelContainer.mainContext
        context.insert(entry)
        try? context.save()
        return entry
    }

    /// Updates the rawText of an entry.
    func updateEntry(_ id: UUID, text: String) {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteEntry>(
            predicate: #Predicate { $0.id == id }
        )
        guard let entry = try? context.fetch(descriptor).first else { return }
        entry.rawText = text
        try? context.save()
    }

    /// Deletes an entry.
    func deleteEntry(_ id: UUID) {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteEntry>(
            predicate: #Predicate { $0.id == id }
        )
        guard let entry = try? context.fetch(descriptor).first else { return }
        context.delete(entry)
        try? context.save()
    }

    /// Returns entries for a notebook, sorted by createdAt descending (newest first).
    func entriesForNotebook(_ containerId: UUID) -> [NoteEntry] {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteEntry>(
            predicate: #Predicate { $0.containerId == containerId },
            sortBy: [SortDescriptor(\.createdAt, order: .reverse)]
        )
        return (try? context.fetch(descriptor)) ?? []
    }

    /// Returns the count of entries for a given notebook.
    func entryCount(for containerId: UUID) -> Int {
        entriesForNotebook(containerId).count
    }

    // MARK: - Export

    /// ISO-8601 date formatter for export timestamps.
    private static let isoFormatter: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()

    /// Exports a notebook and its entries as Markdown.
    func exportMarkdown(notebook: NoteContainer, entries: [NoteEntry]) -> String {
        var lines: [String] = []
        lines.append("# \(notebook.title)")
        lines.append("")

        if entries.isEmpty {
            lines.append("*No entries.*")
        } else {
            for (index, entry) in entries.enumerated() {
                let timestamp = Self.isoFormatter.string(from: entry.createdAt)
                lines.append("**\(timestamp)**")
                lines.append("")
                let text = entry.processedText ?? entry.rawText
                lines.append(text)
                if index < entries.count - 1 {
                    lines.append("")
                    lines.append("---")
                    lines.append("")
                }
            }
        }

        return lines.joined(separator: "\n")
    }

    /// Exports a notebook and its entries as JSON.
    func exportJSON(notebook: NoteContainer, entries: [NoteEntry]) -> String {
        var root: [String: Any] = [
            "id": notebook.id.uuidString,
            "title": notebook.title,
            "container_type": notebook.containerType,
            "processing_mode": notebook.processingMode,
            "created_at": Self.isoFormatter.string(from: notebook.createdAt),
            "updated_at": Self.isoFormatter.string(from: notebook.updatedAt)
        ]

        let entryDicts: [[String: Any]] = entries.map { entry in
            var dict: [String: Any] = [
                "id": entry.id.uuidString,
                "created_at": Self.isoFormatter.string(from: entry.createdAt),
                "source_type": entry.sourceType,
                "raw_text": entry.rawText,
                "mode": entry.mode,
                "container_id": entry.containerId.uuidString
            ]
            if let processed = entry.processedText {
                dict["processed_text"] = processed
            }
            if let duration = entry.durationMs {
                dict["duration_ms"] = duration
            }
            if let lang = entry.language {
                dict["language"] = lang
            }
            return dict
        }
        root["entries"] = entryDicts

        guard let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys]),
              let jsonString = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return jsonString
    }

    // MARK: - Per-notebook Configuration

    /// v1 update — kept for the existing tests that still touch
    /// `processingMode` / `customPrompt`. Runtime no longer reads these fields;
    /// the v0.2.0 backfill consolidates them into `systemPrompt`.
    func updateNotebookConfig(
        _ id: UUID,
        processingMode: String? = nil,
        sttModelOverride: String? = nil,
        llmModelOverride: String? = nil,
        customPrompt: String? = nil
    ) {
        let context = modelContainer.mainContext
        let descriptor = FetchDescriptor<NoteContainer>(
            predicate: #Predicate { $0.id == id }
        )
        guard let notebook = try? context.fetch(descriptor).first else { return }
        if let processingMode { notebook.processingMode = processingMode }
        notebook.sttModelOverride = sttModelOverride
        notebook.llmModelOverride = llmModelOverride
        notebook.customPrompt = customPrompt
        notebook.updatedAt = Date()
        try? context.save()
    }

    // MARK: - Per-notebook Configuration (v2)

    /// v2 config update — operates on the new fields.
    ///
    /// Each parameter is a double-optional so callers can distinguish
    /// "leave unchanged" (`nil`) from "clear to nil" (`.some(nil)`).
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
}

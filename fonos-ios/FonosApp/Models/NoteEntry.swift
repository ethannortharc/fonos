import Foundation
import SwiftData

/// Persisted voice note entry (text-only after transcription).
@Model
final class NoteEntry {
    var id: UUID
    var createdAt: Date
    var sourceType: String
    var rawText: String
    var processedText: String?
    var containerId: UUID
    var mode: String
    var durationMs: Double?
    var language: String?

    init(
        id: UUID = UUID(),
        createdAt: Date = Date(),
        sourceType: String = "note",
        rawText: String = "",
        processedText: String? = nil,
        containerId: UUID = UUID(),
        mode: String = "raw",
        durationMs: Double? = nil,
        language: String? = nil
    ) {
        self.id = id
        self.createdAt = createdAt
        self.sourceType = sourceType
        self.rawText = rawText
        self.processedText = processedText
        self.containerId = containerId
        self.mode = mode
        self.durationMs = durationMs
        self.language = language
    }
}

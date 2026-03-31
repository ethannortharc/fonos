import Foundation
import SwiftData

/// Persisted notebook (container) for voice note entries.
@Model
final class NoteContainer {
    var id: UUID
    var title: String
    var containerType: String
    var processingMode: String
    var sttModelOverride: String?
    var llmModelOverride: String?
    var customPrompt: String?
    var createdAt: Date
    var updatedAt: Date

    init(
        id: UUID = UUID(),
        title: String = "",
        containerType: String = "notebook",
        processingMode: String = "raw",
        sttModelOverride: String? = nil,
        llmModelOverride: String? = nil,
        customPrompt: String? = nil,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.title = title
        self.containerType = containerType
        self.processingMode = processingMode
        self.sttModelOverride = sttModelOverride
        self.llmModelOverride = llmModelOverride
        self.customPrompt = customPrompt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

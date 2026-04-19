import Foundation
import SwiftData

/// Persisted notebook (container) for voice note entries.
@Model
final class NoteContainer {
    var id: UUID
    var title: String
    var containerType: String

    // MARK: - Deprecated v1 (kept for backfill, removed in v0.3.x)

    /// One of "raw" / "light_polish" / "polish" / "summarize" / etc.
    /// No longer read by the runtime — back-filled into `systemPrompt` on first launch.
    var processingMode: String

    /// Free-text prompt from v1 UI. Back-filled into `systemPrompt` on first launch.
    var customPrompt: String?

    // MARK: - Existing model overrides

    var sttModelOverride: String?
    var llmModelOverride: String?

    // MARK: - v2 (active)

    /// Free-text instruction for the LLM. Empty string = Raw mode (no LLM call).
    /// Replaces v1's `processingMode + customPrompt` pair.
    var systemPrompt: String = ""

    /// BCP-47 locale id (e.g. "zh-CN"). nil = use system default.
    var sttLanguage: String?

    /// BCP-47 locale id for the LLM output. nil = follow `sttLanguage`.
    var outputLanguage: String?

    /// When true, EntryRow shows the raw STT transcript inline below the processed text.
    var showRawInline: Bool = false

    /// Optional per-notebook Siri phrase override. nil = "Record to {title}".
    var siriPhrase: String?

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
        updatedAt: Date = Date(),
        systemPrompt: String = "",
        sttLanguage: String? = nil,
        outputLanguage: String? = nil,
        showRawInline: Bool = false,
        siriPhrase: String? = nil
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
        self.systemPrompt = systemPrompt
        self.sttLanguage = sttLanguage
        self.outputLanguage = outputLanguage
        self.showRawInline = showRawInline
        self.siriPhrase = siriPhrase
    }
}

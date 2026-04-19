import Foundation

/// Pure translation from a NoteContainer into the parameters its STT + LLM
/// invocation actually need. Single source of truth for both the runtime
/// pipeline (NoteViewModel) and the Settings UI summary chip.
enum NotebookPipeline {

    struct Resolved: Equatable {
        let sttLanguage: String?
        let sttModelOverride: String?
        let llm: NotebookLLMConfig?
    }

    static func resolve(_ n: NoteContainer) -> Resolved {
        let trimmed = n.systemPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let llm: NotebookLLMConfig? = trimmed.isEmpty ? nil : NotebookLLMConfig(
            systemPrompt: trimmed,
            outputLanguage: n.outputLanguage ?? n.sttLanguage,
            modelOverride: n.llmModelOverride
        )
        return Resolved(
            sttLanguage: n.sttLanguage,
            sttModelOverride: n.sttModelOverride,
            llm: llm
        )
    }
}

struct NotebookLLMConfig: Equatable, Sendable {
    let systemPrompt: String
    let outputLanguage: String?
    let modelOverride: String?
}

import Foundation

/// A configured mode with optional per-mode model overrides and processing settings.
/// Wraps a `Mode` enum value with additional metadata and pipeline configuration.
struct ModeConfig: Codable, Equatable, Hashable, Identifiable, Sendable {
    var id: String              // "raw", "polish", "formal", "translate", or custom UUID
    var mode: Mode              // The mode type
    var name: String            // Display name
    var icon: String            // SF Symbol name
    var description: String     // Short description
    var sttModelID: String?     // STT model profile ID override (nil = use default)
    var llmModelID: String?     // LLM model profile ID override (nil = use default)
    var sttPrompt: String       // Whisper prompt hint
    var sttTemperature: Double  // STT temperature (0.0–1.0)
    var outputLanguage: String  // "auto" or specific language code
    var autoPaste: Bool         // Auto-insert result into active text field
    var isBuiltIn: Bool         // true = cannot delete

    // MARK: - Built-in configs

    static let builtInConfigs: [ModeConfig] = [
        ModeConfig(
            id: "raw",
            mode: .raw,
            name: "Raw",
            icon: "waveform",
            description: "Verbatim transcription with no processing.",
            sttModelID: nil,
            llmModelID: nil,
            sttPrompt: "",
            sttTemperature: 0.0,
            outputLanguage: "auto",
            autoPaste: false,
            isBuiltIn: true
        ),
        ModeConfig(
            id: "polish",
            mode: .polish,
            name: "Polish",
            icon: "sparkles",
            description: "Remove filler words and clean up the transcript.",
            sttModelID: nil,
            llmModelID: nil,
            sttPrompt: "",
            sttTemperature: 0.0,
            outputLanguage: "auto",
            autoPaste: false,
            isBuiltIn: true
        ),
        ModeConfig(
            id: "formal",
            mode: .formal,
            name: "Formal",
            icon: "briefcase",
            description: "Rewrite in professional business style.",
            sttModelID: nil,
            llmModelID: nil,
            sttPrompt: "",
            sttTemperature: 0.0,
            outputLanguage: "auto",
            autoPaste: false,
            isBuiltIn: true
        ),
        ModeConfig(
            id: "translate",
            mode: .translate(targetLanguage: "English"),
            name: "Translate",
            icon: "globe",
            description: "Translate the transcription to another language.",
            sttModelID: nil,
            llmModelID: nil,
            sttPrompt: "",
            sttTemperature: 0.0,
            outputLanguage: "auto",
            autoPaste: false,
            isBuiltIn: true
        )
    ]

    // MARK: - Pipeline summary

    /// Returns a human-readable pipeline summary string for display in the modes list.
    func pipelineSummary(sttModel: ModelProfile?, llmModel: ModelProfile?) -> String {
        let sttName = sttModel?.name ?? "Default STT"
        if mode.requiresLLM {
            let llmName = llmModel?.name ?? "Default LLM"
            return "STT: \(sttName) → LLM: \(llmName)"
        } else {
            return "STT: \(sttName)"
        }
    }
}

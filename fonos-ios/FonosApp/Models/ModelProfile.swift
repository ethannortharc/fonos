import Foundation

/// Represents a configured AI model profile (STT, LLM, or both).
struct ModelProfile: Codable, Equatable, Hashable, Identifiable, Sendable {
    var id: String              // e.g., "openai-1234567890"
    var name: String            // User-friendly name
    var provider: String        // openai, anthropic, google, ollama, lmstudio, custom
    var modelID: String         // Model identifier (e.g., "gpt-4o")
    var baseURL: String?        // Custom endpoint URL
    var capabilities: [String]  // ["stt", "llm"] or subset

    // MARK: - Convenience

    /// True if this model can perform speech-to-text.
    var hasSTT: Bool { capabilities.contains("stt") }

    /// True if this model can perform LLM processing.
    var hasLLM: Bool { capabilities.contains("llm") }

    // MARK: - Init

    init(
        id: String,
        name: String,
        provider: String,
        modelID: String,
        baseURL: String? = nil,
        capabilities: [String] = ["llm"]
    ) {
        self.id = id
        self.name = name
        self.provider = provider
        self.modelID = modelID
        self.baseURL = baseURL
        self.capabilities = capabilities
    }
}

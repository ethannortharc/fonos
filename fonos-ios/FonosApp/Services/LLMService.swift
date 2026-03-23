import Foundation

/// Sends text to an LLM endpoint for post-processing.
/// Scaffold — implementation goes in wp-executor.
final class LLMService: Sendable {
    func process(text: String, mode: Mode, modelProfile: ModelProfile) async throws -> String {
        throw LLMError.notConfigured
    }
}

enum LLMError: LocalizedError {
    case notConfigured
    case networkError(String)
    case parseError(String)
    case authenticationError(String)

    var errorDescription: String? {
        switch self {
        case .notConfigured: "LLM provider not configured."
        case .networkError(let msg): "Network error: \(msg)"
        case .parseError(let msg): "Failed to parse response: \(msg)"
        case .authenticationError(let msg): "Authentication failed: \(msg)"
        }
    }
}

import Foundation

/// Protocol all STT provider implementations must conform to.
protocol STTProvider: Sendable {
    func transcribe(audioData: Data, language: String?) async throws -> String
}

/// Scaffold placeholder — implementations go in wp-executor.
enum STTError: LocalizedError {
    case permissionDenied
    case noResult
    case networkError(String)
    case authenticationError(String)
    case parseError(String)

    var errorDescription: String? {
        switch self {
        case .permissionDenied: "Microphone or speech recognition permission denied."
        case .noResult: "No transcription result returned."
        case .networkError(let msg): "Network error: \(msg)"
        case .authenticationError(let msg): "Authentication failed: \(msg)"
        case .parseError(let msg): "Failed to parse response: \(msg)"
        }
    }
}

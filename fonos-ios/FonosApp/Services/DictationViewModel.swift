import Foundation

// MARK: - DictationError

/// Errors thrown by DictationViewModel.
enum DictationError: LocalizedError, Equatable {
    case noSTTProviderConfigured
    case transcriptionFailed(String)

    var errorDescription: String? {
        switch self {
        case .noSTTProviderConfigured:
            "No speech-to-text provider is configured. Please configure one in Settings."
        case .transcriptionFailed(let reason):
            "Transcription failed: \(reason)"
        }
    }
}

// MARK: - ProcessResult

/// The result of audio processing (STT + optional LLM).
struct ProcessResult: Sendable {
    /// The processed or raw text.
    let text: String
    /// True if LLM processing failed and we fell back to the raw STT transcript.
    let isRawFallback: Bool

    init(text: String, isRawFallback: Bool = false) {
        self.text = text
        self.isRawFallback = isRawFallback
    }
}

// MARK: - DictationViewModel

/// Orchestrates STT transcription and optional LLM post-processing.
final class DictationViewModel: @unchecked Sendable {
    private let sttProvider: (any STTProvider)?
    private let llmService: LLMService?

    init(sttProvider: (any STTProvider)?,
         llmService: LLMService? = nil) {
        self.sttProvider = sttProvider
        self.llmService = llmService
    }

    /// Transcribe audio data using the configured STT provider.
    /// - Throws: `DictationError.noSTTProviderConfigured` if no provider is set.
    func transcribeAudio(_ audioData: Data, language: String?) async throws -> String {
        guard let provider = sttProvider else {
            throw DictationError.noSTTProviderConfigured
        }
        return try await provider.transcribe(audioData: audioData, language: language)
    }

    /// Transcribe audio and optionally apply LLM post-processing.
    /// If LLM processing fails, falls back to the raw transcript.
    func processAudio(_ audioData: Data,
                      language: String?,
                      mode: Mode) async throws -> ProcessResult {
        let rawTranscript = try await transcribeAudio(audioData, language: language)

        // Raw mode or no LLM: return transcript directly
        guard mode.requiresLLM, let llm = llmService else {
            return ProcessResult(text: rawTranscript, isRawFallback: false)
        }

        do {
            let processed = try await llm.process(text: rawTranscript, mode: mode)
            return ProcessResult(text: processed, isRawFallback: false)
        } catch {
            // LLM failure — fall back to raw transcript
            return ProcessResult(text: rawTranscript, isRawFallback: true)
        }
    }
}

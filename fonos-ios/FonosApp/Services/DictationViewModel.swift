import Foundation
import SwiftUI
import Combine

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
/// Also drives the recording UI state for DictationView.
final class DictationViewModel: ObservableObject, @unchecked Sendable {

    // MARK: - UI State

    enum RecordingState: Equatable {
        case idle
        case recording
        case processing
        case result(transcript: String, processed: String?)
        case error(message: String)
    }

    @Published var recordingState: RecordingState = .idle
    @Published var audioLevel: Float = 0
    @Published var currentMode: Mode = .raw
    @Published var sttLatency: TimeInterval = 0
    @Published var llmLatency: TimeInterval = 0

    /// Convenience for views/tests: true when actively recording.
    var isRecording: Bool {
        if case .recording = recordingState { return true }
        return false
    }

    // MARK: - Services

    private let sttProvider: (any STTProvider)?
    private let llmService: LLMService?
    private let audioCapture: AudioCaptureService
    private var audioLevelObservation: Any?

    // MARK: - Init

    /// No-arg initialiser for preview and test use.
    /// Uses no STT provider — transcription will throw DictationError.noSTTProviderConfigured.
    init() {
        sttProvider = nil
        llmService = nil
        audioCapture = AudioCaptureService()
        observeAudioLevel()
    }

    /// Designated initialiser for production use.
    init(sttProvider: (any STTProvider)?,
         llmService: LLMService? = nil,
         audioCapture: AudioCaptureService = AudioCaptureService()) {
        self.sttProvider = sttProvider
        self.llmService = llmService
        self.audioCapture = audioCapture
        observeAudioLevel()
    }

    private func observeAudioLevel() {
        // Forward audio level from AudioCaptureService to this view model
        audioLevelObservation = audioCapture.$audioLevel
            .receive(on: DispatchQueue.main)
            .sink { [weak self] level in
                self?.audioLevel = level
            }
    }

    // MARK: - Recording Control

    @MainActor
    func startRecording() {
        guard !isRecording else { return }

        let permission = audioCapture.micPermissionStatus()

        switch permission {
        case .granted:
            // Permission already granted — start immediately (synchronous, no deadlock)
            doStartCapture()
        case .undetermined:
            // Need to request — do it async, then start synchronously on callback
            Task { @MainActor in
                let granted = await audioCapture.requestMicPermission()
                if granted {
                    doStartCapture()
                } else {
                    recordingState = .error(message: "Microphone permission is required. Please enable it in Settings.")
                }
            }
        case .denied:
            recordingState = .error(message: "Microphone permission denied. Go to Settings → Privacy → Microphone to enable.")
        @unknown default:
            recordingState = .error(message: "Microphone permission unavailable.")
        }
    }

    /// Actually start audio capture. Called synchronously on MainActor after permission is confirmed.
    @MainActor
    private func doStartCapture() {
        do {
            try audioCapture.startCapture()
            recordingState = .recording
        } catch {
            recordingState = .error(message: error.localizedDescription)
        }
    }

    @MainActor
    func stopRecording() {
        guard isRecording else { return }
        let wavData = audioCapture.stopCapture()
        audioLevel = 0

        // If no STT provider is configured, skip processing entirely
        guard sttProvider != nil else {
            recordingState = .error(message: "No speech-to-text provider configured. Add a model with STT capability in Settings.")
            return
        }

        recordingState = .processing

        Task {
            do {
                let result = try await processAudio(
                    wavData ?? Data(),
                    language: nil,
                    mode: currentMode
                )
                await MainActor.run {
                    recordingState = .result(
                        transcript: result.text,
                        processed: result.isRawFallback ? nil : result.text
                    )
                }
            } catch {
                await MainActor.run {
                    recordingState = .error(message: error.localizedDescription)
                }
            }
        }
    }

    @MainActor
    func sendToDestination(_ destination: AnyTextDestination) {
        guard case .result(let transcript, let processed) = recordingState else { return }
        let text = processed ?? transcript
        Task {
            try? await destination.send(text: text)
        }
    }

    // MARK: - Core Pipeline (also used by tests directly)

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

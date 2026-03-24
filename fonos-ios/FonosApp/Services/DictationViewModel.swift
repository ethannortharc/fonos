import Foundation
import SwiftUI
import os.log

private let log = Logger(subsystem: "com.fonos.ios", category: "DictationViewModel")

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
    private var levelPollTimer: Timer?

    // MARK: - Init

    /// No-arg initialiser for preview and test use.
    /// Uses no STT provider — transcription will throw DictationError.noSTTProviderConfigured.
    init() {
        sttProvider = nil
        llmService = nil
        audioCapture = AudioCaptureService()
    }

    /// Designated initialiser for production use.
    init(sttProvider: (any STTProvider)?,
         llmService: LLMService? = nil,
         audioCapture: AudioCaptureService = AudioCaptureService()) {
        self.sttProvider = sttProvider
        self.llmService = llmService
        self.audioCapture = audioCapture
    }

    /// Poll audio level at 10fps — fast enough for waveform, slow enough to not starve touch events.
    private func startLevelPolling() {
        levelPollTimer?.invalidate()
        levelPollTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            guard let self else { return }
            let level = self.audioCapture.currentAudioLevel
            // Only update if level actually changed (avoids unnecessary SwiftUI re-renders)
            if abs(self.audioLevel - level) > 0.01 {
                self.audioLevel = level
            }
        }
    }

    private func stopLevelPolling() {
        levelPollTimer?.invalidate()
        levelPollTimer = nil
        audioLevel = 0
    }

    // MARK: - Recording Control

    @MainActor
    func startRecording() {
        log.info("▶️ startRecording() called, isRecording=\(self.isRecording)")
        guard !isRecording else {
            log.warning("⚠️ Already recording, ignoring tap")
            return
        }

        let permission = audioCapture.micPermissionStatus()
        log.info("🎤 Mic permission status: \(String(describing: permission.rawValue))")

        switch permission {
        case .granted:
            log.info("✅ Permission granted, calling doStartCapture()")
            doStartCapture()
        case .undetermined:
            log.info("❓ Permission undetermined, requesting...")
            Task { @MainActor in
                let granted = await audioCapture.requestMicPermission()
                log.info("🎤 Permission request result: \(granted)")
                if granted {
                    doStartCapture()
                } else {
                    recordingState = .error(message: "Microphone permission is required. Please enable it in Settings.")
                }
            }
        case .denied:
            log.error("❌ Permission denied")
            recordingState = .error(message: "Microphone permission denied. Go to Settings → Privacy → Microphone to enable.")
        @unknown default:
            log.error("❌ Permission unknown")
            recordingState = .error(message: "Microphone permission unavailable.")
        }
    }

    @MainActor
    private func doStartCapture() {
        log.info("🔴 doStartCapture() — DEBUG: testing UI only, no engine")

        // DEBUG: Skip audio engine entirely to isolate if freeze is UI or engine
        recordingState = .recording
        log.info("✅ State set to .recording (engine NOT started)")

        // NOTE: Uncomment below to re-enable real recording:
        // do {
        //     try audioCapture.startCapture()
        //     log.info("✅ startCapture() succeeded, setting state to .recording")
        //     recordingState = .recording
        //     startLevelPolling()
        // } catch {
        //     log.error("❌ startCapture() threw: \(error.localizedDescription)")
        //     recordingState = .error(message: error.localizedDescription)
        // }
    }

    @MainActor
    func stopRecording() {
        guard isRecording else { return }
        let wavData = audioCapture.stopCapture()
        stopLevelPolling()

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

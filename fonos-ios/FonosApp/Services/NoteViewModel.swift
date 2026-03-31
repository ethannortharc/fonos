import Foundation
import SwiftUI
import os.log

private let noteVMLog = Logger(subsystem: "com.fonos.ios", category: "NoteViewModel")

// MARK: - NoteLLMProvider Protocol

/// Protocol for LLM providers used by NoteViewModel.
/// Separate from LLMService to allow test injection with simpler interface.
protocol NoteLLMProvider: AnyObject {
    func process(text: String, prompt: String?) async throws -> String
}

// MARK: - NoteViewModel

/// Orchestrates the recording → transcription → storage pipeline for voice notes.
/// Follows the DictationViewModel pattern for provider resolution and audio level polling.
@MainActor
final class NoteViewModel: ObservableObject, @unchecked Sendable {

    // MARK: - Recording State

    enum RecordingState: Equatable {
        case idle
        case recording
        case processing
        case done
        case error(message: String)
    }

    @Published var recordingState: RecordingState = .idle
    @Published var audioLevel: Float = 0

    // MARK: - Services

    private let noteService: NoteService
    private let audioCapture: AudioCaptureService
    private var levelPollTimer: Timer?

    // MARK: - Injected providers (for tests)

    private var _testSTT: (any STTProvider)?
    private var _testLLM: (any NoteLLMProvider)?

    // MARK: - Config (for production provider resolution)

    var config: AppConfig = AppConfig()

    // MARK: - Init (production)

    init(noteService: NoteService, audioCapture: AudioCaptureService = AudioCaptureService()) {
        self.noteService = noteService
        self.audioCapture = audioCapture
    }

    /// Test/injection initialiser — accepts any STTProvider and any NoteLLMProvider.
    init(noteService: NoteService,
         sttProvider: (any STTProvider)?,
         llmProvider: (any NoteLLMProvider)?,
         audioCapture: AudioCaptureService = AudioCaptureService()) {
        self.noteService = noteService
        self._testSTT = sttProvider
        self._testLLM = llmProvider
        self.audioCapture = audioCapture
    }

    // MARK: - Provider Resolution

    private var resolvedSTT: any STTProvider {
        if let test = _testSTT { return test }
        // Fall back to Apple on-device STT
        return AppleSTT()
    }

    private var resolvedLLM: (any NoteLLMProvider)? {
        if let test = _testLLM { return test }
        // In production: resolve from config (mirroring DictationViewModel pattern)
        let profileID = config.llmProfile
        guard !profileID.isEmpty,
              let profile = config.modelProfiles.first(where: { $0.id == profileID }) else {
            return nil
        }
        let key = (try? KeychainStore(service: "com.fonos.models").get(profile.id)) ?? ""
        let baseURL = profile.baseURL ?? ""
        let service = LLMService(
            apiKey: key,
            modelID: profile.modelID,
            baseURL: baseURL.isEmpty ? "https://api.openai.com" : baseURL
        )
        return LLMServiceNoteAdapter(service: service)
    }

    // MARK: - Recording Control

    func startRecording() {
        guard recordingState == .idle || {
            if case .done = recordingState { return true }
            if case .error = recordingState { return true }
            return false
        }() else { return }

        let permission = audioCapture.micPermissionStatus()
        switch permission {
        case .granted:
            doStartCapture()
        case .undetermined:
            Task { @MainActor in
                let granted = await audioCapture.requestMicPermission()
                if granted {
                    doStartCapture()
                } else {
                    recordingState = .error(message: "Microphone permission is required.")
                }
            }
        case .denied:
            recordingState = .error(message: "Microphone permission denied.")
        @unknown default:
            recordingState = .error(message: "Microphone permission unavailable.")
        }
    }

    private func doStartCapture() {
        recordingState = .recording
        startLevelPolling()
        audioCapture.startCapture { [weak self] error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error {
                    self.recordingState = .error(message: error.localizedDescription)
                    self.stopLevelPolling()
                }
            }
        }
    }

    func stopRecording(to containerId: UUID, mode: String) {
        guard case .recording = recordingState else { return }
        stopLevelPolling()

        let wavData = audioCapture.stopCapture() ?? Data()
        recordingState = .processing

        Task {
            await recordAndStore(to: containerId, mode: mode, audioData: wavData)
        }
    }

    // MARK: - Core Pipeline

    /// Transcribe audio, optionally apply LLM, then persist a NoteEntry.
    /// Called directly by tests with injected audioData.
    /// Never throws — errors are swallowed (no entry created on STT failure).
    func recordAndStore(to containerId: UUID, mode: String, audioData: Data) async {
        // Capture providers on main actor before crossing isolation boundaries
        let stt = resolvedSTT
        let llm = resolvedLLM

        do {
            let rawText = try await withCheckedThrowingContinuation { continuation in
                Task {
                    do {
                        let text = try await stt.transcribe(audioData: audioData, language: nil)
                        continuation.resume(returning: text)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }

            var processedText: String? = nil

            if mode != "raw", let llmProvider = llm {
                do {
                    let result = try await withCheckedThrowingContinuation { continuation in
                        Task {
                            do {
                                let text = try await llmProvider.process(text: rawText, prompt: nil)
                                continuation.resume(returning: text)
                            } catch {
                                continuation.resume(throwing: error)
                            }
                        }
                    }
                    processedText = result
                } catch {
                    noteVMLog.warning("LLM processing failed, using raw transcript: \(error.localizedDescription)")
                    processedText = nil
                }
            }

            noteService.addEntry(
                to: containerId,
                rawText: rawText,
                processedText: processedText,
                mode: mode
            )

            recordingState = .done
        } catch {
            noteVMLog.error("STT transcription failed: \(error.localizedDescription)")
            recordingState = .error(message: error.localizedDescription)
        }
    }

    // MARK: - Audio Level Polling

    private func startLevelPolling() {
        levelPollTimer?.invalidate()
        levelPollTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            guard let self else { return }
            let level = self.audioCapture.currentAudioLevel
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
}

// MARK: - LLMService Adapter

/// Wraps LLMService to conform to NoteLLMProvider.
/// Uses .raw mode (no transformation) as a base; prompt is ignored for the note pipeline.
private final class LLMServiceNoteAdapter: NoteLLMProvider {
    private let service: LLMService

    init(service: LLMService) {
        self.service = service
    }

    func process(text: String, prompt: String?) async throws -> String {
        try await service.process(text: text, mode: .polish)
    }
}

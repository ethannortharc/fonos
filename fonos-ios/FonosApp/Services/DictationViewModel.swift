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

    private let audioCapture: AudioCaptureService
    private var levelPollTimer: Timer?

    /// AppConfig reference — read by resolveSTT/LLM to pick the right provider.
    /// Updated from ContentView when settings change.
    @Published var config: AppConfig = AppConfig()

    // MARK: - Init

    init() {
        audioCapture = AudioCaptureService()
    }

    /// Test initialiser with explicit providers.
    private var _testSTT: (any STTProvider)?
    private var _testLLM: LLMService?
    init(sttProvider: (any STTProvider)?,
         llmService: LLMService? = nil,
         audioCapture: AudioCaptureService = AudioCaptureService()) {
        self._testSTT = sttProvider
        self._testLLM = llmService
        self.audioCapture = audioCapture
    }

    // MARK: - Provider Resolution

    /// Resolve the current STT provider from config. Falls back to Apple Speech.
    var sttProvider: any STTProvider {
        if let test = _testSTT { return test }

        let profileID = config.sttProfile
        if !profileID.isEmpty, let profile = config.modelProfiles.first(where: { $0.id == profileID }) {
            let key = (try? KeychainStore(service: "com.fonos.models").get(profile.id)) ?? ""
            let baseURL = profile.baseURL ?? ""
            log.info("🔌 Resolving STT: provider=\(profile.provider), model=\(profile.modelID), baseURL=\(baseURL)")

            switch profile.provider {
            case "openai":
                let url = baseURL.isEmpty ? "https://api.openai.com" : baseURL
                log.info("🔌 → WhisperSTT(\(url), model=\(profile.modelID))")
                return WhisperSTT(apiKey: key, baseURL: url, modelID: profile.modelID)
            case "fonos":
                let url = URL(string: baseURL.isEmpty ? "http://localhost:9880" : baseURL) ?? URL(string: "http://localhost:9880")!
                log.info("🔌 → FonosSTT(\(url))")
                return FonosSTT(serverURL: url)
            default:
                // For OMLX, Ollama, etc. with STT — use Whisper-compatible endpoint
                if profile.hasSTT && !baseURL.isEmpty {
                    log.info("🔌 → WhisperSTT(\(baseURL), model=\(profile.modelID)) [provider: \(profile.provider)]")
                    return WhisperSTT(apiKey: key, baseURL: baseURL, modelID: profile.modelID)
                }
                log.warning("🔌 → STT profile found but no STT capability or empty URL, falling back to Apple")
            }
        } else {
            log.info("🔌 No STT profile configured, using Apple Speech")
        }
        // Default: Apple on-device Speech Recognition
        return AppleSTT()
    }

    /// Resolve the current LLM service from config. Returns nil if not configured.
    var llmService: LLMService? {
        if let test = _testLLM { return test }

        let profileID = config.llmProfile
        guard !profileID.isEmpty, let profile = config.modelProfiles.first(where: { $0.id == profileID }) else {
            return nil
        }
        let key = (try? KeychainStore(service: "com.fonos.models").get(profile.id)) ?? ""
        let baseURL = profile.baseURL ?? ""
        return LLMService(
            apiKey: key,
            modelID: profile.modelID,
            baseURL: baseURL.isEmpty ? "https://api.openai.com" : baseURL
        )
    }

    /// Human-readable name of the current STT provider for UI display.
    var sttProviderName: String {
        let profileID = config.sttProfile
        if !profileID.isEmpty, let profile = config.modelProfiles.first(where: { $0.id == profileID }) {
            return profile.name
        }
        return "Apple Speech"
    }

    /// Human-readable name of the current LLM provider for UI display.
    var llmProviderName: String? {
        let profileID = config.llmProfile
        guard !profileID.isEmpty, let profile = config.modelProfiles.first(where: { $0.id == profileID }) else {
            return nil
        }
        return profile.name
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
        log.info("🔴 doStartCapture() — starting engine on background thread")

        // Set recording state immediately for responsive UI
        recordingState = .recording
        startLevelPolling()

        // Start audio engine on background thread to avoid blocking main thread
        audioCapture.startCapture { [weak self] error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error {
                    log.error("❌ Engine start failed: \(error.localizedDescription)")
                    self.recordingState = .error(message: error.localizedDescription)
                    self.stopLevelPolling()
                } else {
                    log.info("✅ Engine started successfully on background thread")
                }
            }
        }
    }

    @MainActor
    func stopRecording() {
        log.info("⏹ stopRecording() called")
        guard isRecording else { return }
        stopLevelPolling()

        // Stop engine and get WAV data
        let wavData = audioCapture.stopCapture()

        let dataSize = wavData?.count ?? 0
        let mode = self.currentMode
        log.info("⏹ Got WAV data: \(dataSize) bytes, mode: \(mode.id)")

        if mode.requiresLLM && llmService == nil {
            // Show transcript only — no LLM configured
            log.info("⚠️ Mode requires LLM but none configured, will use raw transcript")
        }

        recordingState = .processing

        Task {
            do {
                log.info("🔄 Starting STT transcription...")
                let result = try await processAudio(
                    wavData ?? Data(),
                    language: nil,
                    mode: mode
                )
                log.info("✅ Processing complete: \(result.text.prefix(50))...")
                await MainActor.run {
                    recordingState = .result(
                        transcript: result.text,
                        processed: result.isRawFallback ? nil : result.text
                    )
                }
            } catch {
                log.error("❌ Processing failed: \(error.localizedDescription)")
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
        return try await sttProvider.transcribe(audioData: audioData, language: language)
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

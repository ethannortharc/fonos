import AVFoundation
import Speech

/// Live speech recognition for keyboard extension.
/// Uses SFSpeechRecognizer with AVAudioEngine — the same pattern
/// used by Gboard and other third-party keyboards for voice input.
/// Does NOT record-then-transcribe — recognizes speech in real-time.
final class KeyboardAudioService: NSObject, @unchecked Sendable {
    private(set) var isRecording = false
    private var audioEngine: AVAudioEngine?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var latestTranscript = ""

    override init() { super.init() }

    /// Start live speech recognition. Transcript updates in real-time.
    func startLiveRecognition(
        language: String?,
        onPartialResult: @escaping @Sendable (String) -> Void,
        completion: @escaping @Sendable (Error?) -> Void
    ) {
        guard !isRecording else { completion(nil); return }

        // Request mic permission (triggers dialog if needed)
        AVAudioSession.sharedInstance().requestRecordPermission { [self] granted in
            DispatchQueue.main.async {
                if granted {
                    self.doStartLiveRecognition(language: language, onPartialResult: onPartialResult, completion: completion)
                } else {
                    completion(NSError(domain: "KB", code: 1, userInfo: [
                        NSLocalizedDescriptionKey: "Mic denied. Settings → Privacy → Microphone → Fonos"
                    ]))
                }
            }
        }
    }

    private func doStartLiveRecognition(
        language: String?,
        onPartialResult: @escaping @Sendable (String) -> Void,
        completion: @escaping @Sendable (Error?) -> Void
    ) {
        // Check speech recognition authorization
        SFSpeechRecognizer.requestAuthorization { [self] status in
            DispatchQueue.main.async {
                guard status == .authorized else {
                    completion(NSError(domain: "KB", code: 2, userInfo: [
                        NSLocalizedDescriptionKey: "Speech recognition denied (status \(status.rawValue))"
                    ]))
                    return
                }
                self.startEngine(language: language, onPartialResult: onPartialResult, completion: completion)
            }
        }
    }

    private func startEngine(
        language: String?,
        onPartialResult: @escaping @Sendable (String) -> Void,
        completion: @escaping @Sendable (Error?) -> Void
    ) {
        let locale = language.map { Locale(identifier: $0) } ?? .current
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
              recognizer.isAvailable else {
            completion(NSError(domain: "KB", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "Speech recognizer unavailable for \(locale.identifier)"
            ]))
            return
        }

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true  // Get results as user speaks
        self.recognitionRequest = request

        let engine = AVAudioEngine()

        // Configure audio session
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            print("🎙 KB: Audio session active for live recognition")
        } catch {
            print("🎙 KB: Session setup failed: \(error), trying without...")
            // Continue anyway — engine might auto-configure
        }

        let inputNode = engine.inputNode
        let recordingFormat = inputNode.outputFormat(forBus: 0)
        print("🎙 KB: Input format: rate=\(recordingFormat.sampleRate), ch=\(recordingFormat.channelCount)")

        guard recordingFormat.channelCount > 0 else {
            completion(NSError(domain: "KB", code: 4, userInfo: [
                NSLocalizedDescriptionKey: "No mic input available (channels=0)"
            ]))
            return
        }

        // Feed audio buffers directly to the speech recognizer
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: recordingFormat) { buffer, _ in
            request.append(buffer)
        }

        // Start recognition task
        latestTranscript = ""
        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            if let result {
                let text = result.bestTranscription.formattedString
                self?.latestTranscript = text
                DispatchQueue.main.async {
                    onPartialResult(text)
                }
            }
            if error != nil || (result?.isFinal ?? false) {
                // Recognition ended
            }
        }

        // Start engine
        engine.prepare()
        do {
            try engine.start()
            audioEngine = engine
            isRecording = true
            print("🎙 KB: ✅ Live recognition started")
            completion(nil)
        } catch {
            inputNode.removeTap(onBus: 0)
            print("🎙 KB: ❌ Engine start failed: \(error.localizedDescription)")
            completion(NSError(domain: "KB", code: 5, userInfo: [
                NSLocalizedDescriptionKey: "Engine: \(error.localizedDescription)"
            ]))
        }
    }

    /// Stop live recognition and return the final transcript.
    func stopLiveRecognition() -> String {
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionTask = nil
        recognitionRequest = nil

        if let engine = audioEngine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            audioEngine = nil
        }

        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        isRecording = false
        let result = latestTranscript
        latestTranscript = ""
        print("🎙 KB: Stopped. Transcript: \(result.prefix(50))...")
        return result
    }
}

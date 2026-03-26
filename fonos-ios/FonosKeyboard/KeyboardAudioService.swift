import AVFoundation
import Speech

/// Dual-track live speech recognition for keyboard extension.
///
/// Strategy: Apple Speech "unlocks" the audio channel and provides instant
/// partial results. Audio buffers are simultaneously accumulated for an
/// optional third-party ASR (Qwen3-ASR, Whisper, etc.) that produces
/// a more accurate final result.
///
/// Track 1 (Apple Speech): instant partial results → shown live
/// Track 2 (Third-party):  accumulated WAV → sent on stop → replaces result
final class KeyboardAudioService: NSObject, @unchecked Sendable {
    private(set) var isRecording = false

    // Engine + Speech
    private var audioEngine: AVAudioEngine?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?

    // Track 1: Apple Speech result
    private var appleTranscript = ""

    // Track 2: Raw audio accumulation for third-party ASR
    private var accumulatedPCM = Data()
    private var captureSampleRate: Double = 16000

    override init() { super.init() }

    // MARK: - Start

    /// Start dual-track live recognition.
    /// - `onPartialResult`: called with Apple Speech partial text (instant)
    /// - `completion`: called when engine starts (nil = success)
    func startLiveRecognition(
        language: String?,
        onPartialResult: @escaping @Sendable (String) -> Void,
        completion: @escaping @Sendable (Error?) -> Void
    ) {
        guard !isRecording else { completion(nil); return }

        // Step 1: Request mic permission
        AVAudioSession.sharedInstance().requestRecordPermission { [self] granted in
            guard granted else {
                completion(makeError("Mic denied. Settings → Privacy → Microphone → Fonos"))
                return
            }
            // Step 2: Request speech recognition permission
            SFSpeechRecognizer.requestAuthorization { status in
                DispatchQueue.main.async {
                    guard status == .authorized else {
                        completion(self.makeError("Speech recognition denied (status \(status.rawValue))"))
                        return
                    }
                    // Step 3: Start engine + recognizer
                    self.startEngine(language: language, onPartialResult: onPartialResult, completion: completion)
                }
            }
        }
    }

    // MARK: - Engine Start

    private func startEngine(
        language: String?,
        onPartialResult: @escaping @Sendable (String) -> Void,
        completion: @escaping @Sendable (Error?) -> Void
    ) {
        let locale = language.map { Locale(identifier: $0) } ?? .current
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
              recognizer.isAvailable else {
            completion(makeError("Speech recognizer unavailable"))
            return
        }

        // Configure audio session
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            print("🎙 KB: Session config failed: \(error), continuing anyway...")
        }

        let engine = AVAudioEngine()
        let inputNode = engine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        captureSampleRate = format.sampleRate
        print("🎙 KB: Input format: rate=\(format.sampleRate), ch=\(format.channelCount)")

        guard format.channelCount > 0 else {
            completion(makeError("No mic input (channels=0)"))
            return
        }

        // Create recognition request
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        self.recognitionRequest = request

        // Reset accumulators
        appleTranscript = ""
        accumulatedPCM = Data()

        // Install tap — dual consumer: Apple Speech + raw buffer accumulation
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
            // Track 1: Feed to Apple Speech recognizer
            request.append(buffer)

            // Track 2: Accumulate raw PCM for third-party ASR
            self?.accumulateBuffer(buffer)
        }

        // Start Apple Speech recognition task (Track 1)
        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            if let result {
                self?.appleTranscript = result.bestTranscription.formattedString
                let text = result.bestTranscription.formattedString
                DispatchQueue.main.async {
                    onPartialResult(text)
                }
            }
            if let error {
                print("🎙 KB: Apple Speech error: \(error.localizedDescription)")
            }
        }

        // Start engine
        engine.prepare()
        do {
            try engine.start()
            audioEngine = engine
            isRecording = true
            print("🎙 KB: ✅ Dual-track recognition started")
            completion(nil)
        } catch {
            inputNode.removeTap(onBus: 0)
            completion(makeError("Engine: \(error.localizedDescription)"))
        }
    }

    // MARK: - Buffer Accumulation (Track 2)

    private func accumulateBuffer(_ buffer: AVAudioPCMBuffer) {
        guard let floatData = buffer.floatChannelData else { return }
        let frameCount = Int(buffer.frameLength)
        // Convert float32 → Int16 PCM
        for i in 0..<frameCount {
            let clamped = max(-1.0, min(1.0, floatData[0][i]))
            var sample = Int16(clamped * Float(Int16.max))
            withUnsafeBytes(of: &sample) { accumulatedPCM.append(contentsOf: $0) }
        }
    }

    // MARK: - Stop

    /// Stop recognition. Returns Apple Speech transcript and WAV data for third-party ASR.
    struct StopResult {
        let appleTranscript: String   // Track 1: instant result from Apple Speech
        let wavData: Data             // Track 2: raw audio for third-party ASR
        let sampleRate: Double        // Sample rate of the WAV data
    }

    func stopLiveRecognition() -> StopResult {
        // End recognition
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionTask = nil
        recognitionRequest = nil

        // Stop engine
        if let engine = audioEngine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            audioEngine = nil
        }

        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        isRecording = false

        // Build WAV from accumulated PCM
        let wavData = buildWAV(pcmData: accumulatedPCM, sampleRate: Int(captureSampleRate))
        let result = StopResult(
            appleTranscript: appleTranscript,
            wavData: wavData,
            sampleRate: captureSampleRate
        )

        print("🎙 KB: Stopped. Apple: \"\(appleTranscript.prefix(30))...\", WAV: \(wavData.count) bytes")

        appleTranscript = ""
        accumulatedPCM = Data()

        return result
    }

    // MARK: - WAV Builder

    private func buildWAV(pcmData: Data, sampleRate: Int) -> Data {
        let dataSize = UInt32(pcmData.count)
        let chunkSize = 36 + dataSize
        var wav = Data(capacity: 44 + Int(dataSize))

        wav.append(contentsOf: "RIFF".utf8)
        appendUInt32LE(&wav, chunkSize)
        wav.append(contentsOf: "WAVE".utf8)
        wav.append(contentsOf: "fmt ".utf8)
        appendUInt32LE(&wav, 16)
        appendUInt16LE(&wav, 1)            // PCM
        appendUInt16LE(&wav, 1)            // mono
        appendUInt32LE(&wav, UInt32(sampleRate))
        appendUInt32LE(&wav, UInt32(sampleRate * 2))  // byte rate
        appendUInt16LE(&wav, 2)            // block align
        appendUInt16LE(&wav, 16)           // bits per sample
        wav.append(contentsOf: "data".utf8)
        appendUInt32LE(&wav, dataSize)
        wav.append(pcmData)

        return wav
    }

    private func appendUInt16LE(_ data: inout Data, _ value: UInt16) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { data.append(contentsOf: $0) }
    }

    private func appendUInt32LE(_ data: inout Data, _ value: UInt32) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { data.append(contentsOf: $0) }
    }

    private func makeError(_ msg: String) -> NSError {
        print("🎙 KB: ❌ \(msg)")
        return NSError(domain: "KB", code: 1, userInfo: [NSLocalizedDescriptionKey: msg])
    }
}

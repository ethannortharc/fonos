import AVFoundation

/// Audio capture for keyboard extension.
/// Tries multiple approaches since keyboard extensions have sandbox restrictions.
final class KeyboardAudioService: NSObject, @unchecked Sendable, AVCaptureAudioDataOutputSampleBufferDelegate {
    private(set) var isRecording = false
    private var capturedData = Data()
    private var engine: AVAudioEngine?
    private var recorder: AVAudioRecorder?
    private var captureSession: AVCaptureSession?
    private let captureQueue = DispatchQueue(label: "com.fonos.kb.capture")

    struct CaptureResult {
        let fileURL: URL
        let wavData: Data
    }

    private var recordingURL: URL {
        URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("fonos_kb.wav")
    }

    override init() { super.init() }

    func startCapture(completion: @escaping @Sendable (Error?) -> Void) {
        guard !isRecording else { completion(nil); return }

        // MUST explicitly request mic permission — keyboard extension has its own
        // permission separate from the main app. This triggers the system dialog.
        let svc = self
        AVAudioSession.sharedInstance().requestRecordPermission { granted in
            DispatchQueue.main.async {
                if granted {
                    svc.startCaptureAfterPermission(completion: completion)
                } else {
                    completion(svc.makeError("Mic denied. Go to Settings → Privacy → Microphone → enable Fonos."))
                }
            }
        }
    }

    private func startCaptureAfterPermission(completion: @escaping @Sendable (Error?) -> Void) {
        // Try approaches in order of reliability
        // Approach 1: AVAudioEngine with NO manual session config (let engine handle it)
        if tryAudioEngine() {
            print("🎙 KB: ✅ AVAudioEngine started (approach 1)")
            isRecording = true
            completion(nil)
            return
        }

        // Approach 2: AVAudioEngine WITH session config
        if tryAudioEngineWithSession() {
            print("🎙 KB: ✅ AVAudioEngine+session started (approach 2)")
            isRecording = true
            completion(nil)
            return
        }

        // Approach 3: AVCaptureSession (camera/capture framework — different sandbox path)
        if tryCaptureSession() {
            print("🎙 KB: ✅ AVCaptureSession started (approach 3)")
            isRecording = true
            completion(nil)
            return
        }

        // Approach 4: AVAudioRecorder
        if tryRecorder() {
            print("🎙 KB: ✅ AVAudioRecorder started (approach 4)")
            isRecording = true
            completion(nil)
            return
        }

        completion(makeError("All recording methods failed. Check mic permission + Full Access."))
    }

    // MARK: - Approach 1: AVAudioEngine, no session config

    private func tryAudioEngine() -> Bool {
        let eng = AVAudioEngine()
        let inputNode = eng.inputNode
        let format = inputNode.outputFormat(forBus: 0)

        guard format.channelCount > 0, format.sampleRate > 0 else {
            print("🎙 KB: Engine approach 1 — invalid format: ch=\(format.channelCount), rate=\(format.sampleRate)")
            return false
        }

        capturedData = Data()

        // Install tap to capture raw audio
        inputNode.installTap(onBus: 0, bufferSize: 4096, format: nil) { [weak self] buffer, _ in
            self?.appendBuffer(buffer)
        }

        eng.prepare()
        do {
            try eng.start()
            engine = eng
            return true
        } catch {
            print("🎙 KB: Engine approach 1 failed: \(error.localizedDescription)")
            inputNode.removeTap(onBus: 0)
            return false
        }
    }

    // MARK: - Approach 2: AVAudioEngine with session

    private func tryAudioEngineWithSession() -> Bool {
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            print("🎙 KB: Engine approach 2 — session setup failed: \(error.localizedDescription)")
            return false
        }

        let eng = AVAudioEngine()
        let inputNode = eng.inputNode
        capturedData = Data()

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: nil) { [weak self] buffer, _ in
            self?.appendBuffer(buffer)
        }

        eng.prepare()
        do {
            try eng.start()
            engine = eng
            return true
        } catch {
            print("🎙 KB: Engine approach 2 failed: \(error.localizedDescription)")
            inputNode.removeTap(onBus: 0)
            return false
        }
    }

    // MARK: - Approach 3: AVCaptureSession

    private func tryCaptureSession() -> Bool {
        let session = AVCaptureSession()

        guard let device = AVCaptureDevice.default(for: .audio) else {
            print("🎙 KB: CaptureSession — no audio device")
            return false
        }

        do {
            let input = try AVCaptureDeviceInput(device: device)
            guard session.canAddInput(input) else {
                print("🎙 KB: CaptureSession — can't add input")
                return false
            }
            session.addInput(input)
        } catch {
            print("🎙 KB: CaptureSession — input error: \(error.localizedDescription)")
            return false
        }

        let output = AVCaptureAudioDataOutput()
        guard session.canAddOutput(output) else {
            print("🎙 KB: CaptureSession — can't add output")
            return false
        }
        output.setSampleBufferDelegate(self, queue: captureQueue)
        session.addOutput(output)

        capturedData = Data()
        session.startRunning()

        if session.isRunning {
            captureSession = session
            return true
        } else {
            print("🎙 KB: CaptureSession — startRunning failed (not running)")
            return false
        }
    }

    // AVCaptureAudioDataOutputSampleBufferDelegate
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let blockBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }
        let length = CMBlockBufferGetDataLength(blockBuffer)
        var data = Data(count: length)
        data.withUnsafeMutableBytes { ptr in
            if let baseAddr = ptr.baseAddress {
                CMBlockBufferCopyDataBytes(blockBuffer, atOffset: 0, dataLength: length, destination: baseAddr)
            }
        }
        capturedData.append(data)
    }

    // MARK: - Approach 4: AVAudioRecorder

    private func tryRecorder() -> Bool {
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers, .defaultToSpeaker])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            print("🎙 KB: Recorder — session setup failed: \(error.localizedDescription)")
            return false
        }

        let url = recordingURL
        try? FileManager.default.removeItem(at: url)

        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatLinearPCM),
            AVSampleRateKey: 16000.0,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]

        do {
            let rec = try AVAudioRecorder(url: url, settings: settings)
            rec.delegate = self
            if rec.record(forDuration: 120) {
                recorder = rec
                return true
            }
            print("🎙 KB: Recorder — record() returned false")
            return false
        } catch {
            print("🎙 KB: Recorder — init failed: \(error.localizedDescription)")
            return false
        }
    }

    // MARK: - Buffer Capture (for AVAudioEngine approaches)

    private func appendBuffer(_ buffer: AVAudioPCMBuffer) {
        // Convert to Int16 PCM data
        guard let floatData = buffer.floatChannelData else { return }
        let frameCount = Int(buffer.frameLength)
        var int16Data = Data(capacity: frameCount * 2)
        for i in 0..<frameCount {
            let clamped = max(-1.0, min(1.0, floatData[0][i]))
            var sample = Int16(clamped * Float(Int16.max))
            int16Data.append(Data(bytes: &sample, count: 2))
        }
        capturedData.append(int16Data)
    }

    // MARK: - Stop

    @discardableResult
    func stopCapture() -> CaptureResult? {
        guard isRecording else { return nil }
        isRecording = false

        // Stop capture session if used
        if let cs = captureSession {
            cs.stopRunning()
            captureSession = nil

            guard capturedData.count > 100 else {
                print("🎙 KB: ❌ CaptureSession data too small: \(capturedData.count)")
                return nil
            }

            // CaptureSession provides raw PCM — build WAV
            let wavData = buildWAV(pcmData: capturedData, sampleRate: 16000)
            let url = recordingURL
            try? wavData.write(to: url)
            print("🎙 KB: ✅ CaptureSession: \(capturedData.count) → \(wavData.count) WAV bytes")
            capturedData = Data()
            return CaptureResult(fileURL: url, wavData: wavData)
        }

        // Stop engine if used
        if let eng = engine {
            eng.inputNode.removeTap(onBus: 0)
            eng.stop()
            engine = nil

            // Build WAV from captured PCM data
            guard capturedData.count > 100 else {
                print("🎙 KB: ❌ Captured data too small: \(capturedData.count) bytes")
                return nil
            }

            let wavData = buildWAV(pcmData: capturedData, sampleRate: 16000)
            let url = recordingURL
            try? wavData.write(to: url)
            print("🎙 KB: ✅ Engine capture: \(capturedData.count) PCM bytes → \(wavData.count) WAV bytes")
            capturedData = Data()

            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            return CaptureResult(fileURL: url, wavData: wavData)
        }

        // Stop recorder if used
        if let rec = recorder {
            rec.stop()
            recorder = nil

            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

            guard let data = try? Data(contentsOf: recordingURL), data.count > 100 else {
                print("🎙 KB: ❌ Recorder file too small")
                return nil
            }

            print("🎙 KB: ✅ Recorder capture: \(data.count) bytes")
            return CaptureResult(fileURL: recordingURL, wavData: data)
        }

        return nil
    }

    // MARK: - WAV Builder

    private func buildWAV(pcmData: Data, sampleRate: Int) -> Data {
        let dataSize = UInt32(pcmData.count)
        let chunkSize = 36 + dataSize
        var wav = Data(capacity: 44 + Int(dataSize))

        // RIFF header
        wav.append(contentsOf: "RIFF".utf8)
        wav.append(uint32LE: chunkSize)
        wav.append(contentsOf: "WAVE".utf8)

        // fmt chunk
        wav.append(contentsOf: "fmt ".utf8)
        wav.append(uint32LE: 16)        // chunk size
        wav.append(uint16LE: 1)         // PCM format
        wav.append(uint16LE: 1)         // mono
        wav.append(uint32LE: UInt32(sampleRate))
        wav.append(uint32LE: UInt32(sampleRate * 2)) // byte rate
        wav.append(uint16LE: 2)         // block align
        wav.append(uint16LE: 16)        // bits per sample

        // data chunk
        wav.append(contentsOf: "data".utf8)
        wav.append(uint32LE: dataSize)
        wav.append(pcmData)

        return wav
    }

    private func makeError(_ msg: String) -> NSError {
        print("🎙 KB: ❌ \(msg)")
        return NSError(domain: "KeyboardAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: msg])
    }
}

extension KeyboardAudioService: AVAudioRecorderDelegate {
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        if !flag { isRecording = false }
    }
}

private extension Data {
    mutating func append(uint16LE value: UInt16) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { append(contentsOf: $0) }
    }
    mutating func append(uint32LE value: UInt32) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { append(contentsOf: $0) }
    }
}

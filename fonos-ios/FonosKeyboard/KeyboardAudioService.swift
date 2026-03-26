import AVFoundation

/// Lightweight audio capture for keyboard extension using AVAudioRecorder.
final class KeyboardAudioService: NSObject, @unchecked Sendable {
    private(set) var isRecording = false
    private var recorder: AVAudioRecorder?

    private var recordingURL: URL {
        // Use NSTemporaryDirectory explicitly (extension sandbox compatible)
        let dir = NSTemporaryDirectory()
        return URL(fileURLWithPath: dir).appendingPathComponent("fonos_kb.m4a")
    }

    override init() {
        super.init()
    }

    struct CaptureResult {
        let fileURL: URL
        let wavData: Data
    }

    func startCapture(completion: @escaping (Error?) -> Void) {
        guard !isRecording else { completion(nil); return }

        // Check mic permission
        let permission = AVAudioSession.sharedInstance().recordPermission
        guard permission == .granted else {
            completion(makeError("Mic not authorized. Enable Full Access in Settings → Keyboards → Fonos."))
            return
        }

        // Configure audio session
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers, .defaultToSpeaker])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            print("🎙 KB: Audio session active, sampleRate=\(session.sampleRate), inputAvailable=\(session.isInputAvailable)")
        } catch {
            completion(makeError("Audio session: \(error.localizedDescription)"))
            return
        }

        // Remove old recording
        let url = recordingURL
        try? FileManager.default.removeItem(at: url)
        print("🎙 KB: Recording URL: \(url.path)")

        // Try AAC format first (more compatible), fallback to PCM
        let aacSettings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
            AVSampleRateKey: session.sampleRate,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ]

        do {
            recorder = try AVAudioRecorder(url: url, settings: aacSettings)
        } catch {
            print("🎙 KB: AAC recorder init failed: \(error.localizedDescription)")
            // Fallback: try PCM
            let pcmSettings: [String: Any] = [
                AVFormatIDKey: Int(kAudioFormatLinearPCM),
                AVSampleRateKey: session.sampleRate,
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false,
            ]
            do {
                let pcmURL = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("fonos_kb.wav")
                try? FileManager.default.removeItem(at: pcmURL)
                recorder = try AVAudioRecorder(url: pcmURL, settings: pcmSettings)
            } catch {
                completion(makeError("Recorder init: \(error.localizedDescription)"))
                return
            }
        }

        guard let recorder else {
            completion(makeError("Recorder is nil after init"))
            return
        }

        recorder.delegate = self
        recorder.isMeteringEnabled = true

        let prepared = recorder.prepareToRecord()
        print("🎙 KB: prepareToRecord=\(prepared)")
        guard prepared else {
            completion(makeError("prepareToRecord() failed"))
            return
        }

        let started = recorder.record()
        print("🎙 KB: record()=\(started), duration=\(recorder.currentTime)")
        guard started else {
            completion(makeError("record() returned false. Input available: \(session.isInputAvailable), category: \(session.category.rawValue)"))
            return
        }

        isRecording = true
        completion(nil)
    }

    @discardableResult
    func stopCapture() -> CaptureResult? {
        guard isRecording else { return nil }

        recorder?.updateMeters()
        let avgPower = recorder?.averagePower(forChannel: 0) ?? -160
        let peakPower = recorder?.peakPower(forChannel: 0) ?? -160
        let duration = recorder?.currentTime ?? 0
        print("🎙 KB: Stop — avg=\(avgPower)dB, peak=\(peakPower)dB, duration=\(String(format: "%.1f", duration))s")

        recorder?.stop()
        isRecording = false

        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        guard let url = recorder?.url,
              FileManager.default.fileExists(atPath: url.path),
              let data = try? Data(contentsOf: url),
              data.count > 100 else {
            print("🎙 KB: ❌ No recording file or too small")
            return nil
        }

        print("🎙 KB: ✅ File: \(data.count) bytes at \(url.lastPathComponent)")
        return CaptureResult(fileURL: url, wavData: data)
    }

    private func makeError(_ message: String) -> NSError {
        print("🎙 KB: ❌ \(message)")
        return NSError(domain: "KeyboardAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

extension KeyboardAudioService: AVAudioRecorderDelegate {
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        print("🎙 KB: Recorder finished, success=\(flag)")
        if !flag { isRecording = false }
    }

    func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
        print("🎙 KB: Encode error: \(error?.localizedDescription ?? "unknown")")
        isRecording = false
    }
}

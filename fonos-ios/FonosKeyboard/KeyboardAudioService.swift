import AVFoundation

/// Lightweight audio capture for keyboard extension using AVAudioRecorder.
/// AVAudioEngine doesn't work reliably in keyboard extensions due to sandbox
/// restrictions. AVAudioRecorder is a higher-level API that handles the
/// audio session more gracefully in constrained environments.
final class KeyboardAudioService: NSObject, @unchecked Sendable {
    private(set) var isRecording = false
    private var recorder: AVAudioRecorder?
    private let recordingURL: URL

    override init() {
        // Record to a temp file in the extension's container
        let tempDir = FileManager.default.temporaryDirectory
        recordingURL = tempDir.appendingPathComponent("fonos_kb_recording.wav")
        super.init()
    }

    /// Start recording. Completion called with nil on success, error on failure.
    func startCapture(completion: @escaping (Error?) -> Void) {
        guard !isRecording else { completion(nil); return }

        // Check mic permission
        let permission = AVAudioSession.sharedInstance().recordPermission
        guard permission == .granted else {
            completion(makeError("Mic not authorized (status \(permission.rawValue)). Enable Full Access in Settings → Keyboards → Fonos."))
            return
        }

        // Configure audio session — use .record (simplest, mic-only)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .default)
            try session.setActive(true)
        } catch {
            completion(makeError("Audio session: \(error.localizedDescription)"))
            return
        }

        // Record at device's native sample rate for best compatibility
        // (SFSpeechRecognizer handles resampling internally)
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatLinearPCM),
            AVSampleRateKey: AVAudioSession.sharedInstance().sampleRate,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]

        do {
            // Remove old recording
            try? FileManager.default.removeItem(at: recordingURL)

            recorder = try AVAudioRecorder(url: recordingURL, settings: settings)
            recorder?.delegate = self
            recorder?.isMeteringEnabled = true  // enable level metering to verify capture
            guard let recorder, recorder.prepareToRecord() else {
                completion(makeError("Failed to prepare recorder"))
                return
            }
            let started = recorder.record()
            if !started {
                completion(makeError("Recorder.record() returned false"))
                return
            }
            isRecording = true
            completion(nil)
        } catch {
            completion(makeError("Recorder init: \(error.localizedDescription)"))
        }
    }

    /// Stop recording and return the file URL (for SFSpeechURLAudioRequest)
    /// and the WAV data (for Whisper API upload).
    struct CaptureResult {
        let fileURL: URL
        let wavData: Data
    }

    @discardableResult
    func stopCapture() -> CaptureResult? {
        guard isRecording else { return nil }

        // Check audio levels before stopping
        recorder?.updateMeters()
        let avgPower = recorder?.averagePower(forChannel: 0) ?? -160
        let peakPower = recorder?.peakPower(forChannel: 0) ?? -160
        print("🎙 KB Audio levels: avg=\(avgPower)dB, peak=\(peakPower)dB")

        recorder?.stop()
        isRecording = false

        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        guard FileManager.default.fileExists(atPath: recordingURL.path) else {
            print("🎙 ❌ Recording file not found at \(recordingURL.path)")
            return nil
        }

        guard let data = try? Data(contentsOf: recordingURL), data.count > 100 else {
            print("🎙 ❌ Recording file too small or unreadable")
            return nil
        }

        print("🎙 ✅ Recording file: \(data.count) bytes, sampleRate=\(AVAudioSession.sharedInstance().sampleRate)")
        return CaptureResult(fileURL: recordingURL, wavData: data)
    }

    private func makeError(_ message: String) -> NSError {
        NSError(domain: "KeyboardAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

extension KeyboardAudioService: AVAudioRecorderDelegate {
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        if !flag {
            isRecording = false
        }
    }

    func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
        isRecording = false
    }
}

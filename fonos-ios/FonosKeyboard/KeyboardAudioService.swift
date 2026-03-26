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

        // Configure audio session for keyboard extension
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .mixWithOthers])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            completion(makeError("Audio session: \(error.localizedDescription)"))
            return
        }

        // Set up recorder with 16kHz mono PCM
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatLinearPCM),
            AVSampleRateKey: 16000.0,
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
            guard let recorder, recorder.prepareToRecord() else {
                completion(makeError("Failed to prepare recorder"))
                return
            }
            recorder.record()
            isRecording = true
            completion(nil)
        } catch {
            completion(makeError("Recorder init: \(error.localizedDescription)"))
        }
    }

    /// Stop recording and return the WAV data.
    @discardableResult
    func stopCapture() -> Data? {
        guard isRecording else { return nil }
        recorder?.stop()
        isRecording = false

        // Deactivate audio session
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        // Read recorded file
        guard FileManager.default.fileExists(atPath: recordingURL.path) else { return nil }
        return try? Data(contentsOf: recordingURL)
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

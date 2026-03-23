import Foundation
import AVFoundation

/// Records audio using AVAudioEngine and produces 16kHz 16-bit mono PCM buffers.
/// Scaffold — implementation goes in wp-executor.
@MainActor
final class AudioCaptureService: ObservableObject {
    @Published var isRecording = false

    private let engine = AVAudioEngine()

    func startCapture() throws {
        isRecording = true
    }

    func stopCapture() -> Data? {
        isRecording = false
        return nil
    }
}

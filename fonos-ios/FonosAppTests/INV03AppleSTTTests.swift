// INV-03: AppleSTT returns non-empty transcript from audio.
// Handles permission denied gracefully (returns error, no crash).
//
// Verifier: auto
// Level: unit (SFSpeechRecognizer mocked via protocol seam)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV03AppleSTTTests

import Testing
import Speech
@testable import FonosApp

// MARK: - Mock types

/// Protocol seam that AudioCaptureService uses to interact with SFSpeechRecognizer.
/// The real implementation wraps SFSpeechRecognizer; tests inject a mock.
private final class MockSpeechRecognizer: SpeechRecognizerProtocol, @unchecked Sendable {
    var stubbedAuthStatus: SFSpeechRecognizerAuthorizationStatus = .authorized
    var stubbedTranscript: String? = "hello world"
    var stubbedError: Error?

    func requestAuthorization(_ handler: @escaping (SFSpeechRecognizerAuthorizationStatus) -> Void) {
        handler(stubbedAuthStatus)
    }

    func recognize(request: SFSpeechAudioBufferRecognitionRequest,
                   resultHandler: @escaping (SFSpeechRecognitionResult?, Error?) -> Void) -> SFSpeechRecognitionTask {
        if let error = stubbedError {
            resultHandler(nil, error)
        } else if let transcript = stubbedTranscript {
            let mockResult = MockSpeechRecognitionResult(bestTranscription: transcript, isFinal: true)
            resultHandler(mockResult as? SFSpeechRecognitionResult, nil)
        } else {
            // No results, no error
            resultHandler(nil, nil)
        }
        return SFSpeechRecognitionTask()
    }
}

/// Minimal stand-in for SFSpeechRecognitionResult.
private final class MockSpeechRecognitionResult: NSObject {
    let transcriptString: String
    let isFinal: Bool

    init(bestTranscription: String, isFinal: Bool) {
        self.transcriptString = bestTranscription
        self.isFinal = isFinal
    }
}

// MARK: - Tests

struct INV03AppleSTTTests {

    // --- Protocol conformance ---

    @Test("AppleSTT conforms to STTProvider protocol")
    func protocolConformance() throws {
        // Compile-time assertion: if AppleSTT doesn't conform, this cast fails to compile.
        let stt: any STTProvider = AppleSTT()
        _ = stt
        #expect(Bool(true))
    }

    // --- Happy path ---

    @Test("AppleSTT returns non-empty transcript from mocked recognizer")
    func returnsTranscript() async throws {
        let mock = MockSpeechRecognizer()
        mock.stubbedTranscript = "hello world"
        let stt = AppleSTT(recognizer: mock)
        let dummyBuffer = makeDummyBuffer()
        let result = try await stt.transcribe(buffer: dummyBuffer, language: "en-US")
        #expect(!result.isEmpty)
        #expect(result == "hello world")
    }

    // --- Permission denied ---

    @Test("AppleSTT throws descriptive error when authorization is denied")
    func throwsOnAuthDenied() async throws {
        let mock = MockSpeechRecognizer()
        mock.stubbedAuthStatus = .denied
        let stt = AppleSTT(recognizer: mock)
        let dummyBuffer = makeDummyBuffer()
        await #expect(throws: STTError.permissionDenied) {
            _ = try await stt.transcribe(buffer: dummyBuffer, language: "en-US")
        }
    }

    // --- No results ---

    @Test("AppleSTT throws appropriate error when recognizer returns no results")
    func throwsOnNoResults() async throws {
        let mock = MockSpeechRecognizer()
        mock.stubbedTranscript = nil
        mock.stubbedError = nil
        let stt = AppleSTT(recognizer: mock)
        let dummyBuffer = makeDummyBuffer()
        await #expect(throws: STTError.noTranscript) {
            _ = try await stt.transcribe(buffer: dummyBuffer, language: "en-US")
        }
    }

    // --- Language locale ---

    @Test("AppleSTT passes language parameter as locale to recognition request")
    func languageParameterSetsLocale() async throws {
        let mock = MockSpeechRecognizer()
        mock.stubbedTranscript = "bonjour"
        let stt = AppleSTT(recognizer: mock)
        let dummyBuffer = makeDummyBuffer()
        // "fr-FR" must be forwarded to the recognition request locale
        let result = try await stt.transcribe(buffer: dummyBuffer, language: "fr-FR")
        #expect(!result.isEmpty)
        #expect(stt.lastUsedLocale?.identifier == "fr-FR")
    }

    // --- Recognizer error propagation ---

    @Test("AppleSTT propagates recognizer error as STTError")
    func propagatesRecognizerError() async throws {
        let mock = MockSpeechRecognizer()
        mock.stubbedError = NSError(domain: "com.apple.speech", code: -1)
        let stt = AppleSTT(recognizer: mock)
        let dummyBuffer = makeDummyBuffer()
        await #expect(throws: STTError.self) {
            _ = try await stt.transcribe(buffer: dummyBuffer, language: "en-US")
        }
    }
}

// MARK: - Helpers

private func makeDummyBuffer() -> Data {
    // Minimal 44-byte WAV header with no audio payload — sufficient for mock injection
    var header = [UInt8](repeating: 0, count: 44)
    "RIFF".utf8.enumerated().forEach { header[$0.offset] = $0.element }
    "WAVE".utf8.enumerated().forEach { header[8 + $0.offset] = $0.element }
    "fmt ".utf8.enumerated().forEach { header[12 + $0.offset] = $0.element }
    "data".utf8.enumerated().forEach { header[36 + $0.offset] = $0.element }
    return Data(header)
}

// QD-04: Error handling robustness.
// All error paths return descriptive errors, no crashes, graceful fallbacks.
//
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/QD04ErrorHandlingTests

import Testing
import Foundation
import Speech
@testable import FonosApp

// MARK: - Mock URLProtocols for specific error scenarios

private final class TimeoutMockURLProtocol: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        client?.urlProtocol(self, didFailWithError: URLError(.timedOut))
    }
    override func stopLoading() {}
}

private final class InvalidKeyMockURLProtocol: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let url = URL(string: "https://api.openai.com")!
        let resp = HTTPURLResponse(url: url, statusCode: 401, httpVersion: nil, headerFields: nil)!
        let body = #"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#
            .data(using: .utf8)!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: body)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

private final class MalformedResponseMockURLProtocol: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        let url = URL(string: "https://api.openai.com")!
        let resp = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)!
        let body = "INVALID_JSON{{{".data(using: .utf8)!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: body)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

private func sessionWith<T: URLProtocol>(_ type: T.Type) -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [type]
    return URLSession(configuration: config)
}

private func dummyAudio() -> Data { Data(count: 44) }

// MARK: - Tests

struct QD04ErrorHandlingTests {

    // MARK: - Network timeout

    @Test("WhisperSTT network timeout throws descriptive STTError.timeout (not crash)")
    func whisperSTTTimeoutDescriptive() async throws {
        let stt = WhisperSTT(session: sessionWith(TimeoutMockURLProtocol.self), apiKey: "sk-test")
        do {
            _ = try await stt.transcribe(audioData: dummyAudio(), language: "en")
            Issue.record("Expected timeout error")
        } catch let error as STTError {
            // Must be timeout-specific, not generic
            if case .timeout = error { /* expected */ }
            else {
                Issue.record("Expected STTError.timeout, got \(error)")
            }
            #expect(!error.localizedDescription.isEmpty)
        }
    }

    @Test("LLMService network timeout throws descriptive LLMError (not crash)")
    func llmServiceTimeoutDescriptive() async throws {
        let service = LLMService(session: sessionWith(TimeoutMockURLProtocol.self), apiKey: "sk-test")
        do {
            _ = try await service.process(text: "hello", mode: .polish)
            Issue.record("Expected timeout error")
        } catch let error as LLMError {
            #expect(!error.localizedDescription.isEmpty)
        } catch {
            Issue.record("Unexpected error type: \(type(of: error))")
        }
    }

    // MARK: - Invalid API key

    @Test("WhisperSTT 401 error throws STTError.authenticationFailed with clear message")
    func whisperSTTInvalidKeyError() async throws {
        let stt = WhisperSTT(session: sessionWith(InvalidKeyMockURLProtocol.self), apiKey: "sk-bad")
        do {
            _ = try await stt.transcribe(audioData: dummyAudio(), language: "en")
            Issue.record("Expected authentication error")
        } catch let error as STTError {
            if case .authenticationFailed = error { /* expected */ }
            else { Issue.record("Expected STTError.authenticationFailed, got \(error)") }
            #expect(!error.localizedDescription.isEmpty)
        }
    }

    @Test("LLMService 401 error throws LLMError.authenticationFailed with clear message")
    func llmServiceInvalidKeyError() async throws {
        let service = LLMService(session: sessionWith(InvalidKeyMockURLProtocol.self), apiKey: "sk-bad")
        do {
            _ = try await service.process(text: "hello", mode: .polish)
            Issue.record("Expected authentication error")
        } catch let error as LLMError {
            #expect(!error.localizedDescription.isEmpty)
            // Description must hint at the cause
            let desc = error.localizedDescription.lowercased()
            #expect(desc.contains("key") || desc.contains("auth") || desc.contains("invalid"))
        }
    }

    // MARK: - Malformed response

    @Test("WhisperSTT malformed JSON response throws STTError.parseError (not crash)")
    func whisperSTTMalformedResponseNoCrash() async throws {
        let stt = WhisperSTT(session: sessionWith(MalformedResponseMockURLProtocol.self), apiKey: "sk-test")
        do {
            _ = try await stt.transcribe(audioData: dummyAudio(), language: "en")
            Issue.record("Expected parse error")
        } catch let error as STTError {
            if case .parseError = error { /* expected */ }
            else { Issue.record("Expected STTError.parseError, got \(error)") }
        } catch {
            Issue.record("Unexpected error type: \(type(of: error))")
        }
    }

    @Test("LLMService malformed JSON response throws LLMError.parseError (not crash)")
    func llmServiceMalformedResponseNoCrash() async throws {
        let service = LLMService(session: sessionWith(MalformedResponseMockURLProtocol.self), apiKey: "sk-test")
        do {
            _ = try await service.process(text: "hello", mode: .polish)
            Issue.record("Expected parse error")
        } catch let error as LLMError {
            if case .parseError = error { /* expected */ }
            else { Issue.record("Expected LLMError.parseError, got \(error)") }
        } catch {
            Issue.record("Unexpected error type: \(type(of: error))")
        }
    }

    // MARK: - Permission denied (AppleSTT)

    @Test("AppleSTT permission denied throws STTError.permissionDenied with user-friendly message")
    func appleSTTPermissionDenied() async throws {
        let mock = PermissionDeniedMockRecognizer()
        let stt = AppleSTT(recognizer: mock)
        do {
            _ = try await stt.transcribe(buffer: dummyAudio(), language: "en-US")
            Issue.record("Expected permission denied error")
        } catch let error as STTError {
            if case .permissionDenied = error { /* expected */ }
            else { Issue.record("Expected STTError.permissionDenied, got \(error)") }
            // Must provide actionable guidance
            let desc = error.localizedDescription.lowercased()
            #expect(desc.contains("permiss") || desc.contains("microphone") || desc.contains("speech"))
        }
    }

    // MARK: - All error types have non-empty localizedDescription

    @Test("All STTError cases have non-empty localizedDescription")
    func sttErrorLocalisedDescriptions() {
        let errors: [STTError] = [
            .permissionDenied,
            .noTranscript,
            .authenticationFailed,
            .badRequest,
            .timeout,
            .networkUnavailable,
            .parseError,
        ]
        for error in errors {
            #expect(!error.localizedDescription.isEmpty, "STTError.\(error) has empty description")
        }
    }

    @Test("All LLMError cases have non-empty localizedDescription")
    func llmErrorLocalisedDescriptions() {
        let errors: [LLMError] = [
            .authenticationFailed,
            .networkUnavailable,
            .timeout,
            .parseError,
            .serverError(statusCode: 500),
        ]
        for error in errors {
            #expect(!error.localizedDescription.isEmpty, "LLMError.\(error) has empty description")
        }
    }
}

// MARK: - Mock helpers

private final class PermissionDeniedMockRecognizer: SpeechRecognizerProtocol, @unchecked Sendable {
    var stubbedTranscript: String? { nil }

    func requestAuthorization(_ handler: @escaping (SFSpeechRecognizerAuthorizationStatus) -> Void) {
        handler(.denied)
    }

    func recognize(request: SFSpeechAudioBufferRecognitionRequest,
                   resultHandler: @escaping (SFSpeechRecognitionResult?, Error?) -> Void) -> SFSpeechRecognitionTask {
        resultHandler(nil, nil)
        return SFSpeechRecognitionTask()
    }
}

// INV-04: WhisperSTT POSTs WAV to OpenAI endpoint, parses response, returns transcript.
// Handles 401, 400, timeout errors gracefully.
//
// Verifier: auto
// Level: unit (URLSession intercepted via URLProtocol subclass)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV04WhisperSTTTests

import Testing
import Foundation
@testable import FonosApp

// MARK: - Mock URLProtocol

private final class MockURLProtocol: URLProtocol {
    nonisolated(unsafe) static var requestHandler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let handler = MockURLProtocol.requestHandler else {
            client?.urlProtocol(self, didFailWithError: URLError(.unknown))
            return
        }
        do {
            // URLSession moves httpBody to httpBodyStream for URLProtocol interceptors.
            // Reconstruct httpBody so handler closures can read req.httpBody directly.
            var enrichedRequest = request
            if enrichedRequest.httpBody == nil, let stream = enrichedRequest.httpBodyStream {
                stream.open()
                var bodyData = Data()
                let bufferSize = 4096
                let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: bufferSize)
                defer { buffer.deallocate() }
                while stream.hasBytesAvailable {
                    let read = stream.read(buffer, maxLength: bufferSize)
                    if read > 0 { bodyData.append(buffer, count: read) }
                }
                stream.close()
                enrichedRequest.httpBody = bodyData
            }
            let (response, data) = try handler(enrichedRequest)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

// MARK: - Helpers

private func makeSession() -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [MockURLProtocol.self]
    return URLSession(configuration: config)
}

private func makeWhisperSTT(session: URLSession, apiKey: String = "sk-test") -> WhisperSTT {
    WhisperSTT(session: session, apiKey: apiKey)
}

private func okResponse(for url: URL, body: String) -> (HTTPURLResponse, Data) {
    let resp = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)!
    return (resp, body.data(using: .utf8)!)
}

private func errorResponse(for url: URL, statusCode: Int) -> (HTTPURLResponse, Data) {
    let resp = HTTPURLResponse(url: url, statusCode: statusCode, httpVersion: nil, headerFields: nil)!
    return (resp, Data())
}

private let kOpenAITranscriptURL = URL(string: "https://api.openai.com/v1/audio/transcriptions")!

private func dummyWAVData() -> Data {
    var header = [UInt8](repeating: 0, count: 44)
    "RIFF".utf8.enumerated().forEach { header[$0.offset] = $0.element }
    "WAVE".utf8.enumerated().forEach { header[8 + $0.offset] = $0.element }
    return Data(header)
}

// MARK: - Tests

// Serialized because MockURLProtocol.requestHandler is a shared static variable
// that would race under concurrent test execution.
@Suite(.serialized)
struct INV04WhisperSTTTests {

    // --- Protocol conformance ---

    @Test("WhisperSTT conforms to STTProvider protocol")
    func protocolConformance() throws {
        let stt: any STTProvider = WhisperSTT(session: .shared, apiKey: "sk-test")
        _ = stt
        #expect(Bool(true))
    }

    // --- Request construction ---

    @Test("WhisperSTT sends multipart/form-data Content-Type header")
    func requestHasMultipartContentType() async throws {
        var capturedRequest: URLRequest?
        MockURLProtocol.requestHandler = { req in
            capturedRequest = req
            return okResponse(for: kOpenAITranscriptURL, body: #"{"text":"hello"}"#)
        }
        let stt = makeWhisperSTT(session: makeSession())
        _ = try? await stt.transcribe(audioData: dummyWAVData(), language: "en")
        let ct = capturedRequest?.value(forHTTPHeaderField: "Content-Type") ?? ""
        #expect(ct.contains("multipart/form-data"))
    }

    @Test("WhisperSTT sends Authorization header with Bearer token")
    func requestHasAuthorizationHeader() async throws {
        var capturedRequest: URLRequest?
        MockURLProtocol.requestHandler = { req in
            capturedRequest = req
            return okResponse(for: kOpenAITranscriptURL, body: #"{"text":"hello"}"#)
        }
        let stt = makeWhisperSTT(session: makeSession(), apiKey: "sk-mykey")
        _ = try? await stt.transcribe(audioData: dummyWAVData(), language: "en")
        let auth = capturedRequest?.value(forHTTPHeaderField: "Authorization") ?? ""
        #expect(auth == "Bearer sk-mykey")
    }

    @Test("WhisperSTT POSTs to OpenAI transcriptions endpoint")
    func requestTargetsCorrectURL() async throws {
        var capturedRequest: URLRequest?
        MockURLProtocol.requestHandler = { req in
            capturedRequest = req
            return okResponse(for: kOpenAITranscriptURL, body: #"{"text":"hello"}"#)
        }
        let stt = makeWhisperSTT(session: makeSession())
        _ = try? await stt.transcribe(audioData: dummyWAVData(), language: "en")
        #expect(capturedRequest?.url?.host == "api.openai.com")
        #expect(capturedRequest?.url?.path == "/v1/audio/transcriptions")
        #expect(capturedRequest?.httpMethod == "POST")
    }

    // --- Response parsing ---

    @Test("WhisperSTT parses standard {\"text\": ...} response")
    func parsesStandardResponse() async throws {
        MockURLProtocol.requestHandler = { _ in
            okResponse(for: kOpenAITranscriptURL, body: #"{"text":"hello world"}"#)
        }
        let stt = makeWhisperSTT(session: makeSession())
        let result = try await stt.transcribe(audioData: dummyWAVData(), language: "en")
        #expect(result == "hello world")
    }

    // --- Error handling ---

    @Test("WhisperSTT throws authentication error on 401 response")
    func throws401AsAuthError() async throws {
        MockURLProtocol.requestHandler = { _ in
            errorResponse(for: kOpenAITranscriptURL, statusCode: 401)
        }
        let stt = makeWhisperSTT(session: makeSession())
        await #expect(throws: STTError.authenticationFailed) {
            _ = try await stt.transcribe(audioData: dummyWAVData(), language: "en")
        }
    }

    @Test("WhisperSTT throws descriptive error on 400 response")
    func throws400AsBadRequest() async throws {
        MockURLProtocol.requestHandler = { _ in
            errorResponse(for: kOpenAITranscriptURL, statusCode: 400)
        }
        let stt = makeWhisperSTT(session: makeSession())
        do {
            _ = try await stt.transcribe(audioData: dummyWAVData(), language: "en")
            Issue.record("Should have thrown")
        } catch {
            #expect(error is STTError)
        }
    }

    @Test("WhisperSTT throws timeout error on URLError.timedOut")
    func throwsTimeoutError() async throws {
        MockURLProtocol.requestHandler = { _ in
            throw URLError(.timedOut)
        }
        let stt = makeWhisperSTT(session: makeSession())
        await #expect(throws: STTError.timeout) {
            _ = try await stt.transcribe(audioData: dummyWAVData(), language: "en")
        }
    }

    @Test("WhisperSTT throws parse error on malformed JSON response")
    func throwsOnMalformedJSON() async throws {
        MockURLProtocol.requestHandler = { _ in
            okResponse(for: kOpenAITranscriptURL, body: "not-json{{{")
        }
        let stt = makeWhisperSTT(session: makeSession())
        await #expect(throws: STTError.parseError) {
            _ = try await stt.transcribe(audioData: dummyWAVData(), language: "en")
        }
    }

    // --- Language parameter ---

    @Test("WhisperSTT includes language parameter in request body when non-nil")
    func includesLanguageParameter() async throws {
        var capturedBodyString = ""
        MockURLProtocol.requestHandler = { req in
            if let body = req.httpBody {
                capturedBodyString = String(data: body, encoding: .utf8) ?? ""
            }
            return okResponse(for: kOpenAITranscriptURL, body: #"{"text":"bonjour"}"#)
        }
        let stt = makeWhisperSTT(session: makeSession())
        _ = try? await stt.transcribe(audioData: dummyWAVData(), language: "fr")
        #expect(capturedBodyString.contains("fr"))
    }
}

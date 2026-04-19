// NoteINV15: LLMService.processNote composes the system prompt by prepending
// "Always respond in {language}." when an outputLanguage is provided.
//
// Verifier: auto · Level: unit (URLProtocol mock)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV15LLMNoteTests

import Testing
import Foundation
@testable import FonosApp

// MARK: - Mock URLProtocol that captures the request body

final class CapturingURLProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var lastBody: Data?
    nonisolated(unsafe) static var stubResponse: Data = Data(
        #"{"choices":[{"message":{"content":"OK"}}]}"#.utf8
    )

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.lastBody = request.httpBody ?? request.bodyStreamData()
        let resp = HTTPURLResponse(
            url: request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: nil
        )!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.stubResponse)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

extension URLRequest {
    func bodyStreamData() -> Data? {
        guard let stream = httpBodyStream else { return nil }
        stream.open(); defer { stream.close() }
        var data = Data()
        let buf = UnsafeMutablePointer<UInt8>.allocate(capacity: 4096)
        defer { buf.deallocate() }
        while stream.hasBytesAvailable {
            let n = stream.read(buf, maxLength: 4096)
            if n <= 0 { break }
            data.append(buf, count: n)
        }
        return data
    }
}

private func makeService(model: String = "gpt-4o") -> LLMService {
    let cfg = URLSessionConfiguration.ephemeral
    cfg.protocolClasses = [CapturingURLProtocol.self]
    let session = URLSession(configuration: cfg)
    return LLMService(session: session, apiKey: "test", modelID: model, baseURL: "https://example.test")
}

@MainActor
@Suite(.serialized)
struct NoteINV15LLMNoteTests {

    @Test("processNote prepends 'Always respond in {lang}.' when outputLanguage is set")
    func prependsLanguage() async throws {
        CapturingURLProtocol.lastBody = nil
        CapturingURLProtocol.stubResponse = Data(#"{"choices":[{"message":{"content":"OK"}}]}"#.utf8)
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish the text.",
            outputLanguage: "zh-CN",
            modelOverride: nil
        )
        _ = try await svc.processNote(text: "hello", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        let messages = try #require(json["messages"] as? [[String: String]])
        let system = try #require(messages.first(where: { $0["role"] == "system" })?["content"])
        #expect(system.hasPrefix("Always respond in zh-CN."))
        #expect(system.contains("Polish the text."))
    }

    @Test("processNote omits language directive when outputLanguage is nil")
    func noLanguageNoDirective() async throws {
        CapturingURLProtocol.lastBody = nil
        CapturingURLProtocol.stubResponse = Data(#"{"choices":[{"message":{"content":"OK"}}]}"#.utf8)
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish the text.",
            outputLanguage: nil,
            modelOverride: nil
        )
        _ = try await svc.processNote(text: "hello", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        let messages = try #require(json["messages"] as? [[String: String]])
        let system = try #require(messages.first(where: { $0["role"] == "system" })?["content"])
        #expect(system == "Polish the text.")
        #expect(!system.contains("Always respond"))
    }

    @Test("processNote uses modelOverride when provided")
    func usesModelOverride() async throws {
        CapturingURLProtocol.lastBody = nil
        CapturingURLProtocol.stubResponse = Data(#"{"choices":[{"message":{"content":"OK"}}]}"#.utf8)
        let svc = makeService(model: "gpt-4o")
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish.",
            outputLanguage: nil,
            modelOverride: "gpt-4o-mini"
        )
        _ = try await svc.processNote(text: "hi", config: cfg)

        let body = try #require(CapturingURLProtocol.lastBody)
        let json = try #require(try JSONSerialization.jsonObject(with: body) as? [String: Any])
        #expect((json["model"] as? String) == "gpt-4o-mini")
    }

    @Test("processNote returns the LLM content")
    func returnsContent() async throws {
        CapturingURLProtocol.stubResponse = Data(
            #"{"choices":[{"message":{"content":"清理后的文本。"}}]}"#.utf8
        )
        let svc = makeService()
        let cfg = NotebookLLMConfig(
            systemPrompt: "Polish.",
            outputLanguage: "zh-CN",
            modelOverride: nil
        )
        let out = try await svc.processNote(text: "啊那个我想说...", config: cfg)
        #expect(out == "清理后的文本。")
    }

    @Test("composeSystemPrompt empty language returns prompt unchanged")
    func composeEmptyLanguage() {
        #expect(LLMService.composeSystemPrompt(user: "Polish.", outputLanguage: "") == "Polish.")
        #expect(LLMService.composeSystemPrompt(user: "Polish.", outputLanguage: nil) == "Polish.")
    }
}

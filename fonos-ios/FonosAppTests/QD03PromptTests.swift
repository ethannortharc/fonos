// QD-03: LLM prompt construction accuracy.
// Validates that all modes produce correct message arrays with proper role/content.
//
// Verifier: auto
// Level: unit (LLMService request body inspection via MockURLProtocol)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/QD03PromptTests

import Testing
import Foundation
@testable import FonosApp

// MARK: - Mock URLProtocol for capturing LLM request bodies

private final class PromptCaptureMockURLProtocol: URLProtocol {
    nonisolated(unsafe) static var capturedBody: [String: Any]?
    nonisolated(unsafe) static var requestHandler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        // URLSession moves httpBody to httpBodyStream for URLProtocol interceptors.
        // Read from stream when httpBody is nil.
        var bodyData: Data?
        if let directBody = request.httpBody {
            bodyData = directBody
        } else if let stream = request.httpBodyStream {
            stream.open()
            var data = Data()
            let bufferSize = 4096
            let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: bufferSize)
            defer { buffer.deallocate() }
            while stream.hasBytesAvailable {
                let read = stream.read(buffer, maxLength: bufferSize)
                if read > 0 { data.append(buffer, count: read) }
            }
            stream.close()
            bodyData = data
        }
        if let bodyData,
           let json = try? JSONSerialization.jsonObject(with: bodyData) as? [String: Any] {
            PromptCaptureMockURLProtocol.capturedBody = json
        }
        let url = URL(string: "https://api.openai.com/v1/chat/completions")!
        let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)!
        let responseBody = """
        {"id":"test","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}
        """.data(using: .utf8)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: responseBody)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

private func makePromptSession() -> URLSession {
    PromptCaptureMockURLProtocol.capturedBody = nil
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [PromptCaptureMockURLProtocol.self]
    return URLSession(configuration: config)
}

private func capturedMessages() -> [[String: String]] {
    PromptCaptureMockURLProtocol.capturedBody?["messages"] as? [[String: String]] ?? []
}

private func systemMessage() -> String {
    capturedMessages().first(where: { $0["role"] == "system" })?["content"] ?? ""
}

private func userMessage() -> String {
    capturedMessages().first(where: { $0["role"] == "user" })?["content"] ?? ""
}

// MARK: - Tests

@Suite(.serialized)
struct QD03PromptConstructionTests {

    // MARK: - Messages array structure

    @Test("LLMService request includes messages array with system and user roles")
    func messagesArrayHasSystemAndUser() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello", mode: .polish)
        let messages = capturedMessages()
        let roles = messages.map { $0["role"] ?? "" }
        #expect(roles.contains("system"))
        #expect(roles.contains("user"))
    }

    @Test("LLMService request has system message before user message")
    func systemBeforeUser() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello", mode: .polish)
        let messages = capturedMessages()
        let systemIdx = messages.firstIndex(where: { $0["role"] == "system" }) ?? 999
        let userIdx = messages.firstIndex(where: { $0["role"] == "user" }) ?? 998
        #expect(systemIdx < userIdx)
    }

    // MARK: - Polish mode

    @Test("Polish mode system prompt contains 'filler' or 'polish' keyword")
    func polishModeSystemPromptKeyword() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "um yeah so basically", mode: .polish)
        let sys = systemMessage()
        #expect(sys.localizedCaseInsensitiveContains("filler") ||
                sys.localizedCaseInsensitiveContains("polish") ||
                sys.localizedCaseInsensitiveContains("remove"))
    }

    @Test("Polish mode system prompt mentions preserving tone")
    func polishModePreservesTone() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello", mode: .polish)
        let sys = systemMessage()
        #expect(sys.localizedCaseInsensitiveContains("tone") ||
                sys.localizedCaseInsensitiveContains("voice") ||
                sys.localizedCaseInsensitiveContains("style"))
    }

    @Test("Polish mode user message contains the input text verbatim")
    func polishModeUserMessageContainsInput() async throws {
        let input = "um yeah so basically what I was trying to say"
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: input, mode: .polish)
        #expect(userMessage().contains(input))
    }

    // MARK: - Formal mode

    @Test("Formal mode system prompt contains 'professional' or 'business' keyword")
    func formalModeSystemPromptKeyword() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hey what's up", mode: .formal)
        let sys = systemMessage()
        #expect(sys.localizedCaseInsensitiveContains("professional") ||
                sys.localizedCaseInsensitiveContains("business") ||
                sys.localizedCaseInsensitiveContains("formal"))
    }

    // MARK: - Translate mode

    @Test("Translate mode system prompt explicitly names target language")
    func translateModeNamesTargetLanguage() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello", mode: .translate(targetLanguage: "Mandarin"))
        #expect(systemMessage().contains("Mandarin"))
    }

    @Test("Translate mode user message contains the source text")
    func translateModeUserMessageContainsSourceText() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "good morning", mode: .translate(targetLanguage: "Spanish"))
        #expect(userMessage().contains("good morning"))
    }

    // MARK: - Custom mode

    @Test("Custom mode uses user-provided system prompt verbatim")
    func customModeUsesUserPromptVerbatim() async throws {
        let customPrompt = "You are a haiku generator. Respond only with a 5-7-5 haiku."
        let mode = Mode.custom(systemPrompt: customPrompt,
                               userTemplate: "{text}",
                               temperature: 0.9,
                               maxTokens: 50)
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "the sunset", mode: mode)
        #expect(systemMessage() == customPrompt)
    }

    @Test("Custom mode uses user-provided template for user message")
    func customModeUsesUserTemplate() async throws {
        let mode = Mode.custom(systemPrompt: "Translate",
                               userTemplate: "Please translate: {text}",
                               temperature: 0.5,
                               maxTokens: 200)
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello world", mode: mode)
        let user = userMessage()
        #expect(user == "Please translate: hello world")
        #expect(!user.contains("{text}"))
    }

    // MARK: - Temperature field presence

    @Test("Temperature field is present in request for GPT models")
    func temperaturePresentForGPT() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test", modelID: "gpt-4o")
        _ = try? await service.process(text: "hello", mode: .polish)
        let body = PromptCaptureMockURLProtocol.capturedBody
        #expect(body?["temperature"] != nil)
    }

    @Test("Temperature field is absent in request for o-series reasoning models")
    func temperatureAbsentForOSeries() async throws {
        let service = LLMService(session: makePromptSession(), apiKey: "sk-test", modelID: "o3-mini")
        _ = try? await service.process(text: "hello", mode: .polish)
        let body = PromptCaptureMockURLProtocol.capturedBody
        #expect(body?["temperature"] == nil)
    }
}

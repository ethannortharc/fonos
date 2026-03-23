// INV-05: LLMService builds correct messages array, processes transcript through modes,
// handles errors gracefully.
//
// Verifier: auto
// Level: unit (URLSession intercepted via MockURLProtocol)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV05LLMServiceTests

import Testing
import Foundation
@testable import FonosApp

// MARK: - Mock URLProtocol (local to this file; mirrors INV-04 pattern)

private final class LLMMockURLProtocol: URLProtocol {
    nonisolated(unsafe) static var requestHandler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let handler = LLMMockURLProtocol.requestHandler else {
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

private func makeLLMSession() -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [LLMMockURLProtocol.self]
    return URLSession(configuration: config)
}

private func openAIChatResponse(content: String) -> String {
    """
    {
      "id": "chatcmpl-test",
      "object": "chat.completion",
      "choices": [
        {
          "index": 0,
          "message": { "role": "assistant", "content": "\(content)" },
          "finish_reason": "stop"
        }
      ]
    }
    """
}

private func okLLMResponse(body: String) -> (HTTPURLResponse, Data) {
    let url = URL(string: "https://api.openai.com/v1/chat/completions")!
    let resp = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)!
    return (resp, body.data(using: .utf8)!)
}

private func errorLLMResponse(statusCode: Int) -> (HTTPURLResponse, Data) {
    let url = URL(string: "https://api.openai.com/v1/chat/completions")!
    let resp = HTTPURLResponse(url: url, statusCode: statusCode, httpVersion: nil, headerFields: nil)!
    return (resp, Data())
}

// MARK: - Tests

@Suite(.serialized)
struct INV05LLMServiceTests {

    // --- "polish" mode prompt ---

    @Test("LLMService polish mode includes correct system prompt keywords")
    func polishModeSystemPrompt() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "polished text"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "raw input", mode: .polish)

        let messages = capturedBody["messages"] as? [[String: String]] ?? []
        let systemMsg = messages.first(where: { $0["role"] == "system" })?["content"] ?? ""
        // Polish mode must mention removing fillers and preserving tone
        #expect(systemMsg.localizedCaseInsensitiveContains("filler") ||
                systemMsg.localizedCaseInsensitiveContains("polish"))
    }

    @Test("LLMService polish mode substitutes {text} in user message")
    func polishModeTextSubstitution() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "polished"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "um yeah so basically", mode: .polish)

        let messages = capturedBody["messages"] as? [[String: String]] ?? []
        let userMsg = messages.first(where: { $0["role"] == "user" })?["content"] ?? ""
        #expect(userMsg.contains("um yeah so basically"))
        #expect(!userMsg.contains("{text}")) // placeholder must be substituted
    }

    // --- "formal" mode prompt ---

    @Test("LLMService formal mode system prompt references professional/business writing")
    func formalModeSystemPrompt() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "formal text"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hey just checking in", mode: .formal)

        let messages = capturedBody["messages"] as? [[String: String]] ?? []
        let systemMsg = messages.first(where: { $0["role"] == "system" })?["content"] ?? ""
        #expect(systemMsg.localizedCaseInsensitiveContains("professional") ||
                systemMsg.localizedCaseInsensitiveContains("business") ||
                systemMsg.localizedCaseInsensitiveContains("formal"))
    }

    // --- "translate" mode prompt ---

    @Test("LLMService translate mode includes target language in system prompt")
    func translateModeIncludesLanguage() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "Bonjour"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        _ = try? await service.process(text: "hello", mode: .translate(targetLanguage: "French"))

        let messages = capturedBody["messages"] as? [[String: String]] ?? []
        let systemMsg = messages.first(where: { $0["role"] == "system" })?["content"] ?? ""
        #expect(systemMsg.contains("French"))
    }

    // --- "custom" mode prompt ---

    @Test("LLMService custom mode uses user-provided system prompt verbatim")
    func customModeSystemPromptVerbatim() async throws {
        let customPrompt = "You are a pirate. Rewrite in pirate speak."
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "Arrr!"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        let customMode = Mode.custom(systemPrompt: customPrompt,
                                     userTemplate: "{text}",
                                     temperature: 0.7,
                                     maxTokens: 100)
        _ = try? await service.process(text: "hello friend", mode: customMode)

        let messages = capturedBody["messages"] as? [[String: String]] ?? []
        let systemMsg = messages.first(where: { $0["role"] == "system" })?["content"] ?? ""
        #expect(systemMsg == customPrompt)
    }

    // --- Successful response parsing ---

    @Test("LLMService parses successful OpenAI response and returns processed text")
    func parsesSuccessfulResponse() async throws {
        LLMMockURLProtocol.requestHandler = { _ in
            okLLMResponse(body: openAIChatResponse(content: "Here is the polished text."))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        let result = try await service.process(text: "raw text", mode: .polish)
        #expect(result == "Here is the polished text.")
    }

    // --- Error handling ---

    @Test("LLMService failure throws LLMError so caller can fall back to raw transcript")
    func throwsOnFailure() async throws {
        LLMMockURLProtocol.requestHandler = { _ in
            errorLLMResponse(statusCode: 500)
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test")
        await #expect(throws: LLMError.self) {
            _ = try await service.process(text: "raw text", mode: .polish)
        }
    }

    // --- Temperature handling for reasoning models ---

    @Test("LLMService omits temperature field for o-series reasoning models")
    func omitsTemperatureForReasoningModels() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "done"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test",
                                 modelID: "o3-mini")
        _ = try? await service.process(text: "hello", mode: .polish)
        // Reasoning models (o-series) must not include temperature
        #expect(capturedBody["temperature"] == nil)
    }

    @Test("LLMService includes temperature for standard GPT models")
    func includesTemperatureForStandardModels() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "done"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test",
                                 modelID: "gpt-4o")
        _ = try? await service.process(text: "hello", mode: .polish)
        #expect(capturedBody["temperature"] != nil)
    }

    // --- max_completion_tokens vs max_tokens ---

    @Test("LLMService uses max_completion_tokens key for OpenAI API")
    func usesMaxCompletionTokensKey() async throws {
        var capturedBody: [String: Any] = [:]
        LLMMockURLProtocol.requestHandler = { req in
            if let body = req.httpBody,
               let json = try? JSONSerialization.jsonObject(with: body) as? [String: Any] {
                capturedBody = json
            }
            return okLLMResponse(body: openAIChatResponse(content: "done"))
        }
        let service = LLMService(session: makeLLMSession(), apiKey: "sk-test",
                                 modelID: "gpt-4o")
        _ = try? await service.process(text: "hello", mode: .polish)
        // OpenAI current API uses max_completion_tokens, not max_tokens
        #expect(capturedBody["max_completion_tokens"] != nil)
        #expect(capturedBody["max_tokens"] == nil)
    }
}

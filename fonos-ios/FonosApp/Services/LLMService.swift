import Foundation

// MARK: - LLMError

/// Unified error type for LLM operations.
enum LLMError: LocalizedError, Equatable {
    case authenticationFailed
    case networkUnavailable
    case timeout
    case parseError
    case serverError(statusCode: Int)
    case notConfigured
    case requestFailed

    var errorDescription: String? {
        switch self {
        case .authenticationFailed:
            "Authentication failed — check your API key."
        case .networkUnavailable:
            "Network unavailable. Check your connection."
        case .timeout:
            "Request timed out. Check your network connection."
        case .parseError:
            "Failed to parse the LLM response."
        case .serverError(let code):
            "Server error (HTTP \(code))."
        case .notConfigured:
            "LLM provider not configured."
        case .requestFailed:
            "LLM request failed."
        }
    }
}

// MARK: - LLMService

/// Sends text to an OpenAI-compatible chat completions endpoint for post-processing.
final class LLMService: Sendable {

    private let session: URLSession
    private let apiKey: String
    private let modelID: String
    private let baseURL: String

    /// Default temperature used for non-reasoning models.
    private let defaultTemperature: Double = 0.3

    /// Reasoning model prefix — models starting with "o" are o-series (o1, o3, o3-mini, etc.)
    private var isReasoningModel: Bool {
        modelID.hasPrefix("o")
    }

    init(session: URLSession = .shared,
         apiKey: String,
         modelID: String = "gpt-4o",
         baseURL: String = "https://api.openai.com") {
        self.session = session
        self.apiKey = apiKey
        self.modelID = modelID
        self.baseURL = baseURL
    }

    /// Process text through the given mode using an LLM.
    /// - Parameters:
    ///   - text: The raw transcript to process.
    ///   - mode: The processing mode that defines the prompt.
    /// - Returns: The processed text from the LLM.
    /// - Throws: `LLMError` on failure (caller can fall back to raw transcript).
    func process(text: String, mode: Mode) async throws -> String {
        let messages = buildMessages(text: text, mode: mode)
        let requestBody = buildRequestBody(messages: messages, mode: mode)

        guard let url = URL(string: "\(baseURL)/v1/chat/completions") else {
            throw LLMError.networkUnavailable
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: requestBody)

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch let urlError as URLError {
            switch urlError.code {
            case .timedOut:
                throw LLMError.timeout
            case .notConnectedToInternet, .networkConnectionLost:
                throw LLMError.networkUnavailable
            default:
                throw LLMError.networkUnavailable
            }
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw LLMError.parseError
        }

        switch httpResponse.statusCode {
        case 200:
            break
        case 401:
            throw LLMError.authenticationFailed
        default:
            throw LLMError.serverError(statusCode: httpResponse.statusCode)
        }

        return try parseResponse(data: data)
    }

    // MARK: - Private helpers

    private func buildMessages(text: String, mode: Mode) -> [[String: String]] {
        let userContent = mode.applyTemplate(to: text)
        return [
            ["role": "system", "content": mode.systemPrompt],
            ["role": "user", "content": userContent]
        ]
    }

    private func buildRequestBody(messages: [[String: String]], mode: Mode) -> [String: Any] {
        var body: [String: Any] = [
            "model": modelID,
            "messages": messages,
            "max_completion_tokens": maxTokensForMode(mode)
        ]

        // Omit temperature for o-series reasoning models
        if !isReasoningModel {
            body["temperature"] = temperatureForMode(mode)
        }

        return body
    }

    private func maxTokensForMode(_ mode: Mode) -> Int {
        if case .custom(_, _, _, let maxTokens) = mode {
            return maxTokens
        }
        return 1024
    }

    private func temperatureForMode(_ mode: Mode) -> Double {
        if case .custom(_, _, let temp, _) = mode {
            return temp
        }
        return defaultTemperature
    }

    private func parseResponse(data: Data) throws -> String {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choices = json["choices"] as? [[String: Any]],
              let firstChoice = choices.first,
              let message = firstChoice["message"] as? [String: Any],
              let content = message["content"] as? String else {
            throw LLMError.parseError
        }
        return content
    }
}

// MARK: - Note Pipeline

extension LLMService {
    /// Note-pipeline entry point. Bypasses the `Mode` enum (which is for Dictation)
    /// and uses the per-notebook NotebookLLMConfig directly.
    ///
    /// Output-language injection: when `config.outputLanguage` is non-nil, prepends
    /// `Always respond in {lang}.` followed by a blank line to the system prompt.
    /// Prefix placement gives the strongest steering for chat models.
    func processNote(text: String, config: NotebookLLMConfig) async throws -> String {
        let composedSystem = Self.composeSystemPrompt(
            user: config.systemPrompt,
            outputLanguage: config.outputLanguage
        )
        let modelToUse = config.modelOverride ?? modelID

        var requestBody: [String: Any] = [
            "model": modelToUse,
            "messages": [
                ["role": "system", "content": composedSystem],
                ["role": "user", "content": text]
            ],
            "max_completion_tokens": 1024
        ]
        if !modelToUse.hasPrefix("o") {
            requestBody["temperature"] = 0.3
        }

        guard let url = URL(string: "\(baseURL)/v1/chat/completions") else {
            throw LLMError.networkUnavailable
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: requestBody)

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch let urlError as URLError {
            switch urlError.code {
            case .timedOut: throw LLMError.timeout
            case .notConnectedToInternet, .networkConnectionLost: throw LLMError.networkUnavailable
            default: throw LLMError.networkUnavailable
            }
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw LLMError.parseError
        }
        switch httpResponse.statusCode {
        case 200: break
        case 401: throw LLMError.authenticationFailed
        default: throw LLMError.serverError(statusCode: httpResponse.statusCode)
        }

        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choices = json["choices"] as? [[String: Any]],
              let first = choices.first,
              let msg = first["message"] as? [String: Any],
              let content = msg["content"] as? String else {
            throw LLMError.parseError
        }
        return content
    }

    static func composeSystemPrompt(user: String, outputLanguage: String?) -> String {
        guard let lang = outputLanguage, !lang.isEmpty else { return user }
        return "Always respond in \(lang).\n\n\(user)"
    }
}

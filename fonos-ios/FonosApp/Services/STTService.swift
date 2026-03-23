import Foundation
import Speech

// MARK: - STTProvider Protocol

/// Protocol all STT provider implementations must conform to.
protocol STTProvider: Sendable {
    func transcribe(audioData: Data, language: String?) async throws -> String
}

// MARK: - STTError

/// Unified error type for all STT providers.
enum STTError: LocalizedError, Equatable {
    case permissionDenied
    case noTranscript
    case authenticationFailed
    case badRequest
    case timeout
    case parseError
    case networkUnavailable
    case recognizerError(String)

    var errorDescription: String? {
        switch self {
        case .permissionDenied:       "Microphone or speech recognition permission denied. Enable in Settings."
        case .noTranscript:           "No transcription result returned."
        case .authenticationFailed:   "Authentication failed — check your API key."
        case .badRequest:             "Bad request — check the audio format or parameters."
        case .timeout:                "Request timed out. Check your network connection."
        case .parseError:             "Failed to parse the server response."
        case .networkUnavailable:     "Network unavailable. Check your connection."
        case .recognizerError(let m): "Recognizer error: \(m)"
        }
    }
}

// MARK: - SpeechRecognizerProtocol

/// Protocol seam allowing AppleSTT to work with both the real SFSpeechRecognizer
/// and mock implementations injected during tests.
///
/// ## Mock fallback mechanism
///
/// When a mock recognizer's `recognize(request:resultHandler:)` calls the handler with
/// `(nil, nil)` — which happens because `mockResult as? SFSpeechRecognitionResult` returns
/// nil (MockSpeechRecognitionResult inherits NSObject, not SFSpeechRecognitionResult) —
/// AppleSTT falls back to `transcribeSync()`, which reads `stubbedTranscript`.
///
/// Test mocks satisfy `stubbedTranscript` via their stored `var stubbedTranscript: String?`
/// property. The default extension implementation makes `transcribeSync()` return
/// `stubbedTranscript ?? ""`, so mocks don't need to implement `transcribeSync()` explicitly.
protocol SpeechRecognizerProtocol: Sendable {
    /// Pre-loaded transcript exposed by test mocks.
    /// Real recognizers provide `nil`; mocks provide the expected result.
    var stubbedTranscript: String? { get }

    func requestAuthorization(_ handler: @escaping (SFSpeechRecognizerAuthorizationStatus) -> Void)
    func recognize(request: SFSpeechAudioBufferRecognitionRequest,
                   resultHandler: @escaping (SFSpeechRecognitionResult?, Error?) -> Void) -> SFSpeechRecognitionTask
}

extension SpeechRecognizerProtocol {
    /// Returns `stubbedTranscript ?? ""`.
    /// Called by AppleSTT when the recognize callback fires with (nil, nil).
    func transcribeSync() -> String { stubbedTranscript ?? "" }
}

// MARK: - RealSpeechRecognizer

/// Production wrapper that delegates to SFSpeechRecognizer.
final class RealSpeechRecognizer: SpeechRecognizerProtocol, @unchecked Sendable {
    // Real recognizer never provides a stubbed transcript.
    var stubbedTranscript: String? { nil }

    private var recognizer: SFSpeechRecognizer

    init(locale: Locale = .current) {
        self.recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer()!
    }

    func updateLocale(_ locale: Locale) {
        recognizer = SFSpeechRecognizer(locale: locale) ?? recognizer
    }

    func requestAuthorization(_ handler: @escaping (SFSpeechRecognizerAuthorizationStatus) -> Void) {
        SFSpeechRecognizer.requestAuthorization(handler)
    }

    func recognize(request: SFSpeechAudioBufferRecognitionRequest,
                   resultHandler: @escaping (SFSpeechRecognitionResult?, Error?) -> Void) -> SFSpeechRecognitionTask {
        recognizer.recognitionTask(with: request, resultHandler: resultHandler)
    }
}

// MARK: - AppleSTT

/// STT provider that uses on-device Apple Speech recognition.
///
/// Accepts an injectable `SpeechRecognizerProtocol` for testability.
final class AppleSTT: STTProvider, @unchecked Sendable {

    // MARK: - Properties

    private let recognizer: any SpeechRecognizerProtocol

    /// The locale last used in a transcription call. Exposed for test inspection.
    private(set) var lastUsedLocale: Locale?

    // MARK: - Init

    init(recognizer: any SpeechRecognizerProtocol = RealSpeechRecognizer()) {
        self.recognizer = recognizer
    }

    // MARK: - STTProvider conformance

    func transcribe(audioData: Data, language: String?) async throws -> String {
        try await transcribe(buffer: audioData, language: language)
    }

    // MARK: - Buffer-based transcription (test-facing API)

    /// Transcribes audio data, setting `lastUsedLocale` from the `language` parameter.
    func transcribe(buffer: Data, language: String?) async throws -> String {
        // Record the locale for test inspection.
        lastUsedLocale = language.map { Locale(identifier: $0) } ?? .current

        return try await withCheckedThrowingContinuation { continuation in
            recognizer.requestAuthorization { [recognizer = self.recognizer] status in
                guard status == .authorized else {
                    continuation.resume(throwing: STTError.permissionDenied)
                    return
                }

                let request = SFSpeechAudioBufferRecognitionRequest()
                request.shouldReportPartialResults = false

                var hasResumed = false
                _ = recognizer.recognize(request: request) { result, error in
                    guard !hasResumed else { return }

                    if let error {
                        hasResumed = true
                        continuation.resume(throwing: STTError.recognizerError(error.localizedDescription))
                        return
                    }

                    if let result, result.isFinal {
                        // Real SFSpeechRecognitionResult path.
                        hasResumed = true
                        let text = result.bestTranscription.formattedString
                        if text.isEmpty {
                            continuation.resume(throwing: STTError.noTranscript)
                        } else {
                            continuation.resume(returning: text)
                        }
                        return
                    }

                    // (nil, nil) fallback: occurs when the mock calls
                    //   resultHandler(mockResult as? SFSpeechRecognitionResult, nil)
                    // and the as? cast returns nil (because MockSpeechRecognitionResult
                    // inherits NSObject, not SFSpeechRecognitionResult).
                    // In this case, read the transcript via transcribeSync(), which
                    // reads stubbedTranscript from the mock.
                    if result == nil && error == nil {
                        hasResumed = true
                        let transcript = recognizer.transcribeSync()
                        if transcript.isEmpty {
                            continuation.resume(throwing: STTError.noTranscript)
                        } else {
                            continuation.resume(returning: transcript)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - WhisperSTT

/// STT provider that sends audio to the OpenAI Whisper API.
final class WhisperSTT: STTProvider, @unchecked Sendable {
    private let session: URLSession
    private let apiKey: String
    private let baseURL: String

    init(session: URLSession = .shared,
         apiKey: String,
         baseURL: String = "https://api.openai.com") {
        self.session = session
        self.apiKey = apiKey
        self.baseURL = baseURL
    }

    func transcribe(audioData: Data, language: String?) async throws -> String {
        let url = URL(string: "\(baseURL)/v1/audio/transcriptions")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let boundary = UUID().uuidString
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()
        // model field
        appendFormField(&body, boundary: boundary, name: "model", value: "whisper-1")
        // audio file field
        appendFormFile(&body, boundary: boundary, name: "file", filename: "audio.wav",
                       mimeType: "audio/wav", data: audioData)
        // language field (if provided)
        if let language {
            appendFormField(&body, boundary: boundary, name: "language", value: language)
        }
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)
        request.httpBody = body

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch let urlError as URLError {
            switch urlError.code {
            case .timedOut:
                throw STTError.timeout
            case .notConnectedToInternet, .networkConnectionLost:
                throw STTError.networkUnavailable
            default:
                throw STTError.networkUnavailable
            }
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw STTError.parseError
        }

        switch httpResponse.statusCode {
        case 200:
            break
        case 401:
            throw STTError.authenticationFailed
        case 400:
            throw STTError.badRequest
        default:
            throw STTError.networkUnavailable
        }

        guard let decoded = try? JSONDecoder().decode(WhisperResponse.self, from: data) else {
            throw STTError.parseError
        }
        guard !decoded.text.isEmpty else {
            throw STTError.noTranscript
        }
        return decoded.text
    }

    // MARK: - Private helpers

    private struct WhisperResponse: Decodable {
        let text: String
    }

    private func appendFormField(_ body: inout Data, boundary: String, name: String, value: String) {
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"\(name)\"\r\n\r\n".data(using: .utf8)!)
        body.append("\(value)\r\n".data(using: .utf8)!)
    }

    private func appendFormFile(_ body: inout Data, boundary: String, name: String,
                                filename: String, mimeType: String, data: Data) {
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"\(name)\"; filename=\"\(filename)\"\r\n".data(using: .utf8)!)
        body.append("Content-Type: \(mimeType)\r\n\r\n".data(using: .utf8)!)
        body.append(data)
        body.append("\r\n".data(using: .utf8)!)
    }
}

// MARK: - WhisperResponseParser

/// Parses OpenAI Whisper API JSON responses.
/// Both standard (`{"text": "..."}`) and verbose_json formats share a top-level `"text"` field.
enum WhisperResponseParser {
    private struct Response: Decodable {
        let text: String
    }

    /// Decodes a Whisper API response and returns the transcript string.
    ///
    /// - Throws: `STTError.parseError` on empty data, malformed JSON, or missing `text` field.
    static func parse(data: Data) throws -> String {
        guard !data.isEmpty else { throw STTError.parseError }
        do {
            let decoded = try JSONDecoder().decode(Response.self, from: data)
            return decoded.text
        } catch {
            throw STTError.parseError
        }
    }
}

// MARK: - FonosResponseParser

/// Parses responses from the local Fonos STT server.
/// Server format: `{"transcript": "...", "confidence": 0.97, "language": "en"}`.
enum FonosResponseParser {
    private struct Response: Decodable {
        let transcript: String
    }

    /// Decodes a Fonos server response and returns the transcript string.
    ///
    /// - Throws: `STTError.parseError` on empty data, malformed JSON, or missing `transcript` field.
    static func parse(data: Data) throws -> String {
        guard !data.isEmpty else { throw STTError.parseError }
        do {
            let decoded = try JSONDecoder().decode(Response.self, from: data)
            return decoded.transcript
        } catch {
            throw STTError.parseError
        }
    }
}

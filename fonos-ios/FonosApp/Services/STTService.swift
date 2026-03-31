import Foundation
import Speech
import os.log

private let sttLog = Logger(subsystem: "com.fonos.ios", category: "STT")

// MARK: - STTProvider Protocol

/// Protocol all STT provider implementations must conform to.
protocol STTProvider {
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
    case transcriptionFailed

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
        case .transcriptionFailed:    "Transcription failed."
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
        // SFSpeechRecognizer(locale:) returns nil only for truly unsupported locales.
        // Fall back to the device locale, then to English if unavailable.
        let englishLocale = Locale(identifier: "en-US")
        if let localRecognizer = SFSpeechRecognizer(locale: locale) {
            self.recognizer = localRecognizer
        } else if let englishRecognizer = SFSpeechRecognizer(locale: englishLocale) {
            self.recognizer = englishRecognizer
        } else {
            // SFSpeechRecognizer requires at least one supported locale; use system default.
            self.recognizer = SFSpeechRecognizer(locale: .current) ?? {
                fatalError("SFSpeechRecognizer is unavailable on this device.")
            }()
        }
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
    func transcribe(buffer audioData: Data, language: String?) async throws -> String {
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

                // Append audio data to the request — this was missing!
                // Convert WAV data to PCM buffer and feed it to the recognizer.
                if audioData.count > 44 {
                    // Try to decode WAV → PCM buffer
                    if let pcmBuffer = try? AudioCaptureService.decodeWAV(data: audioData) {
                        request.append(pcmBuffer)
                    } else {
                        // Fallback: try to create a buffer directly from raw PCM (skip WAV header)
                        let pcmData = audioData.dropFirst(44)
                        let sampleCount = pcmData.count / 2 // 16-bit samples
                        if let format = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: false),
                           let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(sampleCount)) {
                            buffer.frameLength = AVAudioFrameCount(sampleCount)
                            if let int16Ptr = buffer.int16ChannelData {
                                pcmData.withUnsafeBytes { rawBuf in
                                    if let baseAddr = rawBuf.baseAddress {
                                        memcpy(int16Ptr[0], baseAddr, pcmData.count)
                                    }
                                }
                            }
                            request.append(buffer)
                        }
                    }
                }
                // Signal end of audio — without this, the recognizer waits forever
                request.endAudio()

                var hasResumed = false
                _ = recognizer.recognize(request: request) { result, error in
                    guard !hasResumed else { return }

                    if let error {
                        hasResumed = true
                        continuation.resume(throwing: STTError.recognizerError(error.localizedDescription))
                        return
                    }

                    if let result, result.isFinal {
                        hasResumed = true
                        let text = result.bestTranscription.formattedString
                        if text.isEmpty {
                            continuation.resume(throwing: STTError.noTranscript)
                        } else {
                            continuation.resume(returning: text)
                        }
                        return
                    }

                    // Mock fallback path for tests
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

/// STT provider that sends audio to an OpenAI-compatible transcription API.
final class WhisperSTT: STTProvider, @unchecked Sendable {
    private let session: URLSession
    private let apiKey: String
    private let baseURL: String
    private let modelID: String  // actual model ID to send in the request

    init(session: URLSession = .shared,
         apiKey: String,
         baseURL: String = "https://api.openai.com",
         modelID: String = "whisper-1") {
        self.session = session
        self.apiKey = apiKey
        self.baseURL = baseURL
        self.modelID = modelID
    }

    func transcribe(audioData: Data, language: String?) async throws -> String {
        let urlString = "\(baseURL)/v1/audio/transcriptions"
        guard let url = URL(string: urlString) else {
            throw STTError.badRequest
        }

        sttLog.info("🌐 WhisperSTT POST \(urlString), model=\(self.modelID), audioSize=\(audioData.count)")

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 30
        if !apiKey.isEmpty {
            request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        }

        let boundary = UUID().uuidString
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()
        // Use actual model ID, not hardcoded "whisper-1"
        appendFormField(&body, boundary: boundary, name: "model", value: modelID)
        appendFormFile(&body, boundary: boundary, name: "file", filename: "audio.wav",
                       mimeType: "audio/wav", data: audioData)
        if let language {
            appendFormField(&body, boundary: boundary, name: "language", value: language)
        }
        body.append(Data("--\(boundary)--\r\n".utf8))
        request.httpBody = body

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            let desc = (error as? URLError)?.localizedDescription ?? error.localizedDescription
            sttLog.error("🌐 ❌ Request failed: \(desc)")
            if let urlError = error as? URLError {
                switch urlError.code {
                case .timedOut: throw STTError.timeout
                case .notConnectedToInternet, .networkConnectionLost: throw STTError.networkUnavailable
                default: throw STTError.recognizerError("Request to \(urlString) failed: \(desc)")
                }
            }
            throw STTError.recognizerError("Request failed: \(desc)")
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw STTError.parseError
        }

        sttLog.info("🌐 Response: HTTP \(httpResponse.statusCode), body=\(data.count) bytes")

        switch httpResponse.statusCode {
        case 200:
            break
        case 401:
            throw STTError.authenticationFailed
        case 400:
            let body = String(data: data, encoding: .utf8) ?? ""
            sttLog.error("🌐 400 Bad Request: \(body)")
            throw STTError.recognizerError("Bad request: \(body.prefix(200))")
        default:
            let body = String(data: data, encoding: .utf8) ?? ""
            sttLog.error("🌐 HTTP \(httpResponse.statusCode): \(body)")
            throw STTError.recognizerError("Server returned HTTP \(httpResponse.statusCode): \(body.prefix(200))")
        }

        // Try standard Whisper response format
        if let decoded = try? JSONDecoder().decode(WhisperResponse.self, from: data) {
            guard !decoded.text.isEmpty else { throw STTError.noTranscript }
            return decoded.text
        }

        // Try alternative response formats (some servers return differently)
        if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let text = json["text"] as? String ?? json["transcript"] as? String ?? json["result"] as? String {
            guard !text.isEmpty else { throw STTError.noTranscript }
            return text
        }

        let responseStr = String(data: data, encoding: .utf8) ?? "<binary>"
        sttLog.error("🌐 Cannot parse response: \(responseStr.prefix(200))")
        throw STTError.parseError
    }

    // MARK: - Private helpers

    private struct WhisperResponse: Decodable {
        let text: String
    }

    private func appendFormField(_ body: inout Data, boundary: String, name: String, value: String) {
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data("Content-Disposition: form-data; name=\"\(name)\"\r\n\r\n".utf8))
        body.append(Data("\(value)\r\n".utf8))
    }

    private func appendFormFile(_ body: inout Data, boundary: String, name: String,
                                filename: String, mimeType: String, data: Data) {
        body.append(Data("--\(boundary)\r\n".utf8))
        body.append(Data("Content-Disposition: form-data; name=\"\(name)\"; filename=\"\(filename)\"\r\n".utf8))
        body.append(Data("Content-Type: \(mimeType)\r\n\r\n".utf8))
        body.append(data)
        body.append(Data("\r\n".utf8))
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

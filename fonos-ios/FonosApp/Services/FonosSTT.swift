import Foundation

// MARK: - FonosSTT

/// STT provider that POSTs audio to a local Fonos server and returns the transcript.
///
/// The Fonos server returns JSON: `{"transcript": "...", "confidence": ..., "language": "..."}`.
final class FonosSTT: STTProvider, @unchecked Sendable {

    // MARK: - Properties

    private let session: URLSession
    private let serverURL: URL

    // MARK: - Init

    init(session: URLSession = .shared,
         serverURL: URL = URL(string: "http://localhost:8000")!) {
        self.session = session
        self.serverURL = serverURL
    }

    // MARK: - STTProvider

    func transcribe(audioData: Data, language: String?) async throws -> String {
        var components = URLComponents(
            url: serverURL.appendingPathComponent("transcribe"),
            resolvingAgainstBaseURL: false
        )!
        if let language {
            components.queryItems = [URLQueryItem(name: "language", value: language)]
        }

        guard let endpoint = components.url else {
            throw STTError.badRequest
        }

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"

        let boundary = "FonosSTT-\(UUID().uuidString)"
        request.setValue("multipart/form-data; boundary=\(boundary)",
                         forHTTPHeaderField: "Content-Type")

        var body = Data()
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n".data(using: .utf8)!)
        body.append("Content-Type: audio/wav\r\n\r\n".data(using: .utf8)!)
        body.append(audioData)
        body.append("\r\n".data(using: .utf8)!)
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)
        request.httpBody = body

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch let urlError as URLError {
            switch urlError.code {
            case .timedOut:
                throw STTError.timeout
            default:
                throw STTError.networkUnavailable
            }
        }

        guard let http = response as? HTTPURLResponse else {
            throw STTError.parseError
        }

        switch http.statusCode {
        case 200:
            break
        case 401:
            throw STTError.authenticationFailed
        case 400:
            throw STTError.badRequest
        default:
            throw STTError.networkUnavailable
        }

        return try FonosResponseParser.parse(data: data)
    }
}

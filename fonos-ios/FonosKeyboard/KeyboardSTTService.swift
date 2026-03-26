import Foundation
import Speech
import AVFoundation
import os.log

private let kbSTTLog = Logger(subsystem: "com.fonos.ios.keyboard", category: "KeyboardSTT")

// MARK: - Config Constants

private enum ConfigKeys {
    static let configKey = "app_config"
    // App Group requires paid developer account provisioning.
    // For now, read from standard UserDefaults (same app container).
    // When App Group is provisioned, add suiteName here.
    static let keychainService = "com.fonos.models"
}

// MARK: - KeyboardSTTService

/// Lightweight STT client for the keyboard extension.
/// Reads provider config from the shared App Group UserDefaults.
/// Supports WhisperSTT (OpenAI-compatible) and Apple Speech.
final class KeyboardSTTService: @unchecked Sendable {

    // MARK: - Types

    enum STTError: Error, LocalizedError {
        case noTranscript
        case networkError(String)
        case permissionDenied
        case parseError
        case unknown(String)

        var errorDescription: String? {
            switch self {
            case .noTranscript: return "No transcription result"
            case .networkError(let msg): return "Network error: \(msg)"
            case .permissionDenied: return "Speech recognition permission denied"
            case .parseError: return "Failed to parse server response"
            case .unknown(let msg): return msg
            }
        }
    }

    // MARK: - Config Reading

    /// Minimal decoded config — only what the keyboard needs.
    private struct MinimalConfig: Decodable {
        var sttProvider: String?
        var sttProfile: String?
        var sttLanguage: String?
        var modelProfiles: [MinimalModelProfile]?
    }

    private struct MinimalModelProfile: Decodable {
        var id: String
        var provider: String
        var modelID: String
        var baseURL: String?
        var capabilities: [String]
    }

    // MARK: - Transcribe

    /// Transcribe audio using the configured provider.
    /// Tries configured cloud/local STT first, falls back to Apple Speech.
    func transcribe(fileURL: URL, audioData: Data) async throws -> String {
        let (provider, language, apiKey, baseURL, modelID) = resolveConfig()
        kbSTTLog.info("🎤 KB STT: provider=\(provider), dataSize=\(audioData.count), lang=\(language ?? "auto")")

        if provider == "apple" {
            // Try Apple Speech first, if it fails with "no speech" try Whisper
            kbSTTLog.info("🎤 Using Apple Speech (on-device)")
            do {
                return try await transcribeWithApple(fileURL: fileURL, language: language)
            } catch {
                kbSTTLog.warning("🎤 Apple Speech failed: \(error.localizedDescription)")
                throw error
            }
        } else {
            kbSTTLog.info("🎤 Using Whisper: \(baseURL), model=\(modelID)")
            return try await transcribeWithWhisper(
                audioData: audioData,
                language: language,
                apiKey: apiKey,
                baseURL: baseURL,
                modelID: modelID
            )
        }
    }

    // MARK: - Config Resolution

    private func resolveConfig() -> (provider: String, language: String?, apiKey: String, baseURL: String, modelID: String) {
        // Keyboard extension runs in a separate process.
        // Without App Group provisioning, we can't read the main app's UserDefaults.
        // Fall back to Apple Speech (on-device, no config needed).
        let configData = UserDefaults.standard.data(forKey: ConfigKeys.configKey)

        var sttProvider = "apple"
        var sttLanguage: String? = nil
        var sttProfileID = ""

        if let data = configData,
           let config = try? JSONDecoder().decode(MinimalConfig.self, from: data) {
            sttProvider = config.sttProvider ?? "apple"
            sttProfileID = config.sttProfile ?? ""
            let lang = config.sttLanguage ?? "auto"
            sttLanguage = lang == "auto" ? nil : lang
        }

        // If provider is not apple, resolve from model profiles
        if !sttProfileID.isEmpty {
            if let data = configData,
               let config = try? JSONDecoder().decode(MinimalConfig.self, from: data),
               let profiles = config.modelProfiles {

                let profile = profiles.first { $0.id == sttProfileID && $0.capabilities.contains("stt") }
                    ?? profiles.first { $0.capabilities.contains("stt") }

                if let p = profile {
                    let apiKey = readAPIKey(for: p.id) ?? ""
                    let baseURL = p.baseURL ?? defaultBaseURL(for: p.provider)
                    kbSTTLog.info("🔌 Keyboard using STT: \(p.modelID) @ \(baseURL)")
                    return ("whisper", sttLanguage, apiKey, baseURL, p.modelID)
                }
            }
        }

        return ("apple", sttLanguage, "", "", "")
    }

    private func readAPIKey(for profileID: String) -> String? {
        // Read API key from Keychain — same service + key format as main app's KeychainStore
        let service = ConfigKeys.keychainService

        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: profileID,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess,
              let data = result as? Data,
              let key = String(data: data, encoding: .utf8) else {
            return nil
        }
        return key
    }

    private func defaultBaseURL(for provider: String) -> String {
        switch provider {
        case "openai": return "https://api.openai.com"
        case "groq": return "https://api.groq.com/openai"
        default: return "https://api.openai.com"
        }
    }

    // MARK: - Whisper Transcription

    private func transcribeWithWhisper(
        audioData: Data,
        language: String?,
        apiKey: String,
        baseURL: String,
        modelID: String
    ) async throws -> String {
        let urlString = "\(baseURL)/v1/audio/transcriptions"
        guard let url = URL(string: urlString) else {
            throw STTError.networkError("Invalid URL: \(urlString)")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 30
        if !apiKey.isEmpty {
            request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        }

        let boundary = UUID().uuidString
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()
        appendFormField(&body, boundary: boundary, name: "model", value: modelID)
        appendFormFile(&body, boundary: boundary, name: "file", filename: "audio.wav",
                       mimeType: "audio/wav", data: audioData)
        if let lang = language {
            appendFormField(&body, boundary: boundary, name: "language", value: lang)
        }
        body.append(Data("--\(boundary)--\r\n".utf8))
        request.httpBody = body

        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw STTError.networkError(error.localizedDescription)
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw STTError.parseError
        }

        guard httpResponse.statusCode == 200 else {
            throw STTError.networkError("HTTP \(httpResponse.statusCode)")
        }

        // Parse response
        struct WhisperResponse: Decodable { let text: String }
        if let decoded = try? JSONDecoder().decode(WhisperResponse.self, from: data),
           !decoded.text.isEmpty {
            return decoded.text
        }

        if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let text = json["text"] as? String ?? json["transcript"] as? String,
           !text.isEmpty {
            return text
        }

        throw STTError.noTranscript
    }

    // MARK: - Apple Speech Transcription

    private func transcribeWithApple(fileURL: URL, language: String?) async throws -> String {
        return try await withCheckedThrowingContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { status in
                kbSTTLog.info("🎤 Speech auth status: \(status.rawValue)")
                guard status == .authorized else {
                    continuation.resume(throwing: STTError.permissionDenied)
                    return
                }

                let locale = language.map { Locale(identifier: $0) } ?? .current
                let recognizer = SFSpeechRecognizer(locale: locale)
                    ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))

                guard let recognizer else {
                    continuation.resume(throwing: STTError.unknown("Speech recognizer unavailable"))
                    return
                }

                // Use URL request — lets Speech framework read the file directly
                // No manual WAV parsing, no buffer creation, no format issues
                let request = SFSpeechURLRecognitionRequest(url: fileURL)
                kbSTTLog.info("🎤 Recognizing file: \(fileURL.lastPathComponent)")

                var resumed = false
                recognizer.recognitionTask(with: request) { result, error in
                    guard !resumed else { return }
                    if let error {
                        resumed = true
                        kbSTTLog.error("🎤 Recognition error: \(error.localizedDescription)")
                        continuation.resume(throwing: STTError.unknown(error.localizedDescription))
                        return
                    }
                    if let result, result.isFinal {
                        resumed = true
                        let text = result.bestTranscription.formattedString
                        kbSTTLog.info("🎤 Result: \(text.prefix(50))...")
                        if text.isEmpty {
                            continuation.resume(throwing: STTError.noTranscript)
                        } else {
                            continuation.resume(returning: text)
                        }
                    }
                }
            }
        }
    }

    // MARK: - Multipart Helpers

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

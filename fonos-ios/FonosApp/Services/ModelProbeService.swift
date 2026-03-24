import Foundation
import os.log

private let log = Logger(subsystem: "com.fonos.ios", category: "ModelProbe")

/// Probes an OpenAI-compatible endpoint to discover available models.
/// Works with OMLX, Ollama, LM Studio, and any OpenAI-compatible API.
struct ModelProbeService {

    struct ProbeResult: Sendable {
        let models: [DiscoveredModel]
        let endpoint: String
        let provider: String
    }

    struct DiscoveredModel: Identifiable, Sendable {
        let id: String           // model ID from the API
        let name: String         // human-friendly name
        let capabilities: [String]  // ["llm", "stt"] inferred from model name/type
        var selected: Bool = true   // user can deselect before adding
    }

    /// Probe an endpoint for available models.
    /// - Parameters:
    ///   - baseURL: The base URL (e.g., "http://localhost:8000")
    ///   - apiKey: Optional API key for authentication
    ///   - provider: Provider identifier (e.g., "omlx", "ollama", "lmstudio")
    static func probe(baseURL: String, apiKey: String?, provider: String) async throws -> ProbeResult {
        let cleanURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/ "))
        guard let url = URL(string: "\(cleanURL)/v1/models") else {
            throw ProbeError.invalidURL(baseURL)
        }

        log.info("🔍 Probing \(url.absoluteString)...")

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.timeoutInterval = 10
        if let key = apiKey, !key.isEmpty {
            request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        }

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw ProbeError.connectionFailed("Invalid response")
        }

        guard httpResponse.statusCode == 200 else {
            throw ProbeError.connectionFailed("HTTP \(httpResponse.statusCode)")
        }

        // Parse OpenAI-compatible /v1/models response
        let modelsResponse = try JSONDecoder().decode(ModelsListResponse.self, from: data)
        log.info("🔍 Found \(modelsResponse.data.count) models")

        let discovered = modelsResponse.data.map { model in
            DiscoveredModel(
                id: model.id,
                name: humanReadableName(model.id),
                capabilities: inferCapabilities(modelID: model.id, provider: provider)
            )
        }

        return ProbeResult(models: discovered, endpoint: cleanURL, provider: provider)
    }

    /// Infer capabilities from model name patterns.
    private static func inferCapabilities(modelID: String, provider: String) -> [String] {
        let lower = modelID.lowercased()
        var caps: [String] = []

        // STT detection
        if lower.contains("whisper") || lower.contains("stt") || lower.contains("speech")
            || lower.contains("audio") || lower.contains("transcri") {
            caps.append("stt")
        }

        // LLM detection (most models are LLM by default)
        if lower.contains("llama") || lower.contains("mistral") || lower.contains("phi")
            || lower.contains("qwen") || lower.contains("gemma") || lower.contains("gpt")
            || lower.contains("chat") || lower.contains("instruct") || lower.contains("codellama")
            || lower.contains("deepseek") || lower.contains("yi-") || lower.contains("command")
            || lower.contains("claude") || lower.contains("llm") {
            caps.append("llm")
        }

        // If no specific pattern matched, default to LLM for most providers
        if caps.isEmpty {
            // Embedding models are not LLM
            if lower.contains("embed") || lower.contains("bge-") || lower.contains("e5-") {
                // Not LLM — skip
            } else {
                caps.append("llm")
            }
        }

        return caps
    }

    /// Convert model ID to human-readable name.
    private static func humanReadableName(_ id: String) -> String {
        // Remove common prefixes/paths
        var name = id
        if let lastSlash = name.lastIndex(of: "/") {
            name = String(name[name.index(after: lastSlash)...])
        }
        // Replace hyphens/underscores with spaces, capitalize
        name = name.replacingOccurrences(of: "-", with: " ")
            .replacingOccurrences(of: "_", with: " ")
        // Capitalize first letter of each word
        return name.split(separator: " ").map { word in
            word.prefix(1).uppercased() + word.dropFirst()
        }.joined(separator: " ")
    }
}

// MARK: - API Response Types

private struct ModelsListResponse: Decodable {
    let data: [ModelEntry]
}

private struct ModelEntry: Decodable {
    let id: String
    let object: String?
    let created: Int?
    let owned_by: String?
}

// MARK: - Errors

enum ProbeError: LocalizedError {
    case invalidURL(String)
    case connectionFailed(String)
    case noModelsFound

    var errorDescription: String? {
        switch self {
        case .invalidURL(let url): return "Invalid URL: \(url)"
        case .connectionFailed(let reason): return "Connection failed: \(reason)"
        case .noModelsFound: return "No models found at this endpoint"
        }
    }
}

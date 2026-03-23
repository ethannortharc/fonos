import Foundation

/// Processing mode for dictated text.
/// Each case represents a distinct transformation pipeline.
enum Mode: Codable, Equatable, Hashable, Sendable {
    case raw
    case polish
    case formal
    case translate(targetLanguage: String)
    case custom(systemPrompt: String, userTemplate: String, temperature: Double, maxTokens: Int)

    // MARK: - Codable

    private enum CodingKeys: String, CodingKey {
        case type
        case targetLanguage
        case systemPrompt
        case userTemplate
        case temperature
        case maxTokens
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "raw":
            self = .raw
        case "polish":
            self = .polish
        case "formal":
            self = .formal
        case "translate":
            let lang = try container.decode(String.self, forKey: .targetLanguage)
            self = .translate(targetLanguage: lang)
        case "custom":
            let prompt = try container.decode(String.self, forKey: .systemPrompt)
            let template = try container.decode(String.self, forKey: .userTemplate)
            let temp = try container.decode(Double.self, forKey: .temperature)
            let tokens = try container.decode(Int.self, forKey: .maxTokens)
            self = .custom(systemPrompt: prompt, userTemplate: template, temperature: temp, maxTokens: tokens)
        default:
            self = .raw
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .raw:
            try container.encode("raw", forKey: .type)
        case .polish:
            try container.encode("polish", forKey: .type)
        case .formal:
            try container.encode("formal", forKey: .type)
        case .translate(let lang):
            try container.encode("translate", forKey: .type)
            try container.encode(lang, forKey: .targetLanguage)
        case .custom(let prompt, let template, let temp, let tokens):
            try container.encode("custom", forKey: .type)
            try container.encode(prompt, forKey: .systemPrompt)
            try container.encode(template, forKey: .userTemplate)
            try container.encode(temp, forKey: .temperature)
            try container.encode(tokens, forKey: .maxTokens)
        }
    }

    // MARK: - Static convenience

    /// Returns all 5 built-in modes.
    static var builtInModes: [Mode] {
        [
            .raw,
            .polish,
            .formal,
            .translate(targetLanguage: "English"),
            .custom(
                systemPrompt: "You are a helpful assistant.",
                userTemplate: "{text}",
                temperature: 0.7,
                maxTokens: 1024
            )
        ]
    }

    // MARK: - Properties

    /// Stable string identifier for this mode.
    var id: String {
        switch self {
        case .raw: return "raw"
        case .polish: return "polish"
        case .formal: return "formal"
        case .translate: return "translate"
        case .custom: return "custom"
        }
    }

    /// SF Symbol name for this mode.
    var icon: String {
        switch self {
        case .raw: return "waveform"
        case .polish: return "sparkles"
        case .formal: return "briefcase"
        case .translate: return "globe"
        case .custom: return "slider.horizontal.3"
        }
    }

    /// Whether this mode requires an LLM to process.
    var requiresLLM: Bool {
        switch self {
        case .raw: return false
        case .polish, .formal, .translate, .custom: return true
        }
    }

    /// The system prompt used when calling the LLM.
    var systemPrompt: String {
        switch self {
        case .raw:
            return ""
        case .polish:
            return "Remove filler words and polish the text. Preserve the speaker's original tone and meaning. Clean up any disfluencies."
        case .formal:
            return "Rewrite the following text in a professional business style. Ensure formal tone and correct grammar."
        case .translate(let lang):
            return "Translate the following text to \(lang). Preserve the meaning and tone accurately."
        case .custom(let prompt, _, _, _):
            return prompt
        }
    }

    /// The user message template; use `applyTemplate(to:)` to substitute {text}.
    var userTemplate: String {
        switch self {
        case .raw:
            return "{text}"
        case .polish:
            return "{text}"
        case .formal:
            return "{text}"
        case .translate:
            return "{text}"
        case .custom(_, let template, _, _):
            return template
        }
    }

    /// Human-readable display name.
    var displayName: String {
        switch self {
        case .raw: return "Raw"
        case .polish: return "Polish"
        case .formal: return "Formal"
        case .translate(let lang): return "Translate (\(lang))"
        case .custom: return "Custom"
        }
    }

    // MARK: - Methods

    /// Substitutes the `{text}` placeholder in the userTemplate with the given input.
    func applyTemplate(to text: String) -> String {
        userTemplate.replacingOccurrences(of: "{text}", with: text)
    }
}

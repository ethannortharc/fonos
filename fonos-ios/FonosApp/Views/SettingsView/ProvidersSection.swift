import SwiftUI

// MARK: - Providers Section

/// Reusable section for configuring an STT or LLM provider.
/// Displays a provider picker, optional model/language picker, base URL field and API key field.
struct ProvidersSection: View {

    // MARK: - Configuration

    enum ProviderKind {
        case stt
        case llm
    }

    let kind: ProviderKind

    // Bindings into AppConfig
    @Binding var provider: String
    @Binding var baseURL: String

    // Local keychain-backed state (managed by parent)
    @Binding var apiKey: String

    // STT-only
    @Binding var language: String

    // LLM-only
    @Binding var modelID: String

    // MARK: - Styling

    private let amber = Color(hex: "#fbbf24")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.02)
    private let separator = Color.white.opacity(0.04)

    var body: some View {
        Section {
            providerRow
            secondaryRow
            if needsBaseURL { baseURLRow }
            if needsAPIKey { apiKeyRow }
        } header: {
            Text(sectionTitle.uppercased())
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(textDim)
                .textCase(nil)
        }
    }

    // MARK: - Provider Picker

    private var providerRow: some View {
        Picker("Provider", selection: $provider) {
            ForEach(providers, id: \.tag) { option in
                Text(option.label).tag(option.tag)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)
        .listRowSeparatorTint(separator)
    }

    // MARK: - Secondary Row (language for STT, model for LLM)

    @ViewBuilder
    private var secondaryRow: some View {
        switch kind {
        case .stt:
            Picker("Language", selection: $language) {
                Text("Auto-detect").tag("auto")
                Text("English").tag("en-US")
                Text("Spanish").tag("es-ES")
                Text("French").tag("fr-FR")
                Text("German").tag("de-DE")
                Text("Japanese").tag("ja-JP")
                Text("Chinese").tag("zh-CN")
                Text("Portuguese").tag("pt-BR")
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        case .llm:
            Picker("Model", selection: $modelID) {
                ForEach(modelsForProvider(provider), id: \.self) { model in
                    Text(model).tag(model)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        }
    }

    // MARK: - Base URL Row

    private var baseURLRow: some View {
        TextField("Base URL", text: $baseURL)
            .foregroundColor(textPrimary)
            .tint(amber)
            .autocapitalization(.none)
            .autocorrectionDisabled()
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
    }

    // MARK: - API Key Row

    private var apiKeyRow: some View {
        SecureField("API Key", text: $apiKey)
            .foregroundColor(textPrimary)
            .tint(amber)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
    }

    // MARK: - Helpers

    private var sectionTitle: String {
        switch kind {
        case .stt: return "Speech-to-Text"
        case .llm: return "LLM Processing"
        }
    }

    private struct ProviderOption {
        let label: String
        let tag: String
    }

    private var providers: [ProviderOption] {
        switch kind {
        case .stt:
            return [
                .init(label: "Apple Speech", tag: "apple"),
                .init(label: "OpenAI Whisper", tag: "whisper"),
                .init(label: "Fonos Server", tag: "fonos"),
                .init(label: "Custom", tag: "custom")
            ]
        case .llm:
            return [
                .init(label: "OpenAI", tag: "openai"),
                .init(label: "Anthropic", tag: "anthropic"),
                .init(label: "Fonos Server", tag: "fonos"),
                .init(label: "Custom", tag: "custom")
            ]
        }
    }

    private var needsBaseURL: Bool {
        switch kind {
        case .stt: return provider == "fonos" || provider == "custom"
        case .llm: return provider == "fonos" || provider == "custom"
        }
    }

    private var needsAPIKey: Bool {
        switch kind {
        case .stt: return provider != "apple"
        case .llm: return provider != "fonos"
        }
    }

    private func modelsForProvider(_ p: String) -> [String] {
        switch p {
        case "openai":    return ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"]
        case "anthropic": return ["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-3-5"]
        case "fonos":     return ["llama3", "mistral", "phi3"]
        default:          return ["default"]
        }
    }
}

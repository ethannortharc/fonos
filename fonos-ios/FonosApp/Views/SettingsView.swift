import SwiftUI

// MARK: - Settings View

/// Full settings screen with sections for STT, LLM, Recording, Destinations and History.
struct SettingsView: View {

    // MARK: - Config state

    @State private var config: AppConfig = {
        if let data = UserDefaults.standard.data(forKey: "app_config"),
           let decoded = try? JSONDecoder().decode(AppConfig.self, from: data) {
            return decoded
        }
        return AppConfig()
    }()

    // MARK: - Keychain

    private let keychainSTT = KeychainStore(service: "com.fonos.stt")
    private let keychainLLM = KeychainStore(service: "com.fonos.llm")

    // MARK: - Local keychain state

    @State private var sttAPIKey: String = ""
    @State private var llmAPIKey: String = ""

    // MARK: - Navigation state

    @State private var showAddDestination = false

    // MARK: - Colors

    private let bg = Color(hex: "#1a1917")
    private let cardBg = Color.white.opacity(0.02)
    private let cardBorder = Color.white.opacity(0.04)
    private let separator = Color.white.opacity(0.04)
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let amber = Color(hex: "#fbbf24")

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                List {
                    speechToTextSection
                    llmSection
                    recordingSection
                    destinationsSection
                    historySection
                }
                .listStyle(.insetGrouped)
                .scrollContentBackground(.hidden)
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.large)
        }
        .onAppear(perform: loadKeychainKeys)
        .onChange(of: config) { _, _ in
            saveConfig()
        }
        .sheet(isPresented: $showAddDestination) {
            AddDestinationSheet(config: $config)
        }
    }

    // MARK: - Speech-to-Text Section

    private var speechToTextSection: some View {
        Section {
            // Provider picker
            Picker("Provider", selection: $config.sttProvider) {
                Text("Apple Speech").tag("apple")
                Text("OpenAI Whisper").tag("whisper")
                Text("Fonos Server").tag("fonos")
                Text("Custom").tag("custom")
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // Language selector
            Picker("Language", selection: $config.sttLanguage) {
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

            // API Key (Keychain)
            if config.sttProvider != "apple" {
                SecureField("API Key", text: $sttAPIKey)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .listRowBackground(cardBg)
                    .listRowSeparatorTint(separator)
                    .onChange(of: sttAPIKey) { _, newValue in
                        try? keychainSTT.set(newValue, forKey: "api_key")
                    }
            }
        } header: {
            sectionHeader("Speech-to-Text")
        }
    }

    // MARK: - LLM Processing Section

    private var llmSection: some View {
        Section {
            // Provider picker
            Picker("Provider", selection: $config.llmProvider) {
                Text("OpenAI").tag("openai")
                Text("Anthropic").tag("anthropic")
                Text("Fonos Server").tag("fonos")
                Text("Custom").tag("custom")
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // Model selector — filtered by provider
            Picker("Model", selection: modelIDBinding) {
                ForEach(modelsForProvider(config.llmProvider), id: \.self) { model in
                    Text(model).tag(model)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // Base URL (for custom/self-hosted)
            if config.llmProvider == "fonos" || config.llmProvider == "custom" {
                TextField("Base URL", text: $config.llmBaseURL)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .autocapitalization(.none)
                    .autocorrectionDisabled()
                    .listRowBackground(cardBg)
                    .listRowSeparatorTint(separator)
            }

            // API Key (Keychain)
            if config.llmProvider != "fonos" {
                SecureField("API Key", text: $llmAPIKey)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .listRowBackground(cardBg)
                    .listRowSeparatorTint(separator)
                    .onChange(of: llmAPIKey) { _, newValue in
                        try? keychainLLM.set(newValue, forKey: "api_key")
                    }
            }
        } header: {
            sectionHeader("LLM Processing")
        }
    }

    // MARK: - Recording Section

    private var recordingSection: some View {
        Section {
            // Record mode picker
            Picker("Record Mode", selection: $config.recordMode) {
                Text("Tap to Record").tag(RecordMode.tap)
                Text("Hold to Record").tag(RecordMode.hold)
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // Auto-send toggle
            let autoSendEnabled = Binding<Bool>(
                get: { !config.autoSendDestination.isEmpty },
                set: { enabled in
                    if !enabled {
                        config.autoSendDestination = ""
                    } else if config.autoSendDestination.isEmpty,
                              let first = config.destinations.first {
                        config.autoSendDestination = first.id
                    }
                }
            )

            Toggle("Auto-send After Dictation", isOn: autoSendEnabled)
                .tint(amber)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)

            if !config.autoSendDestination.isEmpty {
                Picker("Destination", selection: $config.autoSendDestination) {
                    ForEach(config.destinations, id: \.id) { destination in
                        Text(destinationLabel(destination)).tag(destination.id)
                    }
                }
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
            }
        } header: {
            sectionHeader("Recording")
        }
    }

    // MARK: - Destinations Section

    private var destinationsSection: some View {
        Section {
            ForEach(Array(config.destinations.enumerated()), id: \.element.id) { index, destination in
                HStack(spacing: 12) {
                    Image(systemName: destinationIcon(destination))
                        .foregroundColor(amber)
                        .frame(width: 22)

                    Text(destinationLabel(destination))
                        .foregroundColor(textPrimary)

                    Spacer()

                    // Active indicator for the auto-send destination
                    if config.autoSendDestination == destination.id {
                        Circle()
                            .fill(Color(hex: "#fbbf24"))
                            .frame(width: 6, height: 6)
                    }
                }
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    // Prevent deleting clipboard (built-in)
                    if destination.id != "clipboard" {
                        Button(role: .destructive) {
                            config.destinations.remove(at: index)
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
                }
            }

            Button {
                showAddDestination = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "plus.circle.fill")
                        .foregroundColor(amber)
                    Text("Add Destination")
                        .foregroundColor(amber)
                }
            }
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Destinations")
        }
    }

    // MARK: - History Section

    private var historySection: some View {
        Section {
            Picker("Retention Period", selection: $config.historyRetentionDays) {
                Text("7 Days").tag(7)
                Text("30 Days").tag(30)
                Text("90 Days").tag(90)
                Text("Forever").tag(0)
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("History")
        }
    }

    // MARK: - Section Header

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    // MARK: - Helpers

    private func destinationLabel(_ destination: AnyTextDestination) -> String {
        switch destination.id {
        case "clipboard": return "Clipboard"
        case "messages":  return "Messages"
        case "url_scheme": return "URL Scheme"
        default:           return destination.id.capitalized
        }
    }

    private func destinationIcon(_ destination: AnyTextDestination) -> String {
        switch destination.id {
        case "clipboard":  return "doc.on.clipboard"
        case "messages":   return "message.fill"
        case "url_scheme": return "link"
        default:            return "arrow.up.forward.app"
        }
    }

    private func modelsForProvider(_ provider: String) -> [String] {
        switch provider {
        case "openai":    return ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"]
        case "anthropic": return ["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-3-5"]
        case "fonos":     return ["llama3", "mistral", "phi3"]
        default:          return ["default"]
        }
    }

    private var modelIDBinding: Binding<String> {
        Binding<String>(
            get: {
                config.modelProfiles.first?.modelID ?? modelsForProvider(config.llmProvider).first ?? ""
            },
            set: { newModel in
                if var profile = config.modelProfiles.first {
                    profile.modelID = newModel
                    config.modelProfiles[0] = profile
                } else {
                    config.modelProfiles = [
                        ModelProfile(
                            id: UUID().uuidString,
                            name: newModel,
                            provider: config.llmProvider,
                            modelID: newModel
                        )
                    ]
                }
            }
        )
    }

    // MARK: - Persistence

    private func saveConfig() {
        if let data = try? JSONEncoder().encode(config) {
            UserDefaults.standard.set(data, forKey: "app_config")
        }
    }

    private func loadKeychainKeys() {
        sttAPIKey = (try? keychainSTT.get("api_key")) ?? ""
        llmAPIKey = (try? keychainLLM.get("api_key")) ?? ""
    }
}

// MARK: - Add Destination Sheet

/// Sheet for adding a new destination.
struct AddDestinationSheet: View {
    @Binding var config: AppConfig
    @Environment(\.dismiss) private var dismiss

    @State private var selectedType: DestinationType = .urlScheme
    @State private var urlTemplate: String = ""

    private let amber = Color(hex: "#fbbf24")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let cardBg = Color.white.opacity(0.06)

    enum DestinationType: String, CaseIterable, Identifiable {
        case messages = "Messages"
        case urlScheme = "URL Scheme"
        var id: String { rawValue }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                Form {
                    Section {
                        Picker("Type", selection: $selectedType) {
                            ForEach(DestinationType.allCases) { type in
                                Text(type.rawValue).tag(type)
                            }
                        }
                        .foregroundColor(textPrimary)
                        .listRowBackground(cardBg)
                    } header: {
                        Text("DESTINATION TYPE")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(textPrimary.opacity(0.5))
                            .textCase(nil)
                    }

                    if selectedType == .urlScheme {
                        Section {
                            TextField("e.g. myapp://send?text={text}", text: $urlTemplate)
                                .foregroundColor(textPrimary)
                                .autocapitalization(.none)
                                .autocorrectionDisabled()
                                .listRowBackground(cardBg)
                        } header: {
                            Text("URL TEMPLATE")
                                .font(.system(size: 12, weight: .medium))
                                .foregroundColor(textPrimary.opacity(0.5))
                                .textCase(nil)
                        } footer: {
                            Text("Use {text} as a placeholder for the dictated text.")
                                .foregroundColor(textPrimary.opacity(0.4))
                        }
                    }
                }
                .scrollContentBackground(.hidden)
            }
            .navigationTitle("Add Destination")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundColor(amber)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        addDestination()
                        dismiss()
                    }
                    .foregroundColor(amber)
                    .disabled(!canAdd)
                }
            }
        }
    }

    private var canAdd: Bool {
        switch selectedType {
        case .messages:  return true
        case .urlScheme: return !urlTemplate.isEmpty
        }
    }

    private func addDestination() {
        switch selectedType {
        case .messages:
            let dest = AnyTextDestination(MessagesDestination())
            if !config.destinations.contains(where: { $0.id == dest.id }) {
                config.destinations.append(dest)
            }
        case .urlScheme:
            let dest = AnyTextDestination(URLSchemeDestination(template: urlTemplate))
            config.destinations.append(dest)
        }
    }
}

#Preview {
    SettingsView()
}

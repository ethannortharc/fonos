import SwiftUI

// MARK: - Models Tab

/// Settings tab for managing AI model profiles.
/// Provides default service assignment and full CRUD for model profiles.
struct ModelsTab: View {
    @Binding var config: AppConfig

    @State private var showAddModel = false
    @State private var editingProfile: ModelProfile?

    private let amber = Color(hex: "#fbbf24")
    private let green = Color(hex: "#86efac")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.02)
    private let cardBorder = Color.white.opacity(0.04)
    private let separator = Color.white.opacity(0.04)

    var body: some View {
        List {
            defaultServicesSection
            registeredModelsSection
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .sheet(isPresented: $showAddModel) {
            ModelProfileForm(onSave: { profile in
                config.modelProfiles.append(profile)
            })
        }
        .sheet(item: $editingProfile) { profile in
            ModelProfileForm(
                initialProfile: profile,
                onSave: { updated in
                    if let idx = config.modelProfiles.firstIndex(where: { $0.id == profile.id }) {
                        config.modelProfiles[idx] = updated
                    }
                }
            )
        }
    }

    // MARK: - Default Services Section

    private var defaultServicesSection: some View {
        Section {
            // STT default
            Picker(selection: $config.sttProfile) {
                Text("Not configured").tag("")
                ForEach(sttModels) { profile in
                    Text(profile.name).tag(profile.id)
                }
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: "waveform.circle.fill")
                        .foregroundColor(amber)
                        .frame(width: 22)
                    Text("STT Model")
                        .foregroundColor(textPrimary)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // LLM default
            Picker(selection: $config.llmProfile) {
                Text("Not configured").tag("")
                ForEach(llmModels) { profile in
                    Text(profile.name).tag(profile.id)
                }
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: "cpu.fill")
                        .foregroundColor(green)
                        .frame(width: 22)
                    Text("LLM Model")
                        .foregroundColor(textPrimary)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Default Services")
        }
    }

    // MARK: - Registered Models Section

    private var registeredModelsSection: some View {
        Section {
            if config.modelProfiles.isEmpty {
                Text("No models registered")
                    .foregroundColor(textDim)
                    .font(.system(size: 14))
                    .listRowBackground(cardBg)
                    .listRowSeparatorTint(separator)
            } else {
                ForEach(Array(config.modelProfiles.enumerated()), id: \.element.id) { index, profile in
                    ModelProfileRow(
                        profile: profile,
                        isDefaultSTT: config.sttProfile == profile.id,
                        isDefaultLLM: config.llmProfile == profile.id,
                        amber: amber,
                        green: green,
                        textPrimary: textPrimary,
                        textDim: textDim,
                        cardBg: cardBg,
                        separator: separator,
                        onTap: { editingProfile = profile }
                    )
                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                        Button(role: .destructive) {
                            deleteProfile(at: index, id: profile.id)
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
                }
            }

            // Add Model button
            Button {
                showAddModel = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "plus.circle.fill")
                        .foregroundColor(amber)
                    Text("Add Model")
                        .foregroundColor(amber)
                }
            }
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Registered Models")
        }
    }

    // MARK: - Helpers

    private var sttModels: [ModelProfile] {
        config.modelProfiles.filter(\.hasSTT)
    }

    private var llmModels: [ModelProfile] {
        config.modelProfiles.filter(\.hasLLM)
    }

    private func deleteProfile(at index: Int, id: String) {
        config.modelProfiles.remove(at: index)
        // Clear defaults that reference the deleted profile
        if config.sttProfile == id { config.sttProfile = "" }
        if config.llmProfile == id { config.llmProfile = "" }
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }
}

// MARK: - Model Profile Row

private struct ModelProfileRow: View {
    let profile: ModelProfile
    let isDefaultSTT: Bool
    let isDefaultLLM: Bool
    let amber: Color
    let green: Color
    let textPrimary: Color
    let textDim: Color
    let cardBg: Color
    let separator: Color
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 12) {
                // Provider icon
                providerIcon(for: profile.provider)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: 4) {
                    Text(profile.name)
                        .foregroundColor(textPrimary)
                        .font(.system(size: 15))

                    HStack(spacing: 6) {
                        Text("\(providerDisplayName(profile.provider)) · \(profile.modelID)")
                            .font(.system(size: 12))
                            .foregroundColor(textDim)
                    }

                    // Capability badges + default badges
                    HStack(spacing: 6) {
                        if profile.hasSTT {
                            badgeView("STT", color: amber)
                        }
                        if profile.hasLLM {
                            badgeView("LLM", color: green)
                        }
                        if isDefaultSTT {
                            badgeView("Default STT", color: amber.opacity(0.6))
                        }
                        if isDefaultLLM {
                            badgeView("Default LLM", color: green.opacity(0.6))
                        }
                    }
                }

                Spacer()

                Image(systemName: "chevron.right")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(textDim)
            }
            .padding(.vertical, 4)
        }
        .buttonStyle(PlainButtonStyle())
        .listRowBackground(cardBg)
        .listRowSeparatorTint(separator)
    }

    private func badgeView(_ label: String, color: Color) -> some View {
        Text(label)
            .font(.system(size: 10, weight: .semibold))
            .foregroundColor(color)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                RoundedRectangle(cornerRadius: 4)
                    .fill(color.opacity(0.12))
            )
    }

    private func providerIcon(for provider: String) -> some View {
        let (icon, color): (String, Color) = {
            switch provider {
            case "openai":    return ("brain", amber)
            case "anthropic": return ("sparkle", Color(hex: "#c084fc"))
            case "google":    return ("magnifyingglass.circle", Color(hex: "#60a5fa"))
            case "ollama":    return ("server.rack", green)
            case "lmstudio":  return ("laptopcomputer", green)
            default:          return ("network", textDim)
            }
        }()
        return Image(systemName: icon)
            .foregroundColor(color)
    }

    private func providerDisplayName(_ provider: String) -> String {
        switch provider {
        case "openai":    return "OpenAI"
        case "anthropic": return "Anthropic"
        case "google":    return "Google"
        case "ollama":    return "Ollama"
        case "lmstudio":  return "LM Studio"
        case "custom":    return "Custom"
        default:          return provider.capitalized
        }
    }
}

// MARK: - Model Profile Form

/// Sheet for adding or editing a model profile.
struct ModelProfileForm: View {
    var initialProfile: ModelProfile?
    let onSave: (ModelProfile) -> Void

    @Environment(\.dismiss) private var dismiss

    // Provider selection
    @State private var selectedProvider: String = "openai"

    // Form fields
    @State private var name: String = ""
    @State private var modelID: String = ""
    @State private var baseURL: String = ""
    @State private var apiKey: String = ""
    @State private var hasStt: Bool = false
    @State private var hasLlm: Bool = true

    private let amber = Color(hex: "#fbbf24")
    private let green = Color(hex: "#86efac")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.06)

    private let providers: [(id: String, label: String, icon: String)] = [
        ("openai", "OpenAI", "brain"),
        ("anthropic", "Anthropic", "sparkle"),
        ("google", "Google", "magnifyingglass.circle"),
        ("ollama", "Ollama", "server.rack"),
        ("lmstudio", "LM Studio", "laptopcomputer"),
        ("custom", "Custom", "network")
    ]

    private let keychainModels = KeychainStore(service: "com.fonos.models")

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                Form {
                    // Provider Picker (grid)
                    providerPickerSection

                    // Form fields
                    fieldsSection

                    // Capabilities
                    capabilitiesSection
                }
                .scrollContentBackground(.hidden)
            }
            .navigationTitle(initialProfile == nil ? "Add Model" : "Edit Model")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundColor(amber)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        save()
                        dismiss()
                    }
                    .foregroundColor(amber)
                    .disabled(!canSave)
                }
            }
        }
        .onAppear(perform: populateFromInitial)
        .onChange(of: selectedProvider) { _, _ in
            baseURL = defaultBaseURL(for: selectedProvider)
        }
    }

    // MARK: - Provider Picker

    private var providerPickerSection: some View {
        Section {
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 10) {
                ForEach(providers, id: \.id) { provider in
                    ProviderButton(
                        label: provider.label,
                        icon: provider.icon,
                        isSelected: selectedProvider == provider.id,
                        amber: amber,
                        textPrimary: textPrimary,
                        textDim: textDim
                    ) {
                        selectedProvider = provider.id
                    }
                }
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 8, leading: 4, bottom: 8, trailing: 4))
        } header: {
            formHeader("Provider")
        }
    }

    // MARK: - Fields Section

    private var fieldsSection: some View {
        Section {
            TextField("Name", text: $name)
                .foregroundColor(textPrimary)
                .tint(amber)
                .listRowBackground(cardBg)

            TextField("Model ID", text: $modelID)
                .foregroundColor(textPrimary)
                .tint(amber)
                .autocapitalization(.none)
                .autocorrectionDisabled()
                .font(.system(size: 15, design: .monospaced))
                .listRowBackground(cardBg)

            SecureField("API Key (stored in Keychain)", text: $apiKey)
                .foregroundColor(textPrimary)
                .tint(amber)
                .listRowBackground(cardBg)

            TextField("Base URL", text: $baseURL)
                .foregroundColor(textPrimary)
                .tint(amber)
                .autocapitalization(.none)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .listRowBackground(cardBg)
        } header: {
            formHeader("Configuration")
        }
    }

    // MARK: - Capabilities Section

    private var capabilitiesSection: some View {
        Section {
            Toggle("Speech-to-Text (STT)", isOn: $hasStt)
                .tint(amber)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)

            Toggle("Language Model (LLM)", isOn: $hasLlm)
                .tint(green)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
        } header: {
            formHeader("Capabilities")
        } footer: {
            Text("Enable the capabilities this model supports.")
                .foregroundColor(textDim)
        }
    }

    // MARK: - Helpers

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty &&
        !modelID.trimmingCharacters(in: .whitespaces).isEmpty &&
        (hasStt || hasLlm)
    }

    private func formHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    private func defaultBaseURL(for provider: String) -> String {
        switch provider {
        case "openai":    return "https://api.openai.com"
        case "anthropic": return "https://api.anthropic.com"
        case "google":    return "https://generativelanguage.googleapis.com"
        case "ollama":    return "http://localhost:11434"
        case "lmstudio":  return "http://localhost:1234"
        default:          return ""
        }
    }

    private func populateFromInitial() {
        guard let profile = initialProfile else {
            baseURL = defaultBaseURL(for: selectedProvider)
            return
        }
        selectedProvider = profile.provider
        name = profile.name
        modelID = profile.modelID
        baseURL = profile.baseURL ?? defaultBaseURL(for: profile.provider)
        hasStt = profile.hasSTT
        hasLlm = profile.hasLLM
        // Load API key from Keychain
        apiKey = (try? keychainModels.get(profile.id)) ?? ""
    }

    private func save() {
        var capabilities: [String] = []
        if hasStt { capabilities.append("stt") }
        if hasLlm { capabilities.append("llm") }

        let profileID: String
        if let existing = initialProfile {
            profileID = existing.id
        } else {
            profileID = "\(selectedProvider)-\(Int(Date().timeIntervalSince1970))"
        }

        // Save API key to Keychain
        if !apiKey.isEmpty {
            try? keychainModels.set(apiKey, forKey: profileID)
        }

        let profile = ModelProfile(
            id: profileID,
            name: name.trimmingCharacters(in: .whitespaces),
            provider: selectedProvider,
            modelID: modelID.trimmingCharacters(in: .whitespaces),
            baseURL: baseURL.isEmpty ? nil : baseURL,
            capabilities: capabilities
        )
        onSave(profile)
    }
}

// MARK: - Provider Button

private struct ProviderButton: View {
    let label: String
    let icon: String
    let isSelected: Bool
    let amber: Color
    let textPrimary: Color
    let textDim: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(systemName: icon)
                    .font(.system(size: 20, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? Color(hex: "#1a1917") : textPrimary.opacity(0.7))
                Text(label)
                    .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? Color(hex: "#1a1917") : textDim)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(isSelected ? amber : Color.white.opacity(0.06))
                    .overlay(
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(
                                isSelected ? Color.clear : Color.white.opacity(0.08),
                                lineWidth: 1
                            )
                    )
            )
            .shadow(
                color: isSelected ? amber.opacity(0.3) : .clear,
                radius: 6,
                x: 0,
                y: 2
            )
        }
        .buttonStyle(PlainButtonStyle())
        .animation(.spring(response: 0.25, dampingFraction: 0.75), value: isSelected)
    }
}

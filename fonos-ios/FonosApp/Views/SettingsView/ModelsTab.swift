import SwiftUI

// MARK: - Models Tab

/// Settings tab for managing AI model profiles.
/// Provides default service assignment and full CRUD for model profiles.
struct ModelsTab: View {
    @Binding var config: AppConfig

    @State private var showAddModel = false
    @State private var editingProfile: ModelProfile?
    @State private var showProbe = false
    @State private var probeURL = ""
    @State private var probeKey = ""
    @State private var probeProvider = "omlx"
    @State private var probeResult: ModelProbeService.ProbeResult?
    @State private var probing = false
    @State private var probeError: String?

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
        .sheet(isPresented: $showProbe) {
            ProbeSheet(
                probeURL: $probeURL,
                probeKey: $probeKey,
                probeProvider: $probeProvider,
                probing: $probing,
                probeError: $probeError,
                probeResult: $probeResult,
                onAddModels: { models, apiKey in
                    for model in models {
                        let profileID = "\(model.provider)-\(Int(Date().timeIntervalSince1970))-\(abs(model.id.hashValue) % 10000)"
                        let profile = ModelProfile(
                            id: profileID,
                            name: model.name,
                            provider: model.provider,
                            modelID: model.id,
                            baseURL: model.baseURL,  // URL from the model itself — guaranteed correct
                            capabilities: model.capabilities
                        )
                        print("✅ Adding model: \(profile.name), baseURL=\(profile.baseURL ?? "nil"), caps=\(profile.capabilities)")
                        config.modelProfiles.append(profile)
                        if !apiKey.isEmpty {
                            try? KeychainStore(service: "com.fonos.models").set(apiKey, forKey: profileID)
                        }
                    }
                }
            )
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
                    Text(profile.name)
                                .lineLimit(1)
                                .minimumScaleFactor(0.7)
                                .tag(profile.id)
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
                    Text(profile.name)
                                .lineLimit(1)
                                .minimumScaleFactor(0.7)
                                .tag(profile.id)
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
                    Text("Add Model Manually")
                        .foregroundColor(amber)
                }
            }
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            // Probe button
            Button {
                showProbe = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "antenna.radiowaves.left.and.right")
                        .foregroundColor(green)
                    Text("Probe Endpoint")
                        .foregroundColor(green)
                    Spacer()
                    Text("Auto-detect models")
                        .font(.system(size: 11))
                        .foregroundColor(textDim)
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
                        .font(.system(size: 15, weight: .medium))
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                        .truncationMode(.tail)

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
            case "omlx":      return ("apple.terminal", Color(hex: "#60a5fa"))
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
        case "omlx":      return "OMLX"
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
        ("omlx", "OMLX", "apple.terminal"),
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
            // Only reset URL to default when adding a NEW model (not editing existing)
            // When editing, the URL was already set from the profile in populateFromInitial
            if initialProfile == nil {
                baseURL = defaultBaseURL(for: selectedProvider)
            }
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
        case "omlx":      return "http://localhost:8000"
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

// MARK: - Probe Sheet

/// Sheet for probing an endpoint to discover available models.
private struct ProbeSheet: View {
    @Binding var probeURL: String
    @Binding var probeKey: String
    @Binding var probeProvider: String
    @Binding var probing: Bool
    @Binding var probeError: String?
    @Binding var probeResult: ModelProbeService.ProbeResult?
    let onAddModels: (_ models: [ModelProbeService.DiscoveredModel], _ apiKey: String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var selectedModels: Set<String> = []
    @State private var modelCapabilities: [String: Set<String>] = [:]  // modelID → user-chosen caps

    private let amber = Color(hex: "#fbbf24")
    private let green = Color(hex: "#86efac")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)

    private let probeProviders: [(id: String, label: String, defaultURL: String)] = [
        ("omlx", "OMLX", "http://localhost:8000"),
        ("ollama", "Ollama", "http://localhost:11434"),
        ("lmstudio", "LM Studio", "http://localhost:1234"),
        ("custom", "Custom", ""),
    ]

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                Form {
                    // Provider quick-select
                    Section {
                        ForEach(probeProviders, id: \.id) { p in
                            Button {
                                probeProvider = p.id
                                probeURL = p.defaultURL
                            } label: {
                                HStack {
                                    Text(p.label)
                                        .foregroundColor(probeProvider == p.id ? amber : textPrimary)
                                    Spacer()
                                    if probeProvider == p.id {
                                        Image(systemName: "checkmark")
                                            .foregroundColor(amber)
                                    }
                                }
                            }
                            .listRowBackground(Color.white.opacity(0.04))
                        }
                    } header: {
                        Text("PROVIDER").font(.system(size: 12, weight: .medium)).foregroundColor(textDim).textCase(nil)
                    }

                    // URL + Key
                    Section {
                        TextField("Endpoint URL", text: $probeURL)
                            .foregroundColor(textPrimary)
                            .tint(amber)
                            .autocapitalization(.none)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                            .listRowBackground(Color.white.opacity(0.06))

                        SecureField("API Key (optional)", text: $probeKey)
                            .foregroundColor(textPrimary)
                            .tint(amber)
                            .listRowBackground(Color.white.opacity(0.06))
                    } header: {
                        Text("ENDPOINT").font(.system(size: 12, weight: .medium)).foregroundColor(textDim).textCase(nil)
                    }

                    // Probe button
                    Section {
                        Button {
                            runProbe()
                        } label: {
                            HStack {
                                Spacer()
                                if probing {
                                    ProgressView().tint(.white)
                                } else {
                                    Image(systemName: "antenna.radiowaves.left.and.right")
                                    Text("Probe")
                                }
                                Spacer()
                            }
                            .foregroundColor(.white)
                            .padding(.vertical, 4)
                        }
                        .disabled(probeURL.isEmpty || probing)
                        .listRowBackground(probeURL.isEmpty ? Color.gray.opacity(0.3) : amber)
                    }

                    // Error
                    if let error = probeError {
                        Section {
                            Text(error)
                                .foregroundColor(Color.red)
                                .font(.system(size: 13))
                                .listRowBackground(Color.red.opacity(0.08))
                        }
                    }

                    // Results
                    if let result = probeResult {
                        Section {
                            ForEach(result.models) { model in
                                VStack(alignment: .leading, spacing: 8) {
                                    // Top row: checkbox + name
                                    HStack(spacing: 12) {
                                        Button { toggleModel(model.id) } label: {
                                            Image(systemName: selectedModels.contains(model.id) ? "checkmark.circle.fill" : "circle")
                                                .foregroundColor(selectedModels.contains(model.id) ? amber : textDim)
                                                .font(.system(size: 20))
                                        }
                                        .buttonStyle(.plain)

                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(model.name)
                                                .foregroundColor(textPrimary)
                                                .font(.system(size: 14, weight: .medium))
                                                .lineLimit(1)
                                                .minimumScaleFactor(0.7)
                                            Text(model.id)
                                                .foregroundColor(textDim)
                                                .font(.system(size: 11, design: .monospaced))
                                                .lineLimit(1)
                                                .truncationMode(.middle)
                                        }
                                    }

                                    // Capability row: auto-detected badges OR user toggles
                                    if model.autoDetected {
                                        // API provided type — show as read-only badges
                                        HStack(spacing: 4) {
                                            Image(systemName: "checkmark.seal.fill")
                                                .font(.system(size: 9))
                                                .foregroundColor(green.opacity(0.5))
                                            ForEach(model.capabilities, id: \.self) { cap in
                                                capBadge(cap)
                                            }
                                        }
                                    } else {
                                        // No API type — user chooses capabilities
                                        HStack(spacing: 6) {
                                            Text("Type:")
                                                .font(.system(size: 11))
                                                .foregroundColor(textDim)
                                            capToggle("STT", cap: "stt", modelID: model.id, color: amber)
                                            capToggle("LLM", cap: "llm", modelID: model.id, color: green)
                                            capToggle("TTS", cap: "tts", modelID: model.id, color: Color(hex: "#c084fc"))
                                        }
                                    }
                                }
                                .padding(.vertical, 4)
                                .listRowBackground(Color.white.opacity(0.04))
                            }
                        } header: {
                            HStack {
                                Text("DISCOVERED \(result.models.count) MODELS")
                                    .font(.system(size: 12, weight: .medium)).foregroundColor(green).textCase(nil)
                                Spacer()
                                Button {
                                    if selectedModels.count == result.models.count {
                                        selectedModels.removeAll()
                                    } else {
                                        selectedModels = Set(result.models.map(\.id))
                                    }
                                } label: {
                                    HStack(spacing: 3) {
                                        Image(systemName: selectedModels.count == result.models.count
                                              ? "checkmark.circle.fill" : "circle.dashed")
                                            .font(.system(size: 11))
                                        Text(selectedModels.count == result.models.count ? "Deselect All" : "Select All")
                                            .font(.system(size: 11, weight: .medium))
                                    }
                                    .foregroundColor(amber)
                                    .textCase(nil)
                                }
                            }
                        }

                        // Add selected — only models with at least one capability
                        Section {
                            Button {
                                let selected = result.models.filter { selectedModels.contains($0.id) }
                                // Apply user-selected capabilities
                                let resolved = selected.map { model -> ModelProbeService.DiscoveredModel in
                                    if model.autoDetected { return model }
                                    var m = model
                                    m.capabilities = Array(modelCapabilities[model.id, default: []])
                                    return m
                                }
                                // Filter out models with no capabilities chosen
                                let valid = resolved.filter { !$0.capabilities.isEmpty }
                                guard !valid.isEmpty else { return }
                                onAddModels(valid, probeKey)
                                dismiss()
                            } label: {
                                let validCount = validModelCount(in: result)
                                HStack {
                                    Spacer()
                                    if validCount == 0 && !selectedModels.isEmpty {
                                        Text("Select type for each model first")
                                            .fontWeight(.medium)
                                    } else {
                                        Text("Add \(validCount) Model\(validCount == 1 ? "" : "s")")
                                            .fontWeight(.semibold)
                                    }
                                    Spacer()
                                }
                                .foregroundColor(.white)
                                .padding(.vertical, 4)
                            }
                            .disabled(validModelCount(in: result) == 0)
                            .listRowBackground(validModelCount(in: result) == 0 ? Color.gray.opacity(0.3) : green)
                        }
                    }
                }
                .scrollContentBackground(.hidden)
            }
            .navigationTitle("Probe Endpoint")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundColor(amber)
                }
            }
        }
        .onAppear {
            if probeURL.isEmpty {
                probeURL = probeProviders.first?.defaultURL ?? ""
            }
        }
    }

    private func capBadge(_ cap: String) -> some View {
        let color: Color = cap == "stt" ? amber : cap == "llm" ? green : Color(hex: "#c084fc")
        return Text(cap.uppercased())
            .font(.system(size: 9, weight: .bold))
            .foregroundColor(color)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(RoundedRectangle(cornerRadius: 3).fill(color.opacity(0.12)))
    }

    private func capToggle(_ label: String, cap: String, modelID: String, color: Color) -> some View {
        let isOn = modelCapabilities[modelID, default: []].contains(cap)
        return Button {
            if isOn {
                modelCapabilities[modelID, default: []].remove(cap)
            } else {
                modelCapabilities[modelID, default: []].insert(cap)
            }
        } label: {
            Text(label)
                .font(.system(size: 11, weight: .semibold))
                .foregroundColor(isOn ? .white : color)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(isOn ? color : color.opacity(0.12))
                )
        }
        .buttonStyle(.plain)
    }

    /// Count models that are selected AND have at least one capability.
    private func validModelCount(in result: ModelProbeService.ProbeResult) -> Int {
        result.models.filter { model in
            guard selectedModels.contains(model.id) else { return false }
            if model.autoDetected { return !model.capabilities.isEmpty }
            return !(modelCapabilities[model.id, default: []].isEmpty)
        }.count
    }

    private func toggleModel(_ id: String) {
        if selectedModels.contains(id) {
            selectedModels.remove(id)
        } else {
            selectedModels.insert(id)
        }
    }

    private func runProbe() {
        probing = true
        probeError = nil
        probeResult = nil
        selectedModels = []

        Task {
            do {
                let result = try await ModelProbeService.probe(
                    baseURL: probeURL,
                    apiKey: probeKey.isEmpty ? nil : probeKey,
                    provider: probeProvider
                )
                await MainActor.run {
                    probeResult = result
                    selectedModels = Set(result.models.map(\.id))
                    probing = false
                }
            } catch {
                await MainActor.run {
                    probeError = error.localizedDescription
                    probing = false
                }
            }
        }
    }
}

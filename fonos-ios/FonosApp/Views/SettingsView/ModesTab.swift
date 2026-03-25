import SwiftUI

// MARK: - Modes Tab

/// Settings tab for managing processing mode configurations.
/// Shows built-in and custom modes with pipeline details, and allows add/edit/delete.
struct ModesTab: View {
    @Binding var config: AppConfig

    @State private var showAddMode = false
    @State private var editingModeConfig: ModeConfig?
    @State private var expandedID: String?

    private let amber = Color(hex: "#fbbf24")
    private let green = Color(hex: "#86efac")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.02)
    private let cardBorder = Color.white.opacity(0.04)
    private let separator = Color.white.opacity(0.04)

    var body: some View {
        List {
            Section {
                ForEach(Array(config.modeConfigs.enumerated()), id: \.element.id) { index, modeConfig in
                    ModeConfigRow(
                        modeConfig: modeConfig,
                        isExpanded: expandedID == modeConfig.id,
                        sttModel: sttModel(for: modeConfig),
                        llmModel: llmModel(for: modeConfig),
                        amber: amber,
                        green: green,
                        textPrimary: textPrimary,
                        textDim: textDim,
                        cardBg: cardBg,
                        separator: separator,
                        onTap: {
                            withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                                expandedID = expandedID == modeConfig.id ? nil : modeConfig.id
                            }
                        },
                        onEdit: {
                            editingModeConfig = modeConfig
                        }
                    )
                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                        if !modeConfig.isBuiltIn {
                            Button(role: .destructive) {
                                config.modeConfigs.removeAll { $0.id == modeConfig.id }
                                if config.activeModeID == modeConfig.id {
                                    config.activeModeID = "raw"
                                }
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                    }
                }

                // Add Mode button
                Button {
                    showAddMode = true
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "plus.circle.fill")
                            .foregroundColor(amber)
                        Text("Add Mode")
                            .foregroundColor(amber)
                    }
                }
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
            } header: {
                sectionHeader("Processing Modes")
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .sheet(isPresented: $showAddMode) {
            ModeConfigForm(
                availableSTTModels: sttModels,
                availableLLMModels: llmModels,
                onSave: { newConfig in
                    config.modeConfigs.append(newConfig)
                }
            )
        }
        .sheet(item: $editingModeConfig) { modeConfig in
            ModeConfigForm(
                initialConfig: modeConfig,
                availableSTTModels: sttModels,
                availableLLMModels: llmModels,
                onSave: { updated in
                    if let idx = config.modeConfigs.firstIndex(where: { $0.id == modeConfig.id }) {
                        config.modeConfigs[idx] = updated
                    }
                }
            )
        }
    }

    // MARK: - Helpers

    private var sttModels: [ModelProfile] {
        config.modelProfiles.filter(\.hasSTT)
    }

    private var llmModels: [ModelProfile] {
        config.modelProfiles.filter(\.hasLLM)
    }

    private func sttModel(for modeConfig: ModeConfig) -> ModelProfile? {
        let id = modeConfig.sttModelID ?? config.sttProfile
        return config.modelProfiles.first(where: { $0.id == id })
    }

    private func llmModel(for modeConfig: ModeConfig) -> ModelProfile? {
        let id = modeConfig.llmModelID ?? config.llmProfile
        return config.modelProfiles.first(where: { $0.id == id })
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }
}

// MARK: - Mode Config Row

private struct ModeConfigRow: View {
    let modeConfig: ModeConfig
    let isExpanded: Bool
    let sttModel: ModelProfile?
    let llmModel: ModelProfile?
    let amber: Color
    let green: Color
    let textPrimary: Color
    let textDim: Color
    let cardBg: Color
    let separator: Color
    let onTap: () -> Void
    let onEdit: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Main row
            Button(action: onTap) {
                HStack(spacing: 12) {
                    Image(systemName: modeConfig.icon)
                        .foregroundColor(amber)
                        .frame(width: 22)

                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 8) {
                            Text(modeConfig.name)
                                .foregroundColor(textPrimary)
                                .font(.system(size: 15))

                            if modeConfig.isBuiltIn {
                                Text("Built-in")
                                    .font(.system(size: 10, weight: .medium))
                                    .foregroundColor(textDim)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(
                                        RoundedRectangle(cornerRadius: 4)
                                            .fill(Color.white.opacity(0.06))
                                    )
                            }
                        }

                        Text(modeConfig.pipelineSummary(sttModel: sttModel, llmModel: llmModel))
                            .font(.system(size: 12))
                            .foregroundColor(textDim)
                    }

                    Spacer()

                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(textDim)
                }
                .padding(.vertical, 4)
            }
            .buttonStyle(PlainButtonStyle())

            // Expanded details
            if isExpanded {
                VStack(alignment: .leading, spacing: 12) {
                    Divider()
                        .background(Color.white.opacity(0.06))
                        .padding(.top, 4)

                    // Description
                    if !modeConfig.description.isEmpty {
                        Text(modeConfig.description)
                            .font(.system(size: 13))
                            .foregroundColor(textDim)
                    }

                    // Pipeline steps
                    VStack(alignment: .leading, spacing: 8) {
                        pipelineStep(
                            icon: "waveform",
                            label: "STT",
                            value: sttModel?.name ?? "Default",
                            color: amber
                        )

                        if modeConfig.mode.requiresLLM {
                            pipelineStep(
                                icon: "cpu",
                                label: "LLM",
                                value: llmModel?.name ?? "Default",
                                color: green
                            )
                        }
                    }

                    // Edit button (custom modes only)
                    HStack {
                        Spacer()
                        Button(action: onEdit) {
                            HStack(spacing: 6) {
                                Image(systemName: "pencil")
                                    .font(.system(size: 12))
                                Text("Edit")
                                    .font(.system(size: 13, weight: .medium))
                            }
                            .foregroundColor(amber)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .background(
                                RoundedRectangle(cornerRadius: 8)
                                    .fill(amber.opacity(0.1))
                            )
                        }
                        .buttonStyle(PlainButtonStyle())
                    }
                }
                .padding(.bottom, 8)
            }
        }
        .listRowBackground(cardBg)
        .listRowSeparatorTint(separator)
    }

    private func pipelineStep(icon: String, label: String, value: String, color: Color) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 11, weight: .semibold))
                .foregroundColor(color)
                .frame(width: 16)

            Text(label)
                .font(.system(size: 11, weight: .semibold))
                .foregroundColor(color)

            Text(value)
                .font(.system(size: 12))
                .foregroundColor(textDim)
        }
    }
}

// MARK: - Mode Config Form

/// Sheet for creating or editing a ModeConfig.
struct ModeConfigForm: View {
    var initialConfig: ModeConfig?
    let availableSTTModels: [ModelProfile]
    let availableLLMModels: [ModelProfile]
    let onSave: (ModeConfig) -> Void

    @Environment(\.dismiss) private var dismiss

    // Identity
    @State private var icon: String = "slider.horizontal.3"
    @State private var name: String = ""
    @State private var description: String = ""

    // STT
    @State private var sttModelID: String = ""  // "" = use default
    @State private var sttPrompt: String = ""
    @State private var sttTemperature: Double = 0.0
    @State private var showSTTAdvanced: Bool = false

    // LLM
    @State private var llmEnabled: Bool = true
    @State private var llmModelID: String = ""  // "" = use default
    @State private var systemPrompt: String = ""
    @State private var userTemplate: String = "{text}"
    @State private var llmTemperature: Double = 0.7
    @State private var maxTokens: Int = 1024
    @State private var showLLMAdvanced: Bool = false

    // Output
    @State private var outputLanguage: String = "auto"
    @State private var autoPaste: Bool = false

    private let amber = Color(hex: "#fbbf24")
    private let green = Color(hex: "#86efac")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.06)

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                Form {
                    identitySection
                    sttSection
                    llmSection
                    outputSection
                }
                .scrollContentBackground(.hidden)
            }
            .navigationTitle(initialConfig == nil ? "New Mode" : "Edit Mode")
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
    }

    // MARK: - Identity Section

    private var identitySection: some View {
        Section {
            HStack(spacing: 12) {
                Text("Icon")
                    .foregroundColor(textPrimary)
                    .frame(width: 50, alignment: .leading)
                TextField("SF Symbol or emoji", text: $icon)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .autocapitalization(.none)
                    .autocorrectionDisabled()
            }
            .listRowBackground(cardBg)

            TextField("Name", text: $name)
                .foregroundColor(textPrimary)
                .tint(amber)
                .listRowBackground(cardBg)

            TextField("Description (optional)", text: $description)
                .foregroundColor(textPrimary)
                .tint(amber)
                .listRowBackground(cardBg)
        } header: {
            formHeader("Identity")
        }
    }

    // MARK: - STT Section

    private var sttSection: some View {
        Section {
            // Model picker
            Picker(selection: $sttModelID) {
                Text("Default").tag("")
                ForEach(availableSTTModels) { model in
                    Text(model.name)
                                .lineLimit(1)
                                .minimumScaleFactor(0.7)
                                .tag(model.id)
                }
            } label: {
                Text("Model")
                    .foregroundColor(textPrimary)
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)

            // Advanced disclosure
            DisclosureGroup("Advanced", isExpanded: $showSTTAdvanced) {
                TextField("Whisper Prompt Hint", text: $sttPrompt)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .listRowBackground(cardBg)
                    .font(.system(size: 14))

                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text("Temperature")
                            .foregroundColor(textPrimary)
                        Spacer()
                        Text(String(format: "%.2f", sttTemperature))
                            .foregroundColor(textDim)
                            .monospacedDigit()
                    }
                    Slider(value: $sttTemperature, in: 0.0...1.0, step: 0.05)
                        .tint(amber)
                }
                .listRowBackground(cardBg)
            }
            .foregroundColor(textDim)
            .listRowBackground(cardBg)
        } header: {
            formHeader("Step 1: Speech-to-Text")
        }
    }

    // MARK: - LLM Section

    private var llmSection: some View {
        Section {
            Toggle("Enable LLM Processing", isOn: $llmEnabled)
                .tint(amber)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)

            if llmEnabled {
                Picker(selection: $llmModelID) {
                    Text("Default").tag("")
                    ForEach(availableLLMModels) { model in
                        Text(model.name)
                                .lineLimit(1)
                                .minimumScaleFactor(0.7)
                                .tag(model.id)
                    }
                } label: {
                    Text("Model")
                        .foregroundColor(textPrimary)
                }
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)

                // System prompt
                VStack(alignment: .leading, spacing: 6) {
                    Text("System Prompt")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundColor(textDim)
                    TextEditor(text: $systemPrompt)
                        .foregroundColor(textPrimary)
                        .tint(amber)
                        .frame(minHeight: 80)
                        .scrollContentBackground(.hidden)
                }
                .listRowBackground(cardBg)

                // User template
                TextField("User Template ({text})", text: $userTemplate)
                    .foregroundColor(textPrimary)
                    .tint(amber)
                    .font(.system(size: 14, design: .monospaced))
                    .listRowBackground(cardBg)

                // Advanced disclosure
                DisclosureGroup("Advanced", isExpanded: $showLLMAdvanced) {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text("Temperature")
                                .foregroundColor(textPrimary)
                            Spacer()
                            Text(String(format: "%.2f", llmTemperature))
                                .foregroundColor(textDim)
                                .monospacedDigit()
                        }
                        Slider(value: $llmTemperature, in: 0.0...2.0, step: 0.05)
                            .tint(amber)
                    }
                    .listRowBackground(cardBg)

                    Picker("Max Tokens", selection: $maxTokens) {
                        Text("256").tag(256)
                        Text("512").tag(512)
                        Text("1024").tag(1024)
                        Text("2048").tag(2048)
                        Text("4096").tag(4096)
                    }
                    .foregroundColor(textPrimary)
                    .listRowBackground(cardBg)

                    Picker("Output Language", selection: $outputLanguage) {
                        Text("Auto").tag("auto")
                        Text("English").tag("en")
                        Text("Spanish").tag("es")
                        Text("French").tag("fr")
                        Text("German").tag("de")
                        Text("Japanese").tag("ja")
                        Text("Chinese").tag("zh")
                    }
                    .foregroundColor(textPrimary)
                    .listRowBackground(cardBg)
                }
                .foregroundColor(textDim)
                .listRowBackground(cardBg)
            }
        } header: {
            formHeader("Step 2: LLM Processing")
        }
    }

    // MARK: - Output Section

    private var outputSection: some View {
        Section {
            Toggle("Auto-paste Result", isOn: $autoPaste)
                .tint(amber)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
        } header: {
            formHeader("Output")
        } footer: {
            Text("Automatically insert the result into the active text field.")
                .foregroundColor(textDim)
        }
    }

    // MARK: - Helpers

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty &&
        !icon.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func formHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    private func populateFromInitial() {
        guard let cfg = initialConfig else { return }
        icon = cfg.icon
        name = cfg.name
        description = cfg.description
        sttModelID = cfg.sttModelID ?? ""
        llmModelID = cfg.llmModelID ?? ""
        sttPrompt = cfg.sttPrompt
        sttTemperature = cfg.sttTemperature
        autoPaste = cfg.autoPaste
        outputLanguage = cfg.outputLanguage

        // Extract LLM settings from the mode
        if case .custom(let sp, let ut, let temp, let tokens) = cfg.mode {
            llmEnabled = true
            systemPrompt = sp
            userTemplate = ut
            llmTemperature = temp
            maxTokens = tokens
        } else {
            llmEnabled = cfg.mode.requiresLLM
            systemPrompt = cfg.mode.systemPrompt
            userTemplate = cfg.mode.userTemplate
        }
    }

    private func save() {
        let configID: String
        if let existing = initialConfig {
            configID = existing.id
        } else {
            configID = UUID().uuidString
        }

        // Build the Mode
        let mode: Mode
        if llmEnabled && !systemPrompt.isEmpty {
            mode = .custom(
                systemPrompt: systemPrompt,
                userTemplate: userTemplate.isEmpty ? "{text}" : userTemplate,
                temperature: llmTemperature,
                maxTokens: maxTokens
            )
        } else {
            mode = .raw
        }

        let newConfig = ModeConfig(
            id: configID,
            mode: mode,
            name: name.trimmingCharacters(in: .whitespaces),
            icon: icon.trimmingCharacters(in: .whitespaces),
            description: description.trimmingCharacters(in: .whitespaces),
            sttModelID: sttModelID.isEmpty ? nil : sttModelID,
            llmModelID: llmModelID.isEmpty ? nil : llmModelID,
            sttPrompt: sttPrompt,
            sttTemperature: sttTemperature,
            outputLanguage: outputLanguage,
            autoPaste: autoPaste,
            isBuiltIn: initialConfig?.isBuiltIn ?? false
        )
        onSave(newConfig)
    }
}

import SwiftUI

// MARK: - Modes Section

/// Settings section for managing processing modes.
/// Displays built-in modes (read-only) and allows creating / editing / deleting custom modes.
struct ModesSection: View {
    @Binding var modes: [Mode]

    @State private var showAddMode = false
    @State private var editingMode: Mode?

    private let amber = Color(hex: "#fbbf24")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.02)
    private let separator = Color.white.opacity(0.04)

    // Built-in modes have well-known ids
    private let builtInIDs: Set<String> = ["raw", "polish", "formal", "translate"]

    var body: some View {
        Section {
            ForEach(modes, id: \.id) { mode in
                ModeRow(mode: mode, isBuiltIn: isBuiltIn(mode)) {
                    // Tap on custom mode to edit
                    if !isBuiltIn(mode) {
                        editingMode = mode
                    }
                }
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                    if !isBuiltIn(mode) {
                        Button(role: .destructive) {
                            modes.removeAll { $0.id == mode.id }
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
                }
            }

            Button {
                showAddMode = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "plus.circle.fill")
                        .foregroundColor(amber)
                    Text("Add Custom Mode")
                        .foregroundColor(amber)
                }
            }
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            Text("PROCESSING MODES")
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(textDim)
                .textCase(nil)
        }
        .sheet(isPresented: $showAddMode) {
            CustomModeForm(onSave: { newMode in
                modes.append(newMode)
            })
        }
        .sheet(item: $editingMode) { mode in
            CustomModeForm(
                initialMode: mode,
                onSave: { updated in
                    if let idx = modes.firstIndex(where: { $0.id == mode.id }) {
                        modes[idx] = updated
                    }
                }
            )
        }
    }

    private func isBuiltIn(_ mode: Mode) -> Bool {
        builtInIDs.contains(mode.id)
    }
}

// MARK: - Mode Row

private struct ModeRow: View {
    let mode: Mode
    let isBuiltIn: Bool
    let onTap: () -> Void

    private let amber = Color(hex: "#fbbf24")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 12) {
                Image(systemName: mode.icon)
                    .foregroundColor(amber)
                    .frame(width: 22)

                VStack(alignment: .leading, spacing: 2) {
                    Text(mode.displayName)
                        .foregroundColor(textPrimary)
                        .font(.system(size: 15))
                    if mode.requiresLLM {
                        Text("Requires LLM")
                            .font(.system(size: 11))
                            .foregroundColor(textDim)
                    }
                }

                Spacer()

                if isBuiltIn {
                    Text("Built-in")
                        .font(.system(size: 11))
                        .foregroundColor(textDim)
                } else {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(textDim)
                }
            }
        }
        .buttonStyle(PlainButtonStyle())
    }
}

// MARK: - Custom Mode Form

/// Form for creating or editing a custom mode.
struct CustomModeForm: View {
    var initialMode: Mode?
    let onSave: (Mode) -> Void

    @Environment(\.dismiss) private var dismiss

    @State private var name: String = ""
    @State private var systemPrompt: String = ""
    @State private var userTemplate: String = "{text}"
    @State private var temperature: Double = 0.7
    @State private var maxTokens: Int = 1024

    private let amber = Color(hex: "#fbbf24")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.06)

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                Form {
                    // Name
                    Section {
                        TextField("Mode Name", text: $name)
                            .foregroundColor(textPrimary)
                            .tint(amber)
                            .listRowBackground(cardBg)
                    } header: {
                        formHeader("Name")
                    }

                    // System Prompt
                    Section {
                        TextEditor(text: $systemPrompt)
                            .foregroundColor(textPrimary)
                            .tint(amber)
                            .frame(minHeight: 100)
                            .listRowBackground(cardBg)
                    } header: {
                        formHeader("System Prompt")
                    }

                    // User Template
                    Section {
                        TextEditor(text: $userTemplate)
                            .foregroundColor(textPrimary)
                            .tint(amber)
                            .frame(minHeight: 60)
                            .listRowBackground(cardBg)
                    } header: {
                        formHeader("User Template")
                    } footer: {
                        Text("Use {text} as a placeholder for the dictated text.")
                            .foregroundColor(textDim)
                    }

                    // Temperature
                    Section {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("Temperature")
                                    .foregroundColor(textPrimary)
                                Spacer()
                                Text(String(format: "%.2f", temperature))
                                    .foregroundColor(textDim)
                                    .monospacedDigit()
                            }
                            Slider(value: $temperature, in: 0.0...2.0, step: 0.05)
                                .tint(amber)
                        }
                        .listRowBackground(cardBg)
                    } header: {
                        formHeader("Temperature")
                    }

                    // Max Tokens
                    Section {
                        Picker("Max Tokens", selection: $maxTokens) {
                            Text("256").tag(256)
                            Text("512").tag(512)
                            Text("1024").tag(1024)
                            Text("2048").tag(2048)
                            Text("4096").tag(4096)
                        }
                        .foregroundColor(textPrimary)
                        .listRowBackground(cardBg)
                    } header: {
                        formHeader("Max Tokens")
                    }
                }
                .scrollContentBackground(.hidden)
            }
            .navigationTitle(initialMode == nil ? "New Mode" : "Edit Mode")
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
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty ||
                              systemPrompt.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .onAppear(perform: populateFromInitial)
    }

    private func formHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    private func populateFromInitial() {
        guard case .custom(let sp, let ut, let temp, let mt) = initialMode else { return }
        systemPrompt = sp
        userTemplate = ut
        temperature = temp
        maxTokens = mt
        name = "Custom"
    }

    private func save() {
        let mode = Mode.custom(
            systemPrompt: systemPrompt,
            userTemplate: userTemplate,
            temperature: temperature,
            maxTokens: maxTokens
        )
        onSave(mode)
    }
}

// MARK: - Mode Identifiable conformance for sheet(item:)

extension Mode: Identifiable {}

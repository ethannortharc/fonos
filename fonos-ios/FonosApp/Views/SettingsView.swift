import SwiftUI

// MARK: - Settings View

/// Full settings screen with tabbed interface: Models | Modes | General.
struct SettingsView: View {

    // MARK: - Config state

    @State private var config: AppConfig = {
        if let data = UserDefaults.standard.data(forKey: "app_config"),
           let decoded = try? JSONDecoder().decode(AppConfig.self, from: data) {
            return decoded
        }
        return AppConfig()
    }()

    // MARK: - Tab state

    enum SettingsTab {
        case models, modes, general
    }

    @State private var selectedTab: SettingsTab = .models

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

                VStack(spacing: 0) {
                    // Tab bar
                    tabBar
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .padding(.bottom, 4)

                    Divider()
                        .background(separator)

                    // Tab content
                    tabContent
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.large)
        }
        .onChange(of: config) { _, _ in
            saveConfig()
        }
    }

    // MARK: - Tab Bar

    private var tabBar: some View {
        HStack(spacing: 0) {
            tabButton(title: "Models", tab: .models)
            tabButton(title: "Modes", tab: .modes)
            tabButton(title: "General", tab: .general)
        }
        .padding(4)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color.white.opacity(0.04))
        )
    }

    private func tabButton(title: String, tab: SettingsTab) -> some View {
        let isSelected = selectedTab == tab
        return Button {
            withAnimation(.spring(response: 0.25, dampingFraction: 0.8)) {
                selectedTab = tab
            }
        } label: {
            Text(title)
                .font(.system(size: 14, weight: isSelected ? .semibold : .regular))
                .foregroundColor(isSelected ? Color(hex: "#1a1917") : textDim)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
                .background(
                    RoundedRectangle(cornerRadius: 9)
                        .fill(isSelected ? amber : Color.clear)
                        .shadow(
                            color: isSelected ? amber.opacity(0.3) : .clear,
                            radius: 4,
                            x: 0,
                            y: 2
                        )
                )
        }
        .buttonStyle(PlainButtonStyle())
    }

    // MARK: - Tab Content

    @ViewBuilder
    private var tabContent: some View {
        switch selectedTab {
        case .models:
            ModelsTab(config: $config)
        case .modes:
            ModesTab(config: $config)
        case .general:
            generalTab
        }
    }

    // MARK: - General Tab

    private var generalTab: some View {
        List {
            recordingSection
            destinationsSection
            historySection
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
    }

    // MARK: - Recording Section

    private var recordingSection: some View {
        Section {
            Picker("Record Mode", selection: $config.recordMode) {
                Text("Tap to Record").tag(RecordMode.tap)
                Text("Hold to Record").tag(RecordMode.hold)
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

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
        DestinationsSection(config: $config)
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

    // MARK: - Persistence

    private func saveConfig() {
        if let data = try? JSONEncoder().encode(config) {
            UserDefaults.standard.set(data, forKey: "app_config")
        }
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

import SwiftUI
import SwiftData

// MARK: - NotebookSettingsView

/// Per-notebook settings — pipeline-indexed layout with a summary chip at top.
/// `① Speech-to-Text → ② LLM Processing → ③ Display` makes the recording flow legible.
struct NotebookSettingsView: View {

    // MARK: - Dependencies

    let notebook: NoteContainer
    let noteService: NoteService

    // MARK: - State

    @State private var name: String
    @State private var systemPrompt: String
    @State private var sttLanguage: String         // "" = Auto
    @State private var outputLanguage: String      // "" = Same as STT
    @State private var sttModelOverride: String    // "" = Default
    @State private var llmModelOverride: String    // "" = Default
    @State private var showRawInline: Bool
    @State private var siriPhrase: String

    @State private var showDeleteConfirmation = false
    @Environment(\.dismiss) private var dismiss

    // MARK: - Colors (Fonos warm-dark tokens)

    private let bg = Color(hex: "#1a1917")
    private let cardBg = Color.white.opacity(0.04)
    private let separator = Color.white.opacity(0.04)
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let amber = Color(hex: "#fbbf24")
    private let red = Color(hex: "#ef4444")

    // MARK: - Init

    init(notebook: NoteContainer, noteService: NoteService) {
        self.notebook = notebook
        self.noteService = noteService
        _name = State(initialValue: notebook.title)
        _systemPrompt = State(initialValue: notebook.systemPrompt)
        _sttLanguage = State(initialValue: notebook.sttLanguage ?? "")
        _outputLanguage = State(initialValue: notebook.outputLanguage ?? "")
        _sttModelOverride = State(initialValue: notebook.sttModelOverride ?? "")
        _llmModelOverride = State(initialValue: notebook.llmModelOverride ?? "")
        _showRawInline = State(initialValue: notebook.showRawInline)
        _siriPhrase = State(initialValue: notebook.siriPhrase ?? "")
    }

    // MARK: - Computed

    private var modelProfiles: [ModelProfile] {
        guard let data = UserDefaults.standard.data(forKey: "app_config"),
              let cfg = try? JSONDecoder().decode(AppConfig.self, from: data) else {
            return []
        }
        return cfg.modelProfiles
    }

    private var isQuickNote: Bool { notebook.title == "Quick Note" }

    /// A transient NoteContainer reflecting unsaved Picker state, so the chip
    /// updates as the user edits (without writing to the DB).
    private func stagingNotebook() -> NoteContainer {
        NoteContainer(
            title: name,
            sttModelOverride: sttModelOverride.isEmpty ? nil : sttModelOverride,
            llmModelOverride: llmModelOverride.isEmpty ? nil : llmModelOverride,
            systemPrompt: systemPrompt,
            sttLanguage: sttLanguage.isEmpty ? nil : sttLanguage,
            outputLanguage: outputLanguage.isEmpty ? nil : outputLanguage,
            showRawInline: showRawInline,
            siriPhrase: siriPhrase.isEmpty ? nil : siriPhrase
        )
    }

    // MARK: - Body

    var body: some View {
        ZStack {
            bg.ignoresSafeArea()

            List {
                Section {
                    pipelineChip
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 8, trailing: 16))
                        .listRowSeparator(.hidden)
                }
                generalSection
                sttSection
                llmSection
                displaySection
                shortcutSection
                dangerZoneSection
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
        }
        .navigationTitle("Notebook Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    saveChanges()
                    dismiss()
                }
                .foregroundColor(amber)
            }
        }
        .confirmationDialog(
            "Delete \"\(notebook.title)\"?",
            isPresented: $showDeleteConfirmation,
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                try? noteService.deleteNotebook(notebook.id)
                dismiss()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This will permanently delete the notebook and all its notes.")
        }
    }

    // MARK: - Pipeline chip

    private var pipelineChip: some View {
        let resolved = NotebookPipeline.resolve(stagingNotebook())
        let stt = SupportedLocale.displayName(for: resolved.sttLanguage)
        let outLang = resolved.llm.flatMap { $0.outputLanguage }
        let out = SupportedLocale.displayName(for: outLang)
        let middle = resolved.llm == nil ? "Raw" : "AI"

        return HStack(spacing: 8) {
            Text(stt)
            Text("→").foregroundColor(amber.opacity(0.6))
            Text(middle)
            if resolved.llm != nil {
                Text("→").foregroundColor(amber.opacity(0.6))
                Text(out)
            }
        }
        .font(.system(size: 12, design: .monospaced))
        .foregroundColor(textPrimary.opacity(0.85))
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(amber.opacity(0.08))
                .overlay(RoundedRectangle(cornerRadius: 10).stroke(amber.opacity(0.2), lineWidth: 1))
        )
    }

    // MARK: - Sections

    private var generalSection: some View {
        Section {
            TextField("Notebook Name", text: $name)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
        } header: { sectionHeader("General") }
    }

    private var sttSection: some View {
        Section {
            Picker("Language", selection: $sttLanguage) {
                Text("Auto").tag("")
                ForEach(SupportedLocale.all) { loc in
                    Text(loc.displayName).tag(loc.id)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)

            modelPicker(title: "STT Model", selection: $sttModelOverride, capability: "stt")
        } header: { numberedHeader(1, "Speech-to-Text") }
    }

    private var llmSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 6) {
                Text("System Prompt")
                    .font(.system(size: 13))
                    .foregroundColor(textDim)
                TextEditor(text: $systemPrompt)
                    .frame(minHeight: 120)
                    .scrollContentBackground(.hidden)
                    .padding(8)
                    .background(
                        RoundedRectangle(cornerRadius: 6)
                            .fill(Color.white.opacity(0.03))
                    )
                    .foregroundColor(textPrimary)
                    .font(.system(size: 14))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.sentences)
            }
            .padding(.vertical, 4)
            .listRowBackground(cardBg)

            Picker("Output Language", selection: $outputLanguage) {
                Text("Same as STT").tag("")
                ForEach(SupportedLocale.all) { loc in
                    Text(loc.displayName).tag(loc.id)
                }
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)

            modelPicker(title: "LLM Model", selection: $llmModelOverride, capability: "llm")
        } header: { numberedHeader(2, "LLM Processing") }
        footer: {
            Text("Leave System Prompt empty to skip LLM processing (Raw mode).")
                .foregroundColor(textDim).font(.system(size: 12))
        }
    }

    private var displaySection: some View {
        Section {
            Toggle("Show raw transcript inline", isOn: $showRawInline)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
                .tint(amber)
        } header: { numberedHeader(3, "Display") }
    }

    private var shortcutSection: some View {
        Section {
            HStack {
                Text("Siri Phrase").foregroundColor(textPrimary)
                Spacer()
                Text(siriPhrase.isEmpty ? "Record to \(name)" : siriPhrase)
                    .foregroundColor(textDim)
                    .font(.system(size: 13, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .listRowBackground(cardBg)

            Button {
                if let url = URL(string: "shortcuts://") {
                    UIApplication.shared.open(url)
                }
            } label: {
                HStack {
                    Spacer()
                    Text("Open in Shortcuts").foregroundColor(amber)
                    Spacer()
                }
            }
            .listRowBackground(cardBg)
        } header: { sectionHeader("Shortcut") }
        footer: {
            Text("Once added in Shortcuts.app, this notebook can be triggered via Siri, Back Tap, or the Action Button.")
                .foregroundColor(textDim).font(.system(size: 12))
        }
    }

    private var dangerZoneSection: some View {
        Section {
            Button {
                showDeleteConfirmation = true
            } label: {
                HStack {
                    Spacer()
                    Text("Delete Notebook").foregroundColor(isQuickNote ? textDim : red)
                    Spacer()
                }
            }
            .disabled(isQuickNote)
            .listRowBackground(cardBg)
        } header: { sectionHeader("Danger Zone") }
        footer: {
            if isQuickNote {
                Text("The Quick Note notebook cannot be deleted.")
                    .foregroundColor(textDim).font(.system(size: 12))
            }
        }
    }

    // MARK: - Helpers

    @ViewBuilder
    private func modelPicker(title: String, selection: Binding<String>, capability: String) -> some View {
        let profiles = modelProfiles.filter { $0.capabilities.contains(capability) }
        Picker(title, selection: selection) {
            Text("Default").tag("")
            ForEach(profiles, id: \.id) { profile in
                Text(profile.name).tag(profile.id)
            }
        }
        .foregroundColor(textPrimary)
        .listRowBackground(cardBg)
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    private func numberedHeader(_ n: Int, _ title: String) -> some View {
        HStack(spacing: 8) {
            Text("\(n)")
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .foregroundColor(amber)
                .frame(width: 18, height: 18)
                .background(Circle().fill(amber.opacity(0.15)))
            Text(title.uppercased())
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(textDim)
                .textCase(nil)
        }
    }

    // MARK: - Save

    private func saveChanges() {
        if name != notebook.title {
            noteService.renameNotebook(notebook.id, to: name)
        }
        noteService.updateNotebookConfigV2(
            notebook.id,
            systemPrompt: systemPrompt,
            sttLanguage: .some(sttLanguage.isEmpty ? nil : sttLanguage),
            outputLanguage: .some(outputLanguage.isEmpty ? nil : outputLanguage),
            sttModelOverride: .some(sttModelOverride.isEmpty ? nil : sttModelOverride),
            llmModelOverride: .some(llmModelOverride.isEmpty ? nil : llmModelOverride),
            showRawInline: showRawInline,
            siriPhrase: .some(siriPhrase.isEmpty ? nil : siriPhrase)
        )
    }
}

#Preview {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    let container = try! ModelContainer(for: schema, configurations: [config])
    let service = NoteService(modelContainer: container)
    let notebook = service.createNotebook(title: "My Notebook")
    NavigationStack {
        NotebookSettingsView(notebook: notebook, noteService: service)
    }
}

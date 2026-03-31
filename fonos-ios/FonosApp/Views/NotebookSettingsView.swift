import SwiftUI
import SwiftData

// MARK: - NotebookSettingsView

/// Per-notebook settings screen accessible from the notebook detail's gear button.
struct NotebookSettingsView: View {

    // MARK: - Dependencies

    let notebook: NoteContainer
    let noteService: NoteService

    // MARK: - State

    @State private var name: String
    @State private var processingMode: String
    @State private var sttModelOverride: String
    @State private var llmModelOverride: String
    @State private var customPrompt: String

    @State private var showDeleteConfirmation = false
    @Environment(\.dismiss) private var dismiss

    // MARK: - Colors

    private let bg = Color(hex: "#1a1917")
    private let cardBg = Color.white.opacity(0.02)
    private let cardBorder = Color.white.opacity(0.04)
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
        _processingMode = State(initialValue: notebook.processingMode)
        _sttModelOverride = State(initialValue: notebook.sttModelOverride ?? "")
        _llmModelOverride = State(initialValue: notebook.llmModelOverride ?? "")
        _customPrompt = State(initialValue: notebook.customPrompt ?? "")
    }

    // MARK: - Body

    var body: some View {
        ZStack {
            bg.ignoresSafeArea()

            List {
                generalSection
                processingSection
                modelOverridesSection
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

    // MARK: - Sections

    private var generalSection: some View {
        Section {
            TextField("Notebook Name", text: $name)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("General")
        }
    }

    private var processingSection: some View {
        Section {
            Picker("Mode", selection: $processingMode) {
                Text("Raw").tag("raw")
                Text("Light Polish").tag("light_polish")
                Text("Summarize").tag("summarize")
            }
            .foregroundColor(textPrimary)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)

            TextField("Custom Prompt (optional)", text: $customPrompt, axis: .vertical)
                .lineLimit(3...6)
                .foregroundColor(textPrimary)
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Processing")
        }
    }

    private var modelOverridesSection: some View {
        Section {
            TextField("STT Model Override (optional)", text: $sttModelOverride)
                .foregroundColor(textPrimary)
                .autocapitalization(.none)
                .autocorrectionDisabled()
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)

            TextField("LLM Model Override (optional)", text: $llmModelOverride)
                .foregroundColor(textPrimary)
                .autocapitalization(.none)
                .autocorrectionDisabled()
                .listRowBackground(cardBg)
                .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Model Overrides")
        } footer: {
            Text("Leave blank to use the app default model.")
                .foregroundColor(textDim)
                .font(.system(size: 12))
        }
    }

    private var dangerZoneSection: some View {
        Section {
            Button {
                showDeleteConfirmation = true
            } label: {
                HStack {
                    Spacer()
                    Text("Delete Notebook")
                        .foregroundColor(isQuickNote ? textDim : red)
                    Spacer()
                }
            }
            .disabled(isQuickNote)
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            sectionHeader("Danger Zone")
        } footer: {
            if isQuickNote {
                Text("The Quick Note notebook cannot be deleted.")
                    .foregroundColor(textDim)
                    .font(.system(size: 12))
            }
        }
    }

    // MARK: - Helpers

    private var isQuickNote: Bool {
        notebook.title == "Quick Note"
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.system(size: 12, weight: .medium))
            .foregroundColor(textDim)
            .textCase(nil)
    }

    private func saveChanges() {
        // Update name if changed
        if name != notebook.title {
            noteService.renameNotebook(notebook.id, to: name)
        }
        // Update per-notebook config
        noteService.updateNotebookConfig(
            notebook.id,
            processingMode: processingMode,
            sttModelOverride: sttModelOverride.isEmpty ? nil : sttModelOverride,
            llmModelOverride: llmModelOverride.isEmpty ? nil : llmModelOverride,
            customPrompt: customPrompt.isEmpty ? nil : customPrompt
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

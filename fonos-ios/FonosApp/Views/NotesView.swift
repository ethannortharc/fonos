import SwiftUI
import SwiftData

/// Notebook grid view — the main Notes tab content.
struct NotesView: View {

    @StateObject private var noteViewModel: NoteViewModel
    private let noteService: NoteService

    @State private var notebooks: [NoteContainer] = []
    @State private var showNewNotebookSheet = false
    @State private var newNotebookTitle = ""
    @State private var selectedNotebook: NoteContainer? = nil

    // MARK: - Init

    /// Default init for use in ContentView — creates its own NoteService.
    init() {
        // Use an in-memory container for preview/default initialisation.
        // In production, ContentView passes a real container.
        let schema = Schema([NoteContainer.self, NoteEntry.self])
        let config = ModelConfiguration(isStoredInMemoryOnly: true)
        let container = (try? ModelContainer(for: schema, configurations: [config]))
            ?? { fatalError("Cannot create default ModelContainer") }()
        let service = NoteService(modelContainer: container)
        self._noteViewModel = StateObject(wrappedValue: NoteViewModel(noteService: service))
        self.noteService = service
    }

    init(noteService: NoteService, noteViewModel: NoteViewModel) {
        self.noteService = noteService
        self._noteViewModel = StateObject(wrappedValue: noteViewModel)
    }

    // MARK: - Body

    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917")
                    .ignoresSafeArea()

                ScrollView {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 160), spacing: 12)],
                        spacing: 12
                    ) {
                        ForEach(notebooks, id: \.id) { notebook in
                            NavigationLink(destination: NotebookDetailView(
                                notebook: notebook,
                                noteService: noteService,
                                noteViewModel: noteViewModel
                            )) {
                                NotebookCard(notebook: notebook, entryCount: noteService.entryCount(for: notebook.id))
                            }
                            .buttonStyle(PlainButtonStyle())
                        }

                        // New Notebook button
                        newNotebookCard
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Notes")
            .navigationBarTitleDisplayMode(.large)
            .toolbarColorScheme(.dark, for: .navigationBar)
            .onAppear { reloadNotebooks() }
            .sheet(isPresented: $showNewNotebookSheet, onDismiss: reloadNotebooks) {
                NewNotebookSheet(
                    title: $newNotebookTitle,
                    onCreate: { title, template in
                        let nb = noteService.createNotebook(title: title)
                        if !template.systemPromptSeed.isEmpty {
                            noteService.updateNotebookConfigV2(nb.id, systemPrompt: template.systemPromptSeed)
                        }
                        showNewNotebookSheet = false
                        reloadNotebooks()
                    },
                    onCancel: { showNewNotebookSheet = false }
                )
            }
        }
        .preferredColorScheme(.dark)
    }

    // MARK: - New Notebook Card

    private var newNotebookCard: some View {
        Button {
            newNotebookTitle = ""
            showNewNotebookSheet = true
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: "plus")
                    .font(.system(size: 28, weight: .light))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                Text("New Notebook")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
            }
            .frame(maxWidth: .infinity, minHeight: 120, alignment: .center)
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .strokeBorder(
                        Color(hex: "#fafaf9").opacity(0.15),
                        style: StrokeStyle(lineWidth: 1.5, dash: [6, 4])
                    )
            )
        }
    }

    // MARK: - Helpers

    private func reloadNotebooks() {
        notebooks = noteService.allNotebooks()
    }
}

// MARK: - NotebookCard

private struct NotebookCard: View {
    let notebook: NoteContainer
    let entryCount: Int

    private var isQuickNote: Bool { notebook.title == "Quick Note" }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: "book.closed.fill")
                    .font(.system(size: 20))
                    .foregroundColor(isQuickNote ? Color(hex: "#fbbf24") : Color(hex: "#fafaf9").opacity(0.6))
                Spacer()
            }

            Spacer(minLength: 4)

            Text(notebook.title)
                .font(.system(size: 15, weight: .semibold))
                .foregroundColor(Color(hex: "#fafaf9"))
                .lineLimit(2)
                .multilineTextAlignment(.leading)

            HStack {
                Text("\(entryCount) \(entryCount == 1 ? "entry" : "entries")")
                    .font(.system(size: 11))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                Spacer()
                modeBadge
            }
        }
        .padding(14)
        .frame(minHeight: 120, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(Color(hex: "#fafaf9").opacity(0.04))
                .overlay(
                    RoundedRectangle(cornerRadius: 14)
                        .strokeBorder(
                            isQuickNote ? Color(hex: "#fbbf24").opacity(0.5) : Color(hex: "#fafaf9").opacity(0.08),
                            lineWidth: isQuickNote ? 1.5 : 1
                        )
                )
        )
    }

    @ViewBuilder
    private var modeBadge: some View {
        Text(notebook.processingMode)
            .font(.system(size: 9, weight: .semibold, design: .monospaced))
            .foregroundColor(Color(hex: "#fbbf24").opacity(0.8))
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(
                Capsule()
                    .fill(Color(hex: "#fbbf24").opacity(0.1))
            )
    }
}

// MARK: - NewNotebookSheet

private struct NewNotebookSheet: View {
    @Binding var title: String
    let onCreate: (String, NotebookTemplate) -> Void
    let onCancel: () -> Void

    @State private var selectedTemplate: NotebookTemplate = .raw

    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917").ignoresSafeArea()
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Notebook Name")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .textCase(.uppercase)
                        TextField("Notebook name", text: $title)
                            .textFieldStyle(.roundedBorder)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Starting Template")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .textCase(.uppercase)
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 10) {
                                ForEach(NotebookTemplate.allCases) { tpl in
                                    templateChip(tpl)
                                }
                            }
                        }
                        Text(selectedTemplate.systemPromptSeed.isEmpty
                             ? "No LLM processing (raw transcripts only)."
                             : selectedTemplate.systemPromptSeed)
                            .font(.system(size: 12))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                            .padding(.top, 4)
                    }

                    Spacer()
                }
                .padding(20)
            }
            .navigationTitle("New Notebook")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onCancel() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { onCreate(title, selectedTemplate) }
                        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private func templateChip(_ tpl: NotebookTemplate) -> some View {
        let isSelected = tpl == selectedTemplate
        return Button {
            selectedTemplate = tpl
        } label: {
            HStack(spacing: 6) {
                Image(systemName: tpl.symbolName)
                    .font(.system(size: 12, weight: .medium))
                Text(tpl.displayName)
                    .font(.system(size: 13, weight: .medium))
            }
            .foregroundColor(isSelected ? Color(hex: "#1a1917") : Color(hex: "#fafaf9"))
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isSelected ? Color(hex: "#fbbf24") : Color.white.opacity(0.05))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(isSelected ? Color.clear : Color.white.opacity(0.1), lineWidth: 1)
                    )
            )
        }
        .buttonStyle(.plain)
    }
}

#Preview {
    NotesView()
}

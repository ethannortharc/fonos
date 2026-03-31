import SwiftUI

/// Entry list within a notebook.
struct NotebookDetailView: View {
    let notebook: NoteContainer
    let noteService: NoteService
    @ObservedObject var noteViewModel: NoteViewModel

    @State private var entries: [NoteEntry] = []
    @State private var showRecordSheet = false

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            Color(hex: "#1a1917")
                .ignoresSafeArea()

            Group {
                if entries.isEmpty {
                    emptyState
                } else {
                    entriesList
                }
            }

            // Floating action button
            Button {
                showRecordSheet = true
            } label: {
                ZStack {
                    Circle()
                        .fill(Color(hex: "#fbbf24"))
                        .frame(width: 60, height: 60)
                        .shadow(color: Color(hex: "#fbbf24").opacity(0.4), radius: 10, x: 0, y: 4)
                    Image(systemName: "mic.fill")
                        .font(.system(size: 24, weight: .medium))
                        .foregroundColor(Color(hex: "#1a1917"))
                }
            }
            .padding(24)
        }
        .navigationTitle(notebook.title)
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                NavigationLink(destination: NotebookSettingsView(notebook: notebook, noteService: noteService)) {
                    Image(systemName: "gear")
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.7))
                }
            }
        }
        .onAppear { reloadEntries() }
        .sheet(isPresented: $showRecordSheet, onDismiss: reloadEntries) {
            RecordNoteSheet(
                notebook: notebook,
                noteViewModel: noteViewModel,
                onDismiss: { showRecordSheet = false }
            )
        }
    }

    // MARK: - Empty State

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "mic.circle")
                .font(.system(size: 56, weight: .light))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.2))
            Text("No entries yet")
                .font(.system(size: 17, weight: .medium))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
            Text("Tap the mic button to record your first note")
                .font(.system(size: 14))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.3))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Entries List

    private var entriesList: some View {
        List {
            ForEach(entries, id: \.id) { entry in
                EntryRow(entry: entry)
                    .listRowBackground(Color(hex: "#1a1917"))
                    .listRowSeparatorTint(Color(hex: "#fafaf9").opacity(0.08))
            }
            .onDelete { indexSet in
                indexSet.forEach { i in
                    noteService.deleteEntry(entries[i].id)
                }
                reloadEntries()
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    // MARK: - Helpers

    private func reloadEntries() {
        entries = noteService.entriesForNotebook(notebook.id)
    }
}

// MARK: - EntryRow

private struct EntryRow: View {
    let entry: NoteEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(entry.createdAt, style: .time)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                Spacer()
                modeBadge
            }

            Text(entry.processedText ?? entry.rawText)
                .font(.system(size: 15))
                .foregroundColor(Color(hex: "#fafaf9"))
                .lineLimit(4)
                .multilineTextAlignment(.leading)

            if let processed = entry.processedText, processed != entry.rawText {
                Text(entry.rawText)
                    .font(.system(size: 12))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.35))
                    .lineLimit(2)
            }
        }
        .padding(.vertical, 8)
    }

    private var modeBadge: some View {
        Text(entry.mode)
            .font(.system(size: 9, weight: .semibold, design: .monospaced))
            .foregroundColor(Color(hex: "#fbbf24").opacity(0.8))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Capsule().fill(Color(hex: "#fbbf24").opacity(0.1)))
    }
}


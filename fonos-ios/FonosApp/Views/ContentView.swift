import SwiftUI
import SwiftData

struct ContentView: View {
    @StateObject private var dictationViewModel = DictationViewModel()
    private let noteService: NoteService
    @StateObject private var noteViewModel: NoteViewModel

    init(modelContainer: ModelContainer) {
        let service = NoteService(modelContainer: modelContainer)
        self.noteService = service
        self._noteViewModel = StateObject(wrappedValue: NoteViewModel(noteService: service))
    }

    /// Convenience init for previews/tests — uses in-memory container.
    init() {
        let container = try! ModelContainer(
            for: NoteContainer.self, NoteEntry.self, DictationSession.self,
            configurations: ModelConfiguration(isStoredInMemoryOnly: true)
        )
        self.init(modelContainer: container)
    }

    var body: some View {
        TabView {
            DictationView(viewModel: dictationViewModel)
                .tabItem {
                    Label("Dictate", systemImage: "mic.fill")
                }

            HistoryView()
                .tabItem {
                    Label("History", systemImage: "clock.fill")
                }

            NotesView(noteService: noteService, noteViewModel: noteViewModel)
                .tabItem {
                    Label("Notes", systemImage: "note.text")
                }

            SettingsView(config: Binding(
                get: { dictationViewModel.config },
                set: { newConfig in
                    dictationViewModel.config = newConfig
                    if let data = try? JSONEncoder().encode(newConfig) {
                        UserDefaults.standard.set(data, forKey: "app_config")
                    }
                }
            ))
            .tabItem {
                Label("Settings", systemImage: "gear")
            }
        }
        .preferredColorScheme(.dark)
        .tint(Color(hex: "#fbbf24"))
        .onAppear {
            if let data = UserDefaults.standard.data(forKey: "app_config"),
               let saved = try? JSONDecoder().decode(AppConfig.self, from: data) {
                dictationViewModel.config = saved
            }
        }
    }
}

#Preview {
    ContentView()
        .modelContainer(for: [DictationSession.self, NoteContainer.self, NoteEntry.self], inMemory: true)
}

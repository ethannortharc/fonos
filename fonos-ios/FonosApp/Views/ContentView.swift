import SwiftUI

struct ContentView: View {
    @StateObject private var dictationViewModel = DictationViewModel()

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

            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gear")
                }
        }
        .preferredColorScheme(.dark)
        .tint(Color(hex: "#fbbf24"))
    }
}

#Preview {
    ContentView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

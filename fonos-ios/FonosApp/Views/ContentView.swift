import SwiftUI

struct ContentView: View {
    var body: some View {
        TabView {
            DictationView()
                .tabItem {
                    Label("Dictation", systemImage: "mic.fill")
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
    }
}

#Preview {
    ContentView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

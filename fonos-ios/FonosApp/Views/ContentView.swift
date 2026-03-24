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

            SettingsView(config: Binding(
                get: { dictationViewModel.config },
                set: { newConfig in
                    dictationViewModel.config = newConfig
                    // Persist to UserDefaults
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
            // Load saved config into view model
            if let data = UserDefaults.standard.data(forKey: "app_config"),
               let saved = try? JSONDecoder().decode(AppConfig.self, from: data) {
                dictationViewModel.config = saved
            }
        }
    }
}

#Preview {
    ContentView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

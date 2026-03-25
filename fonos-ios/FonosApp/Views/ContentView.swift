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
                    // Persist to UserDefaults (standard + App Group for keyboard extension)
                    if let data = try? JSONEncoder().encode(newConfig) {
                        UserDefaults.standard.set(data, forKey: "app_config")
                        UserDefaults(suiteName: "group.com.fonos.ios")?.set(data, forKey: "app_config")
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
            // Try App Group first, fall back to standard UserDefaults
            let appGroupDefaults = UserDefaults(suiteName: "group.com.fonos.ios")
            let data = appGroupDefaults?.data(forKey: "app_config")
                ?? UserDefaults.standard.data(forKey: "app_config")
            if let data,
               let saved = try? JSONDecoder().decode(AppConfig.self, from: data) {
                dictationViewModel.config = saved
                // Mirror to App Group for keyboard extension access
                appGroupDefaults?.set(data, forKey: "app_config")
            }
        }
    }
}

#Preview {
    ContentView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

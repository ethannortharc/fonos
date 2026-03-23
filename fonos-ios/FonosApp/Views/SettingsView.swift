import SwiftUI

/// Settings screen — placeholder scaffold.
struct SettingsView: View {
    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917")
                    .ignoresSafeArea()

                List {
                    Section("STT Provider") {
                        Text("Apple Speech (on-device)")
                            .foregroundColor(Color(hex: "#fafaf9"))
                    }
                    Section("Processing Mode") {
                        Text("Raw")
                            .foregroundColor(Color(hex: "#fafaf9"))
                    }
                    Section("Destination") {
                        Text("Clipboard")
                            .foregroundColor(Color(hex: "#fafaf9"))
                    }
                }
                .listStyle(.insetGrouped)
                .scrollContentBackground(.hidden)
            }
            .navigationTitle("Settings")
        }
    }
}

#Preview {
    SettingsView()
}

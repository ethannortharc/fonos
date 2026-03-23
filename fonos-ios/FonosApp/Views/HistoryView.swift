import SwiftUI
import SwiftData

/// History screen — placeholder scaffold.
struct HistoryView: View {
    @Query(sort: \DictationSession.date, order: .reverse) private var sessions: [DictationSession]

    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917")
                    .ignoresSafeArea()

                if sessions.isEmpty {
                    Text("No dictation history yet.")
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.6))
                } else {
                    List(sessions) { session in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(session.outputText.isEmpty ? session.inputText : session.outputText)
                                .lineLimit(2)
                                .foregroundColor(Color(hex: "#fafaf9"))
                            Text(session.date.formatted(date: .abbreviated, time: .shortened))
                                .font(.caption)
                                .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                        }
                        .listRowBackground(Color(hex: "#1a1917"))
                    }
                    .listStyle(.plain)
                }
            }
            .navigationTitle("History")
            .navigationBarTitleDisplayMode(.large)
        }
    }
}

#Preview {
    HistoryView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

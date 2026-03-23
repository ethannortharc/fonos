import SwiftUI
import SwiftData

/// History screen — full implementation with search, filter chips, and dictation session cards.
struct HistoryView: View {
    @Query(sort: \DictationSession.date, order: .reverse) private var sessions: [DictationSession]
    @Environment(\.modelContext) private var modelContext

    @State private var searchText: String = ""
    @State private var activeFilter: HistoryFilter = .all

    // MARK: - Filter chips

    enum HistoryFilter: String, CaseIterable {
        case all = "All"
        case polish = "Polish"
        case formal = "Formal"
        case translate = "Translate"

        var modeKey: String? {
            switch self {
            case .all:       return nil
            case .polish:    return "polish"
            case .formal:    return "formal"
            case .translate: return "translate"
            }
        }
    }

    // MARK: - Filtered sessions

    private var filteredSessions: [DictationSession] {
        sessions.filter { session in
            // Mode filter
            if let modeKey = activeFilter.modeKey, session.mode != modeKey {
                return false
            }
            // Search filter
            if !searchText.isEmpty {
                let query = searchText.lowercased()
                let inInput = session.inputText.lowercased().contains(query)
                let inOutput = session.outputText.lowercased().contains(query)
                return inInput || inOutput
            }
            return true
        }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                Color(hex: "#1a1917")
                    .ignoresSafeArea()

                VStack(spacing: 0) {
                    // MARK: - Search bar
                    searchBar
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                        .padding(.bottom, 12)

                    // MARK: - Filter chips
                    filterChips
                        .padding(.horizontal, 16)
                        .padding(.bottom, 12)

                    // MARK: - Content
                    if sessions.isEmpty {
                        emptyState
                    } else if filteredSessions.isEmpty {
                        noResultsState
                    } else {
                        sessionList
                    }
                }
            }
            .navigationTitle("History")
            .navigationBarTitleDisplayMode(.large)
        }
    }

    // MARK: - Search bar

    private var searchBar: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))

            TextField("", text: $searchText, prompt:
                Text("Search dictations...")
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.3))
            )
            .font(.system(size: 15))
            .foregroundColor(Color(hex: "#fafaf9"))
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)

            if !searchText.isEmpty {
                Button {
                    searchText = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 14))
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                }
                .buttonStyle(PlainButtonStyle())
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(hex: "#fafaf9").opacity(0.05))
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(Color(hex: "#fafaf9").opacity(0.08), lineWidth: 1)
                )
        )
    }

    // MARK: - Filter chips

    private var filterChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(HistoryFilter.allCases, id: \.rawValue) { filter in
                    filterChip(filter)
                }
            }
            .padding(.horizontal, 2)
        }
    }

    private func filterChip(_ filter: HistoryFilter) -> some View {
        let isActive = activeFilter == filter
        return Button {
            withAnimation(.spring(response: 0.25, dampingFraction: 0.7)) {
                activeFilter = filter
            }
        } label: {
            Text(filter.rawValue)
                .font(.system(size: 13, weight: isActive ? .semibold : .regular))
                .foregroundColor(isActive ? Color(hex: "#1a1917") : Color(hex: "#fafaf9").opacity(0.6))
                .padding(.horizontal, 14)
                .padding(.vertical, 7)
                .background(
                    Capsule()
                        .fill(isActive ? Color(hex: "#fbbf24") : Color(hex: "#fafaf9").opacity(0.07))
                        .overlay(
                            Capsule()
                                .strokeBorder(
                                    isActive ? Color.clear : Color(hex: "#fafaf9").opacity(0.08),
                                    lineWidth: 1
                                )
                        )
                )
        }
        .buttonStyle(PlainButtonStyle())
    }

    // MARK: - Session list

    private var sessionList: some View {
        List {
            ForEach(filteredSessions) { session in
                ActivityCard(session: session, onResend: nil)
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
            }
            .onDelete(perform: deleteSessions)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    // MARK: - Empty states

    private var emptyState: some View {
        VStack(spacing: 16) {
            Image(systemName: "clock")
                .font(.system(size: 40, weight: .light))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.2))
            Text("No dictation history yet.")
                .font(.system(size: 15))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
            Text("Your recorded sessions will appear here.")
                .font(.system(size: 13))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.25))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.bottom, 80)
    }

    private var noResultsState: some View {
        VStack(spacing: 12) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 36, weight: .light))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.2))
            Text("No results found.")
                .font(.system(size: 15))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.bottom, 80)
    }

    // MARK: - Delete

    private func deleteSessions(at offsets: IndexSet) {
        for index in offsets {
            let session = filteredSessions[index]
            modelContext.delete(session)
        }
    }
}

#Preview {
    HistoryView()
        .modelContainer(for: DictationSession.self, inMemory: true)
}

import SwiftUI

// MARK: - Destinations Section

/// Settings section for managing text routing destinations.
/// Supports add, toggle (auto-send target), and swipe-to-delete for non-built-in destinations.
struct DestinationsSection: View {
    @Binding var config: AppConfig

    @State private var showAdd = false

    private let amber = Color(hex: "#fbbf24")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.02)
    private let separator = Color.white.opacity(0.04)

    var body: some View {
        Section {
            ForEach(Array(config.destinations.enumerated()), id: \.element.id) { index, destination in
                destinationRow(destination, at: index)
            }

            Button {
                showAdd = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "plus.circle.fill")
                        .foregroundColor(amber)
                    Text("Add Destination")
                        .foregroundColor(amber)
                }
            }
            .listRowBackground(cardBg)
            .listRowSeparatorTint(separator)
        } header: {
            Text("DESTINATIONS")
                .font(.system(size: 12, weight: .medium))
                .foregroundColor(textDim)
                .textCase(nil)
        }
        .sheet(isPresented: $showAdd) {
            AddDestinationSheet(config: $config)
        }
    }

    // MARK: - Destination Row

    private func destinationRow(_ destination: AnyTextDestination, at index: Int) -> some View {
        let isAutoSend = config.autoSendDestination == destination.id

        return HStack(spacing: 12) {
            Image(systemName: iconFor(destination))
                .foregroundColor(amber)
                .frame(width: 22)

            Text(labelFor(destination))
                .foregroundColor(textPrimary)

            Spacer()

            // Active / auto-send indicator
            if isAutoSend {
                Circle()
                    .fill(amber)
                    .frame(width: 6, height: 6)
            }

            // Enable toggle (marks as auto-send destination)
            Toggle("", isOn: Binding<Bool>(
                get: { isAutoSend },
                set: { enabled in
                    config.autoSendDestination = enabled ? destination.id : ""
                }
            ))
            .tint(amber)
            .labelsHidden()
        }
        .listRowBackground(cardBg)
        .listRowSeparatorTint(separator)
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if destination.id != "clipboard" {
                Button(role: .destructive) {
                    config.destinations.remove(at: index)
                    // Clear auto-send if we deleted the target
                    if config.autoSendDestination == destination.id {
                        config.autoSendDestination = ""
                    }
                } label: {
                    Label("Delete", systemImage: "trash")
                }
            }
        }
    }

    // MARK: - Helpers

    private func labelFor(_ destination: AnyTextDestination) -> String {
        switch destination.id {
        case "clipboard":  return "Clipboard"
        case "messages":   return "Messages"
        case "url_scheme": return "URL Scheme"
        default:            return destination.id.capitalized
        }
    }

    private func iconFor(_ destination: AnyTextDestination) -> String {
        switch destination.id {
        case "clipboard":  return "doc.on.clipboard"
        case "messages":   return "message.fill"
        case "url_scheme": return "link"
        default:            return "arrow.up.forward.app"
        }
    }
}

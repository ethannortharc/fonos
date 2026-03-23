import SwiftUI

// MARK: - Destination Picker

/// Picker view for selecting a text destination when sending dictated text.
/// Presents a list of configured destinations with icons and labels.
struct DestinationPicker: View {

    // MARK: - Input

    let destinations: [AnyTextDestination]

    /// Currently selected destination id, or empty string for none.
    @Binding var selectedID: String

    @Environment(\.dismiss) private var dismiss

    // MARK: - Styling

    private let amber = Color(hex: "#fbbf24")
    private let bg = Color(hex: "#1a1917")
    private let textPrimary = Color(hex: "#fafaf9")
    private let textDim = Color(hex: "#fafaf9").opacity(0.5)
    private let cardBg = Color.white.opacity(0.04)
    private let cardBorder = Color.white.opacity(0.06)

    var body: some View {
        NavigationStack {
            ZStack {
                bg.ignoresSafeArea()

                List {
                    Section {
                        // None option
                        destinationRow(
                            id: "",
                            icon: "nosign",
                            label: "None"
                        )

                        ForEach(destinations, id: \.id) { destination in
                            destinationRow(
                                id: destination.id,
                                icon: iconFor(destination),
                                label: labelFor(destination)
                            )
                        }
                    } header: {
                        Text("SELECT DESTINATION")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(textDim)
                            .textCase(nil)
                    }
                }
                .listStyle(.insetGrouped)
                .scrollContentBackground(.hidden)
            }
            .navigationTitle("Send To")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundColor(amber)
                }
            }
        }
    }

    // MARK: - Row

    private func destinationRow(id: String, icon: String, label: String) -> some View {
        let isSelected = selectedID == id
        return Button {
            selectedID = id
            dismiss()
        } label: {
            HStack(spacing: 12) {
                Image(systemName: icon)
                    .foregroundColor(amber)
                    .frame(width: 22)

                Text(label)
                    .foregroundColor(textPrimary)

                Spacer()

                if isSelected {
                    Image(systemName: "checkmark")
                        .foregroundColor(amber)
                        .font(.system(size: 14, weight: .semibold))
                }
            }
        }
        .buttonStyle(PlainButtonStyle())
        .listRowBackground(cardBg)
        .listRowSeparatorTint(Color.white.opacity(0.04))
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

#Preview {
    DestinationPicker(
        destinations: [
            AnyTextDestination(ClipboardDestination()),
            AnyTextDestination(MessagesDestination())
        ],
        selectedID: .constant("clipboard")
    )
}

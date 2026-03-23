import SwiftUI

/// Single dictation result card for the history list.
struct ActivityCard: View {
    let session: DictationSession
    var onResend: ((DictationSession) -> Void)? = nil

    @State private var isExpanded = false

    private var displayText: String {
        session.outputText.isEmpty ? session.inputText : session.outputText
    }

    private var modeIcon: String {
        switch session.mode {
        case "raw":       return "waveform"
        case "polish":    return "sparkles"
        case "formal":    return "briefcase"
        case "translate": return "globe"
        case "custom":    return "slider.horizontal.3"
        default:          return "waveform"
        }
    }

    private var modeName: String {
        switch session.mode {
        case "raw":       return "Raw"
        case "polish":    return "Polish"
        case "formal":    return "Formal"
        case "translate": return "Translate"
        case "custom":    return "Custom"
        default:          return session.mode.capitalized
        }
    }

    private var formattedTimestamp: String {
        let calendar = Calendar.current
        if calendar.isDateInToday(session.date) {
            let formatter = DateFormatter()
            formatter.dateFormat = "h:mm a"
            return formatter.string(from: session.date)
        } else if calendar.isDateInYesterday(session.date) {
            return "Yesterday"
        } else {
            let formatter = DateFormatter()
            formatter.dateFormat = "MMM d"
            return formatter.string(from: session.date)
        }
    }

    private var latencyText: String {
        if session.latencyMs < 1000 {
            return String(format: "%.0fms", session.latencyMs)
        } else {
            return String(format: "%.1fs", session.latencyMs / 1000)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // MARK: - Header row
            HStack(spacing: 8) {
                // Mode icon + name
                HStack(spacing: 5) {
                    Image(systemName: modeIcon)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(Color(hex: "#fbbf24"))
                    Text(modeName)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(Color(hex: "#fbbf24"))
                }

                Spacer()

                // Timestamp (monospace)
                Text(formattedTimestamp)
                    .font(.system(size: 11, weight: .regular, design: .monospaced))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
            }
            .padding(.bottom, 8)

            // MARK: - Text content
            Text(displayText)
                .font(.system(size: 14))
                .foregroundColor(Color(hex: "#fafaf9"))
                .lineLimit(isExpanded ? nil : 2)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)

            // MARK: - Footer row
            HStack(spacing: 12) {
                // Destination
                HStack(spacing: 4) {
                    Image(systemName: destinationIcon(session.destination))
                        .font(.system(size: 10))
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.35))
                    Text(destinationLabel(session.destination))
                        .font(.system(size: 11))
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.35))
                }

                Spacer()

                // Latency (monospace)
                if session.latencyMs > 0 {
                    Text(latencyText)
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundColor(Color(hex: "#fbbf24").opacity(0.5))
                }
            }
            .padding(.top, 8)

            // MARK: - Expanded detail
            if isExpanded {
                VStack(alignment: .leading, spacing: 8) {
                    if !session.inputText.isEmpty && session.inputText != session.outputText && !session.outputText.isEmpty {
                        Divider()
                            .background(Color(hex: "#fafaf9").opacity(0.08))
                            .padding(.vertical, 4)

                        VStack(alignment: .leading, spacing: 4) {
                            Text("Original")
                                .font(.caption2)
                                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                            Text(session.inputText)
                                .font(.system(size: 13))
                                .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                                .multilineTextAlignment(.leading)
                        }
                    }

                    // Re-send button
                    if let onResend {
                        Button {
                            onResend(session)
                        } label: {
                            HStack(spacing: 6) {
                                Image(systemName: "arrow.uturn.right")
                                    .font(.system(size: 12, weight: .medium))
                                Text("Re-send")
                                    .font(.system(size: 13, weight: .medium))
                            }
                            .foregroundColor(Color(hex: "#fbbf24"))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                            .background(
                                RoundedRectangle(cornerRadius: 10)
                                    .fill(Color(hex: "#fbbf24").opacity(0.1))
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 10)
                                            .strokeBorder(Color(hex: "#fbbf24").opacity(0.2), lineWidth: 1)
                                    )
                            )
                        }
                        .buttonStyle(PlainButtonStyle())
                        .padding(.top, 4)
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(Color(hex: "#fafaf9").opacity(0.02))
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .strokeBorder(Color(hex: "#fafaf9").opacity(0.04), lineWidth: 1)
                )
        )
        .contentShape(Rectangle())
        .onTapGesture {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                isExpanded.toggle()
            }
        }
    }

    // MARK: - Helpers

    private func destinationIcon(_ destination: String) -> String {
        switch destination.lowercased() {
        case "clipboard":  return "doc.on.clipboard"
        case "messages":   return "message.fill"
        case "share":      return "square.and.arrow.up"
        case "wechat":     return "message.circle"
        case "telegram":   return "paperplane.fill"
        case "slack":      return "number"
        case "email":      return "envelope.fill"
        case "notion":     return "doc.text"
        default:           return "arrow.right.circle"
        }
    }

    private func destinationLabel(_ destination: String) -> String {
        switch destination.lowercased() {
        case "clipboard": return "Clipboard"
        case "messages":  return "Messages"
        case "share":     return "Share"
        default:          return destination.capitalized
        }
    }
}

#Preview {
    VStack(spacing: 12) {
        ActivityCard(
            session: DictationSession(
                id: UUID(),
                date: Date(),
                mode: "polish",
                inputText: "um yeah so basically i was thinking about the meeting and",
                outputText: "I was reflecting on the meeting and its implications.",
                destination: "clipboard",
                latencyMs: 320
            )
        )
        ActivityCard(
            session: DictationSession(
                id: UUID(),
                date: Date().addingTimeInterval(-86400),
                mode: "formal",
                inputText: "hey can we chat",
                outputText: "Could we schedule a conversation at your earliest convenience?",
                destination: "messages",
                latencyMs: 450
            )
        )
    }
    .padding()
    .background(Color(hex: "#1a1917"))
}

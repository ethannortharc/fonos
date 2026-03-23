import SwiftUI

/// Horizontal drum-roller mode selector with snap behavior.
/// The current mode is prominently highlighted in amber; adjacent modes are dimmed.
struct ModePicker: View {
    @Binding var selectedMode: Mode
    let modes: [Mode]

    // Width for each mode item
    private let itemWidth: CGFloat = 100
    private let itemSpacing: CGFloat = 12

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: itemSpacing) {
                ForEach(modes, id: \.id) { mode in
                    modeItem(mode)
                        .containerRelativeFrame(.horizontal, count: 4, spacing: itemSpacing)
                }
            }
            .scrollTargetLayout()
        }
        .scrollTargetBehavior(.viewAligned)
        .frame(height: 64)
    }

    @ViewBuilder
    private func modeItem(_ mode: Mode) -> some View {
        let isSelected = selectedMode == mode

        Button {
            if selectedMode != mode {
                selectedMode = mode
                let generator = UIImpactFeedbackGenerator(style: .light)
                generator.impactOccurred()
            }
        } label: {
            VStack(spacing: 6) {
                Image(systemName: mode.icon)
                    .font(.system(size: 16, weight: isSelected ? .semibold : .regular))
                Text(shortName(mode))
                    .font(.system(size: 12, weight: isSelected ? .semibold : .regular))
                    .lineLimit(1)
            }
            .foregroundColor(isSelected ? Color(hex: "#1a1917") : Color(hex: "#fafaf9"))
            .opacity(isSelected ? 1.0 : 0.3)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .fill(isSelected ? Color(hex: "#fbbf24") : Color(hex: "#fafaf9").opacity(0.06))
                    .overlay(
                        RoundedRectangle(cornerRadius: 14)
                            .strokeBorder(
                                isSelected ? Color.clear : Color(hex: "#fafaf9").opacity(0.06),
                                lineWidth: 1
                            )
                    )
            )
            .shadow(
                color: isSelected ? Color(hex: "#fbbf24").opacity(0.3) : .clear,
                radius: 8,
                x: 0,
                y: 2
            )
        }
        .buttonStyle(PlainButtonStyle())
        .animation(.spring(response: 0.3, dampingFraction: 0.75), value: isSelected)
    }

    private func shortName(_ mode: Mode) -> String {
        switch mode {
        case .raw:       return "Raw"
        case .polish:    return "Polish"
        case .formal:    return "Formal"
        case .translate: return "Translate"
        case .custom:    return "Custom"
        }
    }
}

#Preview {
    @Previewable @State var selectedMode: Mode = .polish

    let modes: [Mode] = [
        .raw,
        .polish,
        .formal,
        .translate(targetLanguage: "English"),
        .custom(systemPrompt: "You are helpful.", userTemplate: "{text}", temperature: 0.7, maxTokens: 1024)
    ]

    return VStack {
        ModePicker(selectedMode: $selectedMode, modes: modes)
            .padding(.horizontal, 16)
        Text("Selected: \(selectedMode.displayName)")
            .foregroundColor(.white)
    }
    .padding()
    .background(Color(hex: "#1a1917"))
}

import SwiftUI

/// Large circular mic button with recording states.
/// - Idle: amber background, mic icon
/// - Recording: red background, stop icon, pulse animation
/// - Processing: activity indicator
struct RecordButton: View {
    let state: RecordButton.ButtonState
    let onTap: () -> Void

    enum ButtonState {
        case idle
        case recording
        case processing
    }

    @State private var isPulsing = false

    private let buttonSize: CGFloat = 96

    var body: some View {
        ZStack {
            // Pulse ring (recording state only)
            if state == .recording {
                Circle()
                    .stroke(Color(hex: "#ef4444").opacity(0.4), lineWidth: 2)
                    .frame(
                        width: buttonSize + (isPulsing ? 32 : 0),
                        height: buttonSize + (isPulsing ? 32 : 0)
                    )
                    .opacity(isPulsing ? 0 : 0.8)
                    .animation(
                        .easeOut(duration: 1.2).repeatForever(autoreverses: false),
                        value: isPulsing
                    )
            }

            // Main button
            Button(action: onTap) {
                Circle()
                    .fill(buttonBackground)
                    .frame(width: buttonSize, height: buttonSize)
                    .overlay {
                        buttonContent
                    }
                    .shadow(color: buttonShadow, radius: state == .recording ? 16 : 8, x: 0, y: 4)
            }
            .buttonStyle(PlainButtonStyle())
            .scaleEffect(state == .recording ? 1.05 : 1.0)
            .animation(.spring(response: 0.3, dampingFraction: 0.6), value: state == .recording)
        }
        .frame(width: buttonSize + 48, height: buttonSize + 48)
        .onAppear {
            if state == .recording {
                isPulsing = true
            }
        }
        .onChange(of: state) { _, newState in
            isPulsing = (newState == .recording)
        }
    }

    private var buttonBackground: Color {
        switch state {
        case .idle:       return Color(hex: "#fbbf24")
        case .recording:  return Color(hex: "#ef4444")
        case .processing: return Color(hex: "#fbbf24").opacity(0.7)
        }
    }

    private var buttonShadow: Color {
        switch state {
        case .idle:       return Color(hex: "#fbbf24").opacity(0.4)
        case .recording:  return Color(hex: "#ef4444").opacity(0.5)
        case .processing: return Color.clear
        }
    }

    @ViewBuilder
    private var buttonContent: some View {
        switch state {
        case .idle:
            Image(systemName: "mic.fill")
                .font(.system(size: 40, weight: .medium))
                .foregroundColor(Color(hex: "#1a1917"))
        case .recording:
            Image(systemName: "stop.fill")
                .font(.system(size: 32, weight: .medium))
                .foregroundColor(.white)
        case .processing:
            ProgressView()
                .progressViewStyle(CircularProgressViewStyle(tint: Color(hex: "#1a1917")))
                .scaleEffect(1.4)
        }
    }
}

#Preview {
    VStack(spacing: 32) {
        RecordButton(state: .idle, onTap: {})
        RecordButton(state: .recording, onTap: {})
        RecordButton(state: .processing, onTap: {})
    }
    .padding()
    .background(Color(hex: "#1a1917"))
}

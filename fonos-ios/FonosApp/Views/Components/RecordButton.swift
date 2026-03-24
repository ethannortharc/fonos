import SwiftUI

/// Large circular mic button with recording states.
struct RecordButton: View {
    let state: RecordButton.ButtonState
    let onTap: () -> Void

    enum ButtonState: Equatable {
        case idle
        case recording
        case processing
    }

    private let buttonSize: CGFloat = 96

    var body: some View {
        Button(action: onTap) {
            ZStack {
                // Background circle — color changes with state
                Circle()
                    .fill(state == .recording ? Color.red : Color.orange)
                    .frame(width: buttonSize, height: buttonSize)

                // Icon
                Group {
                    switch state {
                    case .idle:
                        Image(systemName: "mic.fill")
                            .font(.system(size: 40, weight: .medium))
                            .foregroundColor(.white)
                    case .recording:
                        Image(systemName: "stop.fill")
                            .font(.system(size: 32, weight: .medium))
                            .foregroundColor(.white)
                    case .processing:
                        ProgressView()
                            .progressViewStyle(CircularProgressViewStyle(tint: .white))
                            .scaleEffect(1.4)
                    }
                }
            }
            .shadow(
                color: state == .recording ? Color.red.opacity(0.5) : Color.orange.opacity(0.4),
                radius: state == .recording ? 16 : 8,
                x: 0, y: 4
            )
        }
        .buttonStyle(.plain)
        .frame(width: buttonSize + 48, height: buttonSize + 48)
    }
}

#Preview {
    VStack(spacing: 32) {
        RecordButton(state: .idle, onTap: {})
        RecordButton(state: .recording, onTap: {})
        RecordButton(state: .processing, onTap: {})
    }
    .padding()
    .background(Color.black)
}

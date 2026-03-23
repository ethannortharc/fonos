import SwiftUI

/// Live audio level waveform visualization.
/// Shows animated vertical bars that reflect the current audio level.
/// Amber color, animates smoothly during recording or shows idle animation.
struct WaveformView: View {
    var audioLevel: Float
    var isRecording: Bool

    private let barCount = 20
    private let barWidth: CGFloat = 3
    private let barSpacing: CGFloat = 3
    private let maxBarHeight: CGFloat = 48
    private let minBarHeight: CGFloat = 4

    @State private var idlePhase: Double = 0

    var body: some View {
        HStack(spacing: barSpacing) {
            ForEach(0..<barCount, id: \.self) { index in
                RoundedRectangle(cornerRadius: barWidth / 2)
                    .fill(Color(hex: "#fbbf24"))
                    .frame(width: barWidth, height: barHeight(for: index))
                    .animation(.easeInOut(duration: 0.08), value: audioLevel)
                    .animation(.easeInOut(duration: 0.4), value: idlePhase)
            }
        }
        .frame(height: maxBarHeight)
        .onAppear {
            startIdleAnimation()
        }
        .onChange(of: isRecording) { _, _ in
            // Reset phase when state changes
            idlePhase = 0
            if !isRecording {
                startIdleAnimation()
            }
        }
    }

    private func barHeight(for index: Int) -> CGFloat {
        if isRecording {
            // Bars animate based on audio level with variation per bar position
            let position = Double(index) / Double(barCount)
            let levelFactor = Double(max(0.05, audioLevel))
            let distance = abs(position - 0.5) * 2
            let shapeFactor = max(0.1, 1.0 - distance * 0.6)
            let noise = sin(Double(index) * 1.8 + idlePhase * 3) * 0.15
            let height = levelFactor * shapeFactor + noise * levelFactor + 0.05
            return CGFloat(max(Double(minBarHeight), min(height * Double(maxBarHeight), Double(maxBarHeight))))
        } else {
            // Gentle idle sine-wave animation
            let position = Double(index) / Double(barCount)
            let wave = sin(position * .pi * 2 + idlePhase) * 0.5 + 0.5
            let baseHeight = 0.12 + wave * 0.22
            return CGFloat(max(Double(minBarHeight), baseHeight * Double(maxBarHeight)))
        }
    }

    private func startIdleAnimation() {
        withAnimation(.linear(duration: 2).repeatForever(autoreverses: false)) {
            idlePhase = .pi * 2
        }
    }
}

#Preview {
    VStack(spacing: 24) {
        WaveformView(audioLevel: 0, isRecording: false)
        WaveformView(audioLevel: 0.6, isRecording: true)
        WaveformView(audioLevel: 1.0, isRecording: true)
    }
    .padding()
    .background(Color(hex: "#1a1917"))
}

import SwiftUI

/// Live audio level waveform visualization.
/// Bars animate dramatically during recording based on audio level.
struct WaveformView: View {
    var audioLevel: Float
    var isRecording: Bool

    private let barCount = 24
    private let barWidth: CGFloat = 4
    private let barSpacing: CGFloat = 3
    private let maxBarHeight: CGFloat = 80
    private let minBarHeight: CGFloat = 6

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 15.0, paused: !isRecording)) { timeline in
            HStack(spacing: barSpacing) {
                ForEach(0..<barCount, id: \.self) { index in
                    RoundedRectangle(cornerRadius: barWidth / 2)
                        .fill(barColor(for: index))
                        .frame(width: barWidth, height: barHeight(for: index, date: timeline.date))
                }
            }
            .frame(height: maxBarHeight)
        }
    }

    private func barColor(for index: Int) -> Color {
        if isRecording {
            // Gradient from amber to red based on level
            let blend = Double(min(1.0, audioLevel * 2))
            return Color(
                red: (251 + (239 - 251) * blend) / 255,
                green: (191 - 123 * blend) / 255,
                blue: (36 + 32 * blend) / 255
            )
        } else {
            return Color(red: 251/255, green: 191/255, blue: 36/255).opacity(0.4)
        }
    }

    private func barHeight(for index: Int, date: Date) -> CGFloat {
        if isRecording {
            let time = date.timeIntervalSinceReferenceDate
            let position = Double(index) / Double(barCount)

            // Boost audio level aggressively for visual impact
            let boostedLevel = Double(min(1.0, pow(max(0.01, audioLevel), 0.4)))

            // Center-weighted shape — middle bars are tallest
            let distance = abs(position - 0.5) * 2
            let shapeFactor = max(0.15, 1.0 - distance * distance * 0.7)

            // Multi-frequency noise for organic movement
            let noise1 = sin(Double(index) * 2.1 + time * 8.0) * 0.2
            let noise2 = sin(Double(index) * 0.7 + time * 5.0) * 0.15
            let noise3 = cos(Double(index) * 3.3 + time * 12.0) * 0.1

            // Combine: level drives overall height, noise adds variation
            let height = boostedLevel * shapeFactor * (1.0 + noise1 + noise2 + noise3) + 0.08
            let clamped = max(Double(minBarHeight) / Double(maxBarHeight), min(height, 1.0))

            return CGFloat(clamped * Double(maxBarHeight))
        } else {
            // Gentle static wave pattern
            let position = Double(index) / Double(barCount)
            let wave = sin(position * .pi * 2) * 0.5 + 0.5
            let baseHeight = 0.08 + wave * 0.15
            return CGFloat(max(Double(minBarHeight), baseHeight * Double(maxBarHeight)))
        }
    }
}

#Preview {
    VStack(spacing: 24) {
        Text("Idle").foregroundColor(.white)
        WaveformView(audioLevel: 0, isRecording: false)

        Text("Low level").foregroundColor(.white)
        WaveformView(audioLevel: 0.1, isRecording: true)

        Text("Medium level").foregroundColor(.white)
        WaveformView(audioLevel: 0.4, isRecording: true)

        Text("High level").foregroundColor(.white)
        WaveformView(audioLevel: 0.8, isRecording: true)
    }
    .padding()
    .background(Color.black)
}

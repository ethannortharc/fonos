import SwiftUI

/// Live audio level waveform visualization.
/// Shows bars that reflect the current audio level during recording,
/// or a gentle static pattern when idle.
struct WaveformView: View {
    var audioLevel: Float
    var isRecording: Bool

    private let barCount = 20
    private let barWidth: CGFloat = 3
    private let barSpacing: CGFloat = 3
    private let maxBarHeight: CGFloat = 48
    private let minBarHeight: CGFloat = 4

    var body: some View {
        TimelineView(.animation(minimumInterval: 0.1, paused: !isRecording)) { timeline in
            HStack(spacing: barSpacing) {
                ForEach(0..<barCount, id: \.self) { index in
                    RoundedRectangle(cornerRadius: barWidth / 2)
                        .fill(Color.orange)
                        .frame(width: barWidth, height: barHeight(for: index, date: timeline.date))
                }
            }
            .frame(height: maxBarHeight)
        }
    }

    private func barHeight(for index: Int, date: Date) -> CGFloat {
        let position = Double(index) / Double(barCount)

        if isRecording {
            let levelFactor = Double(max(0.05, audioLevel))
            let distance = abs(position - 0.5) * 2
            let shapeFactor = max(0.1, 1.0 - distance * 0.6)
            let time = date.timeIntervalSinceReferenceDate
            let noise = sin(Double(index) * 1.8 + time * 3) * 0.15
            let height = levelFactor * shapeFactor + noise * levelFactor + 0.05
            return CGFloat(max(Double(minBarHeight), min(height * Double(maxBarHeight), Double(maxBarHeight))))
        } else {
            // Static idle pattern — no animation, no repeatForever
            let wave = sin(position * .pi * 2) * 0.5 + 0.5
            let baseHeight = 0.12 + wave * 0.22
            return CGFloat(max(Double(minBarHeight), baseHeight * Double(maxBarHeight)))
        }
    }
}

#Preview {
    VStack(spacing: 24) {
        WaveformView(audioLevel: 0, isRecording: false)
        WaveformView(audioLevel: 0.6, isRecording: true)
    }
    .padding()
    .background(Color.black)
}

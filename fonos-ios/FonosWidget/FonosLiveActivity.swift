import ActivityKit
import SwiftUI
import WidgetKit

// MARK: - Activity Attributes

/// Attributes for the Fonos recording Live Activity.
/// Displayed on the Dynamic Island and Lock Screen during background recording.
public struct FonosRecordingAttributes: ActivityAttributes {
    public typealias ContentState = FonosRecordingStatus

    /// Static data (set when activity starts and doesn't change).
    public var sessionID: String

    public init(sessionID: String) {
        self.sessionID = sessionID
    }

    // MARK: - Content State (dynamic, updated as recording proceeds)

    public struct FonosRecordingStatus: Codable, Hashable, Sendable {
        /// Elapsed recording time in seconds.
        public var elapsedSeconds: Int
        /// Whether recording is actively ongoing.
        public var isRecording: Bool
        /// Waveform level samples (0.0–1.0), 5 bars.
        public var waveformLevels: [Float]

        public init(elapsedSeconds: Int = 0, isRecording: Bool = true, waveformLevels: [Float] = [0.4, 0.6, 0.8, 0.5, 0.7]) {
            self.elapsedSeconds = elapsedSeconds
            self.isRecording = isRecording
            self.waveformLevels = waveformLevels
        }

        /// Formatted elapsed time string, e.g. "0:42".
        public var elapsedFormatted: String {
            let minutes = elapsedSeconds / 60
            let seconds = elapsedSeconds % 60
            return String(format: "%d:%02d", minutes, seconds)
        }
    }
}

// MARK: - Design Colors (shared)

private extension Color {
    static let fonosBackground = Color(red: 0x1a / 255.0, green: 0x19 / 255.0, blue: 0x17 / 255.0)
    static let fonosAmber = Color(red: 0xfb / 255.0, green: 0xbf / 255.0, blue: 0x24 / 255.0)
    static let fonosRecording = Color(red: 0xef / 255.0, green: 0x44 / 255.0, blue: 0x44 / 255.0)
}

// MARK: - Waveform Bars View

/// Five animated bars representing live audio levels.
private struct LiveActivityWaveformView: View {
    let levels: [Float]

    var body: some View {
        HStack(alignment: .center, spacing: 2) {
            ForEach(Array(levels.enumerated()), id: \.offset) { _, level in
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(Color.fonosRecording)
                    .frame(width: 3, height: CGFloat(level) * 16 + 4)
            }
        }
    }
}

// MARK: - Live Activity Widget

struct FonosLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: FonosRecordingAttributes.self) { context in
            // Lock Screen / Expanded view
            LockScreenLiveActivityView(context: context)
                .activityBackgroundTint(Color.fonosBackground)
                .activitySystemActionForegroundColor(.white)

        } dynamicIsland: { context in
            DynamicIsland {
                // Expanded Dynamic Island
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 6) {
                        Image(systemName: "mic.fill")
                            .foregroundColor(.fonosRecording)
                            .font(.system(size: 16, weight: .semibold))
                        Text("Recording")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundColor(.white)
                    }
                    .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.elapsedFormatted)
                        .font(.system(size: 14, weight: .semibold, design: .monospaced))
                        .foregroundColor(.fonosAmber)
                        .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(spacing: 16) {
                        LiveActivityWaveformView(levels: context.state.waveformLevels)
                        Spacer()
                        Text("Fonos")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundColor(.white.opacity(0.5))
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)
                }
            } compactLeading: {
                // Compact leading — mic icon
                Image(systemName: "mic.fill")
                    .foregroundColor(.fonosRecording)
                    .font(.system(size: 12, weight: .semibold))
            } compactTrailing: {
                // Compact trailing — elapsed time
                Text(context.state.elapsedFormatted)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundColor(.fonosAmber)
            } minimal: {
                // Minimal — just the mic icon
                Image(systemName: "mic.fill")
                    .foregroundColor(.fonosRecording)
                    .font(.system(size: 10, weight: .semibold))
            }
            .widgetURL(URL(string: "fonos://record"))
            .keylineTint(.fonosRecording)
        }
    }
}

// MARK: - Lock Screen View

private struct LockScreenLiveActivityView: View {
    let context: ActivityViewContext<FonosRecordingAttributes>

    var body: some View {
        HStack(spacing: 12) {
            // Recording indicator
            ZStack {
                Circle()
                    .fill(Color.fonosRecording.opacity(0.2))
                    .frame(width: 40, height: 40)
                Image(systemName: "mic.fill")
                    .foregroundColor(.fonosRecording)
                    .font(.system(size: 18, weight: .semibold))
            }

            // Status text
            VStack(alignment: .leading, spacing: 2) {
                Text("Fonos Recording")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundColor(.white)
                Text("Tap to return to app")
                    .font(.system(size: 12))
                    .foregroundColor(.white.opacity(0.6))
            }

            Spacer()

            // Elapsed time + waveform
            VStack(alignment: .trailing, spacing: 4) {
                Text(context.state.elapsedFormatted)
                    .font(.system(size: 16, weight: .bold, design: .monospaced))
                    .foregroundColor(.fonosAmber)
                LiveActivityWaveformView(levels: context.state.waveformLevels)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
}

// MARK: - Preview

extension FonosRecordingAttributes {
    static var previewAttributes: Self {
        .init(sessionID: "preview-session")
    }
}

extension FonosRecordingAttributes.FonosRecordingStatus {
    static var previewState: Self {
        .init(elapsedSeconds: 42, isRecording: true, waveformLevels: [0.3, 0.7, 0.5, 0.9, 0.4])
    }
}

#Preview("Lock Screen", as: .content, using: FonosRecordingAttributes.previewAttributes) {
    FonosLiveActivityWidget()
} contentStates: {
    FonosRecordingAttributes.FonosRecordingStatus.previewState
}

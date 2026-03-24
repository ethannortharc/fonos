import SwiftUI
import os.log

private let viewLog = Logger(subsystem: "com.fonos.ios", category: "DictationView")

/// Main dictation recording screen.
struct DictationView: View {
    @ObservedObject var viewModel: DictationViewModel
    @State private var showCopiedFeedback = false

    /// Convenience initialiser for previews and unit tests.
    /// Uses a default no-op DictationViewModel (no STT provider).
    init(viewModel: DictationViewModel = DictationViewModel()) {
        self.viewModel = viewModel
    }

    private let modes: [Mode] = [.raw, .polish, .formal, .translate(targetLanguage: "English"), .custom(
        systemPrompt: "You are a helpful assistant.",
        userTemplate: "{text}",
        temperature: 0.7,
        maxTokens: 1024
    )]

    var body: some View {
        ZStack {
            Color(hex: "#1a1917")
                .ignoresSafeArea()

            VStack(spacing: 0) {
                // MARK: - Header
                headerView
                    .padding(.top, 8)
                    .padding(.horizontal, 20)

                // MARK: - Mode Picker Strip
                modePickerStrip
                    .padding(.top, 16)
                    .padding(.horizontal, 20)

                Spacer()

                // MARK: - Waveform
                WaveformView(
                    audioLevel: viewModel.audioLevel,
                    isRecording: viewModel.isRecording
                )
                .padding(.horizontal, 40)
                .padding(.bottom, 24)

                // MARK: - Record Button
                recordButtonSection

                // MARK: - Result Area
                if case .result(let transcript, let processed) = viewModel.recordingState {
                    resultCard(transcript: transcript, processed: processed)
                        .padding(.horizontal, 20)
                        .padding(.top, 24)
                }

                // MARK: - Error
                if case .error(let message) = viewModel.recordingState {
                    errorCard(message: message)
                        .padding(.horizontal, 20)
                        .padding(.top, 24)
                }

                Spacer()

                // MARK: - Latency
                if viewModel.sttLatency > 0 || viewModel.llmLatency > 0 {
                    latencyView
                        .padding(.horizontal, 20)
                        .padding(.bottom, 8)
                }

                // MARK: - Destination Quick Actions
                destinationStrip
                    .padding(.horizontal, 20)
                    .padding(.bottom, 16)
            }
        }
    }

    // MARK: - Header

    private var headerView: some View {
        HStack {
            Text("Fonos")
                .font(.system(size: 22, weight: .bold, design: .default))
                .foregroundColor(Color(hex: "#fafaf9"))

            Spacer()

            // Status indicator
            HStack(spacing: 6) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 8, height: 8)
                Text(statusText)
                    .font(.caption)
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.6))
            }
        }
    }

    private var statusColor: Color {
        switch viewModel.recordingState {
        case .idle:       return Color(hex: "#fafaf9").opacity(0.3)
        case .recording:  return Color(hex: "#ef4444")
        case .processing: return Color(hex: "#fbbf24")
        case .result:     return Color(hex: "#86efac")
        case .error:      return Color(hex: "#ef4444")
        }
    }

    private var statusText: String {
        switch viewModel.recordingState {
        case .idle:       return "Ready"
        case .recording:  return "Recording"
        case .processing: return "Processing"
        case .result:     return "Done"
        case .error:      return "Error"
        }
    }

    // MARK: - Mode Picker Strip

    private var modePickerStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(modes, id: \.id) { mode in
                    modeChip(mode)
                }
            }
            .padding(.horizontal, 4)
        }
    }

    private func modeChip(_ mode: Mode) -> some View {
        let isSelected = viewModel.currentMode == mode
        return Button {
            viewModel.currentMode = mode
        } label: {
            HStack(spacing: 6) {
                Image(systemName: mode.icon)
                    .font(.system(size: 12, weight: .medium))
                Text(mode.displayName)
                    .font(.system(size: 13, weight: isSelected ? .semibold : .regular))
            }
            .foregroundColor(isSelected ? Color(hex: "#1a1917") : Color(hex: "#fafaf9").opacity(0.7))
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(isSelected ? Color(hex: "#fbbf24") : Color(hex: "#fafaf9").opacity(0.08))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12)
                            .strokeBorder(
                                isSelected ? Color.clear : Color(hex: "#fafaf9").opacity(0.08),
                                lineWidth: 1
                            )
                    )
            )
        }
        .buttonStyle(PlainButtonStyle())
        .animation(.spring(response: 0.25, dampingFraction: 0.7), value: isSelected)
    }

    // MARK: - Record Button Section

    private var recordButtonSection: some View {
        VStack(spacing: 16) {
            RecordButton(
                state: recordButtonState,
                onTap: handleRecordTap
            )

            Text(recordButtonHint)
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(
                    viewModel.isRecording ? Color.red : Color.white.opacity(0.5)
                )
        }
    }

    private var recordButtonState: RecordButton.ButtonState {
        switch viewModel.recordingState {
        case .idle, .result, .error: return .idle
        case .recording:             return .recording
        case .processing:            return .processing
        }
    }

    private var recordButtonHint: String {
        switch viewModel.recordingState {
        case .idle:       return "Tap to dictate"
        case .recording:  return "Tap to stop"
        case .processing: return "Processing..."
        case .result:     return "Tap to record again"
        case .error:      return "Tap to try again"
        }
    }

    private func handleRecordTap() {
        viewLog.info("👆 handleRecordTap() — current state: \(String(describing: viewModel.recordingState))")
        switch viewModel.recordingState {
        case .idle, .result, .error:
            viewLog.info("👆 → calling startRecording()")
            viewModel.startRecording()
        case .recording:
            viewLog.info("👆 → calling stopRecording()")
            viewModel.stopRecording()
        case .processing:
            viewLog.info("👆 → processing, ignoring tap")
            break
        }
    }

    // MARK: - Result Card

    private func resultCard(transcript: String, processed: String?) -> some View {
        let displayText = processed ?? transcript

        return VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundColor(Color(hex: "#86efac"))
                Text("Result")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(Color(hex: "#86efac"))
                Spacer()

                if showCopiedFeedback {
                    Text("Copied!")
                        .font(.caption)
                        .foregroundColor(Color(hex: "#86efac"))
                        .transition(.opacity)
                }
            }

            Text(displayText)
                .font(.system(size: 16))
                .foregroundColor(Color(hex: "#fafaf9"))
                .lineLimit(6)
                .multilineTextAlignment(.leading)

            if processed != nil && processed != transcript {
                Divider()
                    .background(Color(hex: "#fafaf9").opacity(0.1))

                VStack(alignment: .leading, spacing: 4) {
                    Text("Original")
                        .font(.caption2)
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
                    Text(transcript)
                        .font(.system(size: 13))
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                        .lineLimit(3)
                }
            }
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(Color(hex: "#fafaf9").opacity(0.02))
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .strokeBorder(Color(hex: "#fafaf9").opacity(0.08), lineWidth: 1)
                )
        )
        .onTapGesture {
            UIPasteboard.general.string = displayText
            withAnimation {
                showCopiedFeedback = true
            }
            Task {
                try? await Task.sleep(nanoseconds: 1_500_000_000)
                withAnimation {
                    showCopiedFeedback = false
                }
            }
        }
    }

    // MARK: - Error Card

    private func errorCard(message: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundColor(Color(hex: "#ef4444"))
            Text(message)
                .font(.system(size: 14))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.8))
                .lineLimit(3)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(Color(hex: "#ef4444").opacity(0.08))
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .strokeBorder(Color(hex: "#ef4444").opacity(0.2), lineWidth: 1)
                )
        )
    }

    // MARK: - Latency

    private var latencyView: some View {
        HStack(spacing: 16) {
            if viewModel.sttLatency > 0 {
                latencyBadge(label: "STT", seconds: viewModel.sttLatency)
            }
            if viewModel.llmLatency > 0 {
                latencyBadge(label: "LLM", seconds: viewModel.llmLatency)
            }
            Spacer()
        }
    }

    private func latencyBadge(label: String, seconds: TimeInterval) -> some View {
        let ms = Int(seconds * 1000)
        return HStack(spacing: 4) {
            Text(label)
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(Color(hex: "#fafaf9").opacity(0.4))
            Text("\(ms)ms")
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(Color(hex: "#fbbf24").opacity(0.7))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(
            Capsule()
                .fill(Color(hex: "#fafaf9").opacity(0.04))
        )
    }

    // MARK: - Destination Strip

    private var destinationStrip: some View {
        HStack(spacing: 10) {
            destinationButton(icon: "doc.on.clipboard", label: "Copy") {
                if case .result(let transcript, let processed) = viewModel.recordingState {
                    UIPasteboard.general.string = processed ?? transcript
                }
            }
            destinationButton(icon: "message.fill", label: "Messages") {
                if case .result(let transcript, let processed) = viewModel.recordingState {
                    let text = processed ?? transcript
                    let dest = AnyTextDestination(MessagesDestination())
                    viewModel.sendToDestination(dest)
                    _ = text
                }
            }
            destinationButton(icon: "square.and.arrow.up", label: "Share") {
                if case .result(let transcript, let processed) = viewModel.recordingState {
                    let text = processed ?? transcript
                    shareText(text)
                }
            }
            Spacer()
        }
    }

    private func destinationButton(icon: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 18, weight: .medium))
                    .foregroundColor(Color(hex: "#fbbf24"))
                Text(label)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.6))
            }
            .frame(width: 64, height: 56)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(hex: "#fafaf9").opacity(0.04))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12)
                            .strokeBorder(Color(hex: "#fafaf9").opacity(0.06), lineWidth: 1)
                    )
            )
        }
        .buttonStyle(PlainButtonStyle())
        .disabled({
            if case .result = viewModel.recordingState { return false }
            return true
        }())
        .opacity({
            if case .result = viewModel.recordingState { return 1.0 }
            return 0.4
        }())
    }

    // MARK: - Share Sheet

    private func shareText(_ text: String) {
        guard let windowScene = UIApplication.shared.connectedScenes.first as? UIWindowScene,
              let rootVC = windowScene.windows.first?.rootViewController else {
            return
        }
        let av = UIActivityViewController(activityItems: [text], applicationActivities: nil)
        rootVC.present(av, animated: true)
    }
}

#Preview {
    DictationView(viewModel: DictationViewModel())
}

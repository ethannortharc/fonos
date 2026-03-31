import SwiftUI

/// Recording modal sheet for capturing a voice note into a notebook.
struct RecordNoteSheet: View {
    let notebook: NoteContainer
    @ObservedObject var noteViewModel: NoteViewModel
    let onDismiss: () -> Void

    @State private var elapsedSeconds: Int = 0
    @State private var timer: Timer? = nil

    var body: some View {
        ZStack {
            Color(hex: "#1a1917").ignoresSafeArea()

            VStack(spacing: 32) {
                // Header
                VStack(spacing: 6) {
                    Text("Recording to")
                        .font(.system(size: 13))
                        .foregroundColor(Color(hex: "#fafaf9").opacity(0.5))
                    Text(notebook.title)
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundColor(Color(hex: "#fafaf9"))
                }
                .padding(.top, 32)

                // Waveform
                WaveformView(
                    audioLevel: noteViewModel.audioLevel,
                    isRecording: noteViewModel.recordingState == .recording
                )
                .padding(.horizontal, 32)

                // Timer
                if noteViewModel.recordingState == .recording {
                    Text(formattedTime)
                        .font(.system(size: 28, weight: .light, design: .monospaced))
                        .foregroundColor(Color(hex: "#fafaf9"))
                }

                // State-dependent content
                switch noteViewModel.recordingState {
                case .recording:
                    stopButton

                case .processing:
                    VStack(spacing: 12) {
                        ProgressView()
                            .progressViewStyle(CircularProgressViewStyle(tint: Color(hex: "#fbbf24")))
                            .scaleEffect(1.4)
                        Text("Processing...")
                            .font(.system(size: 14))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.6))
                    }

                case .done:
                    VStack(spacing: 12) {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 40))
                            .foregroundColor(Color(hex: "#86efac"))
                        Text("Saved")
                            .font(.system(size: 16, weight: .medium))
                            .foregroundColor(Color(hex: "#86efac"))
                    }
                    .onAppear {
                        Task {
                            try? await Task.sleep(nanoseconds: 800_000_000)
                            onDismiss()
                        }
                    }

                case .error(let message):
                    VStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.system(size: 32))
                            .foregroundColor(Color(hex: "#ef4444"))
                        Text(message)
                            .font(.system(size: 13))
                            .foregroundColor(Color(hex: "#fafaf9").opacity(0.7))
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 32)
                        Button("Dismiss") { onDismiss() }
                            .foregroundColor(Color(hex: "#fbbf24"))
                    }

                case .idle:
                    EmptyView()
                }

                Spacer()
            }
        }
        .onAppear {
            noteViewModel.startRecording()
            startTimer()
        }
        .onDisappear {
            stopTimer()
        }
    }

    // MARK: - Stop Button

    private var stopButton: some View {
        Button {
            noteViewModel.stopRecording(
                to: notebook.id,
                mode: notebook.processingMode
            )
            stopTimer()
        } label: {
            ZStack {
                Circle()
                    .fill(Color(hex: "#ef4444"))
                    .frame(width: 80, height: 80)
                    .shadow(color: Color(hex: "#ef4444").opacity(0.4), radius: 12)
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color.white)
                    .frame(width: 28, height: 28)
            }
        }
    }

    // MARK: - Timer

    private var formattedTime: String {
        let minutes = elapsedSeconds / 60
        let seconds = elapsedSeconds % 60
        return String(format: "%02d:%02d", minutes, seconds)
    }

    private func startTimer() {
        elapsedSeconds = 0
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
            elapsedSeconds += 1
        }
    }

    private func stopTimer() {
        timer?.invalidate()
        timer = nil
    }
}

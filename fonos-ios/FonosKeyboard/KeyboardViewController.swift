import UIKit
import AVFoundation
import Speech
import os.log

private let kbLog = Logger(subsystem: "com.fonos.ios.keyboard", category: "KeyboardVC")

// MARK: - KeyboardViewController

final class KeyboardViewController: UIInputViewController {

    // MARK: - State

    private enum RecordingState {
        case idle
        case recording
        case processing
    }

    private var recordingState: RecordingState = .idle {
        didSet { updateUI() }
    }

    // MARK: - Services

    private let audioService = KeyboardAudioService()
    private let sttService = KeyboardSTTService()

    // MARK: - UI Elements

    private let containerView = UIView()
    private let globeButton = UIButton(type: .system)
    private let modeLabel = UILabel()
    private let micButton = UIButton(type: .custom)
    private let statusLabel = UILabel()
    private let dismissButton = UIButton(type: .system)

    // MARK: - Colors

    private let bgColor = UIColor(red: 0x1a/255.0, green: 0x19/255.0, blue: 0x17/255.0, alpha: 1)
    private let amberColor = UIColor(red: 0xfb/255.0, green: 0xbf/255.0, blue: 0x24/255.0, alpha: 1)
    private let redColor = UIColor(red: 0xef/255.0, green: 0x44/255.0, blue: 0x44/255.0, alpha: 1)
    private let textColor = UIColor(red: 0xfa/255.0, green: 0xfa/255.0, blue: 0xf9/255.0, alpha: 1)
    private let dimColor = UIColor.white.withAlphaComponent(0.5)

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        setupUI()
        updateUI()
        requestPermissions()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
    }

    // MARK: - UI Setup

    private func setupUI() {
        view.backgroundColor = bgColor

        // Container fills the view with a fixed height
        containerView.backgroundColor = bgColor
        containerView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(containerView)

        NSLayoutConstraint.activate([
            containerView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            containerView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            containerView.topAnchor.constraint(equalTo: view.topAnchor),
            containerView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            containerView.heightAnchor.constraint(equalToConstant: 120),
        ])

        setupGlobeButton()
        setupModeLabel()
        setupMicButton()
        setupStatusLabel()
        setupDismissButton()
        layoutElements()
    }

    private func setupGlobeButton() {
        let config = UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
        let image = UIImage(systemName: "globe", withConfiguration: config)
        globeButton.setImage(image, for: .normal)
        globeButton.tintColor = dimColor
        globeButton.translatesAutoresizingMaskIntoConstraints = false
        globeButton.addTarget(self, action: #selector(globeTapped), for: .touchUpInside)
        containerView.addSubview(globeButton)
    }

    private func setupModeLabel() {
        modeLabel.font = UIFont.systemFont(ofSize: 11, weight: .medium)
        modeLabel.textColor = dimColor
        modeLabel.textAlignment = .center
        modeLabel.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(modeLabel)

        // Read mode name from config
        let modeName = readModeName()
        modeLabel.text = modeName
    }

    private func setupMicButton() {
        micButton.translatesAutoresizingMaskIntoConstraints = false
        micButton.layer.cornerRadius = 24
        micButton.clipsToBounds = true
        micButton.addTarget(self, action: #selector(micTapped), for: .touchUpInside)
        containerView.addSubview(micButton)
    }

    private func setupStatusLabel() {
        statusLabel.font = UIFont.systemFont(ofSize: 13, weight: .regular)
        statusLabel.textColor = textColor
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 2
        statusLabel.lineBreakMode = .byWordWrapping
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(statusLabel)
    }

    private func setupDismissButton() {
        let config = UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
        let image = UIImage(systemName: "keyboard.chevron.compact.down", withConfiguration: config)
        dismissButton.setImage(image, for: .normal)
        dismissButton.tintColor = dimColor
        dismissButton.translatesAutoresizingMaskIntoConstraints = false
        dismissButton.addTarget(self, action: #selector(dismissTapped), for: .touchUpInside)
        containerView.addSubview(dismissButton)
    }

    private func layoutElements() {
        // Two-row layout:
        // Row 1 (top):  [globe] [modeLabel]  [micButton]  [dismiss]
        // Row 2 (bottom): [statusLabel — full width, multiline]

        let topY: CGFloat = 28  // center Y for top row
        let micSize: CGFloat = 52

        NSLayoutConstraint.activate([
            // Globe: top-left
            globeButton.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 12),
            globeButton.centerYAnchor.constraint(equalTo: containerView.topAnchor, constant: topY),
            globeButton.widthAnchor.constraint(equalToConstant: 36),
            globeButton.heightAnchor.constraint(equalToConstant: 36),

            // Mode label: right of globe
            modeLabel.leadingAnchor.constraint(equalTo: globeButton.trailingAnchor, constant: 6),
            modeLabel.centerYAnchor.constraint(equalTo: containerView.topAnchor, constant: topY),
            modeLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 100),

            // Mic button: center top
            micButton.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
            micButton.centerYAnchor.constraint(equalTo: containerView.topAnchor, constant: topY),
            micButton.widthAnchor.constraint(equalToConstant: micSize),
            micButton.heightAnchor.constraint(equalToConstant: micSize),

            // Dismiss: top-right
            dismissButton.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -12),
            dismissButton.centerYAnchor.constraint(equalTo: containerView.topAnchor, constant: topY),
            dismissButton.widthAnchor.constraint(equalToConstant: 36),
            dismissButton.heightAnchor.constraint(equalToConstant: 36),

            // Status label: bottom row, full width
            statusLabel.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -16),
            statusLabel.topAnchor.constraint(equalTo: containerView.topAnchor, constant: topY + micSize / 2 + 8),
            statusLabel.bottomAnchor.constraint(lessThanOrEqualTo: containerView.bottomAnchor, constant: -8),
        ])

        micButton.layer.cornerRadius = micSize / 2
    }

    // MARK: - UI Update

    private func updateUI() {
        switch recordingState {
        case .idle:
            micButton.backgroundColor = amberColor
            let config = UIImage.SymbolConfiguration(pointSize: 20, weight: .bold)
            micButton.setImage(UIImage(systemName: "mic.fill", withConfiguration: config), for: .normal)
            micButton.tintColor = .black
            statusLabel.text = "Ready"
            statusLabel.textColor = textColor

        case .recording:
            micButton.backgroundColor = redColor
            let config = UIImage.SymbolConfiguration(pointSize: 20, weight: .bold)
            micButton.setImage(UIImage(systemName: "stop.fill", withConfiguration: config), for: .normal)
            micButton.tintColor = .white
            statusLabel.text = "Recording..."
            statusLabel.textColor = redColor

        case .processing:
            micButton.backgroundColor = amberColor.withAlphaComponent(0.5)
            let config = UIImage.SymbolConfiguration(pointSize: 20, weight: .bold)
            micButton.setImage(UIImage(systemName: "ellipsis", withConfiguration: config), for: .normal)
            micButton.tintColor = .black
            statusLabel.text = "Processing..."
            statusLabel.textColor = textColor
        }
    }

    // MARK: - Actions

    @objc private func globeTapped() {
        advanceToNextInputMode()
    }

    @objc private func dismissTapped() {
        dismissKeyboard()
    }

    @objc private func micTapped() {
        switch recordingState {
        case .idle:
            startRecording()
        case .recording:
            stopRecordingAndTranscribe()
        case .processing:
            break // ignore taps while processing
        }
    }

    // MARK: - Recording Flow

    private func startRecording() {
        // Check if "Allow Full Access" is enabled (required for mic in keyboard extension)
        guard isFullAccessEnabled else {
            showError("Enable Full Access")
            kbLog.error("Full Access not enabled — go to Settings → Keyboards → Fonos → Allow Full Access")
            return
        }

        audioService.startCapture { [weak self] error in
            DispatchQueue.main.async {
                if let error {
                    kbLog.error("❌ Capture failed: \(error.localizedDescription)")
                    self?.showError(error.localizedDescription)
                } else {
                    self?.recordingState = .recording
                }
            }
        }
    }

    private func stopRecordingAndTranscribe() {
        guard let wavData = audioService.stopCapture() else {
            kbLog.error("⏹ No WAV data returned from recorder")
            showError("No audio data")
            recordingState = .idle
            return
        }

        kbLog.info("⏹ Got WAV data: \(wavData.count) bytes")
        statusLabel.text = "Processing \(wavData.count / 1024)KB..."
        recordingState = .processing

        let stt = sttService
        Task {
            do {
                kbLog.info("🔄 Starting transcription...")
                let transcript = try await stt.transcribe(audioData: wavData)
                kbLog.info("✅ Transcript: \(transcript.prefix(50))...")
                await MainActor.run {
                    self.textDocumentProxy.insertText(transcript)
                    self.statusLabel.text = "Done ✓"
                    self.statusLabel.textColor = UIColor(red: 0x86/255, green: 0xef/255, blue: 0xac/255, alpha: 1)
                    self.recordingState = .idle
                    // Reset status after 2s
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
                        self?.statusLabel.text = "Ready"
                        self?.statusLabel.textColor = self?.textColor
                    }
                }
            } catch {
                kbLog.error("❌ Transcription failed: \(error.localizedDescription)")
                await MainActor.run {
                    self.showError(error.localizedDescription)
                    self.recordingState = .idle
                }
            }
        }
    }

    // MARK: - Text Insertion

    private func insertText(_ text: String) {
        textDocumentProxy.insertText(text)
    }

    // MARK: - Error Display

    private func showError(_ message: String) {
        statusLabel.text = message
        statusLabel.textColor = redColor
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            self?.statusLabel.text = "Ready"
            self?.statusLabel.textColor = self?.textColor
        }
    }

    // MARK: - Permissions

    /// Check if "Allow Full Access" is enabled for this keyboard extension.
    /// Without it, mic and network access are blocked.
    private var isFullAccessEnabled: Bool {
        // The most reliable way to check is hasFullAccess (inherited from UIInputViewController)
        return self.hasFullAccess
    }

    private func requestPermissions() {
        // In keyboard extension, mic permission must be requested AFTER "Allow Full Access" is granted
        if isFullAccessEnabled {
            AVAudioSession.sharedInstance().requestRecordPermission { granted in
                if !granted {
                    DispatchQueue.main.async { [weak self] in
                        self?.showError("Allow mic access")
                    }
                }
            }
        }
    }

    // MARK: - Config Helper

    private func readModeName() -> String {
        guard let data = UserDefaults.standard.data(forKey: "app_config") else {
            return "Dictate"
        }

        struct MinConfig: Decodable {
            var activeModeID: String?
            var modeConfigs: [MinModeConfig]?
        }
        struct MinModeConfig: Decodable {
            var id: String
            var displayName: String?
            var mode: MinMode?
        }
        struct MinMode: Decodable {
            var name: String?
        }

        guard let config = try? JSONDecoder().decode(MinConfig.self, from: data) else {
            return "Dictate"
        }

        let activeID = config.activeModeID ?? "raw"
        if let modeConfig = config.modeConfigs?.first(where: { $0.id == activeID }),
           let name = modeConfig.displayName ?? modeConfig.mode?.name {
            return name
        }

        return "Dictate"
    }
}

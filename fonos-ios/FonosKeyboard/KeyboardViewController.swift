import UIKit
import AVFoundation
import os.log

private let kbLog = Logger(subsystem: "com.fonos.ios.keyboard", category: "KeyboardVC")

final class KeyboardViewController: UIInputViewController {

    // MARK: - State

    private enum KeyboardState {
        case ready
        case recording
        case error(String)
        case done
    }

    private var keyboardState: KeyboardState = .ready {
        didSet { updateUI() }
    }

    // MARK: - Services

    private let audioService = KeyboardAudioService()
    private let sttService = KeyboardSTTService()

    // MARK: - UI Elements

    private let containerView = UIView()
    private let modeButton = UIButton(type: .system)
    private let micButton = UIButton(type: .custom)
    private let statusLabel = UILabel()
    private let switchButton = UIButton(type: .system)

    // MARK: - Colors

    private let amberColor = UIColor(red: 0xfb/255, green: 0xbf/255, blue: 0x24/255, alpha: 1)
    private let redColor = UIColor(red: 0xef/255, green: 0x44/255, blue: 0x44/255, alpha: 1)
    private let greenColor = UIColor(red: 0x86/255, green: 0xef/255, blue: 0xac/255, alpha: 1)

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        setupUI()
        updateUI()
    }

    // MARK: - UI Setup

    private func setupUI() {
        view.backgroundColor = .clear
        containerView.backgroundColor = .clear
        containerView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(containerView)

        NSLayoutConstraint.activate([
            containerView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            containerView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            containerView.topAnchor.constraint(equalTo: view.topAnchor),
            containerView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            containerView.heightAnchor.constraint(equalToConstant: 100),
        ])

        // Mode button (left) — shows current mode, long-press to switch keyboard
        modeButton.setTitle("✦ Dictate", for: .normal)
        modeButton.setTitleColor(amberColor, for: .normal)
        modeButton.titleLabel?.font = .systemFont(ofSize: 13, weight: .semibold)
        modeButton.translatesAutoresizingMaskIntoConstraints = false
        modeButton.addTarget(self, action: #selector(modeTapped), for: .touchUpInside)
        let longPress = UILongPressGestureRecognizer(target: self, action: #selector(switchKeyboard(_:)))
        modeButton.addGestureRecognizer(longPress)
        containerView.addSubview(modeButton)

        // Mic button (center)
        micButton.translatesAutoresizingMaskIntoConstraints = false
        micButton.layer.cornerRadius = 28
        micButton.clipsToBounds = true
        micButton.addTarget(self, action: #selector(micTapped), for: .touchUpInside)
        containerView.addSubview(micButton)

        // Status label (bottom, full width)
        statusLabel.font = .systemFont(ofSize: 13, weight: .regular)
        statusLabel.textColor = .secondaryLabel
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 2
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(statusLabel)

        // Switch keyboard button (right) — small, for switching keyboards
        let switchConfig = UIImage.SymbolConfiguration(pointSize: 14, weight: .regular)
        switchButton.setImage(UIImage(systemName: "keyboard.chevron.compact.down", withConfiguration: switchConfig), for: .normal)
        switchButton.tintColor = .tertiaryLabel
        switchButton.translatesAutoresizingMaskIntoConstraints = false
        switchButton.addTarget(self, action: #selector(dismissTapped), for: .touchUpInside)
        containerView.addSubview(switchButton)

        // Layout
        let micSize: CGFloat = 56
        NSLayoutConstraint.activate([
            // Mode: top-left
            modeButton.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 16),
            modeButton.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 12),

            // Mic: center-top
            micButton.centerXAnchor.constraint(equalTo: containerView.centerXAnchor),
            micButton.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 8),
            micButton.widthAnchor.constraint(equalToConstant: micSize),
            micButton.heightAnchor.constraint(equalToConstant: micSize),

            // Switch: top-right
            switchButton.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -16),
            switchButton.topAnchor.constraint(equalTo: containerView.topAnchor, constant: 12),

            // Status: below mic
            statusLabel.leadingAnchor.constraint(equalTo: containerView.leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: containerView.trailingAnchor, constant: -16),
            statusLabel.topAnchor.constraint(equalTo: micButton.bottomAnchor, constant: 6),
        ])

        // Read mode name
        modeButton.setTitle("✦ \(readModeName())", for: .normal)
    }

    // MARK: - UI Update

    private func updateUI() {
        let symConfig = UIImage.SymbolConfiguration(pointSize: 22, weight: .bold)
        switch keyboardState {
        case .ready:
            micButton.backgroundColor = amberColor
            micButton.setImage(UIImage(systemName: "mic.fill", withConfiguration: symConfig), for: .normal)
            micButton.tintColor = .black
            micButton.isEnabled = true
            statusLabel.text = "Tap to dictate"
            statusLabel.textColor = .secondaryLabel

        case .recording:
            micButton.backgroundColor = redColor
            micButton.setImage(UIImage(systemName: "stop.fill", withConfiguration: symConfig), for: .normal)
            micButton.tintColor = .white
            statusLabel.text = "Recording... tap to stop"
            statusLabel.textColor = redColor

        case .error(let msg):
            micButton.backgroundColor = amberColor
            micButton.setImage(UIImage(systemName: "mic.fill", withConfiguration: symConfig), for: .normal)
            micButton.tintColor = .black
            micButton.isEnabled = true
            statusLabel.text = msg
            statusLabel.textColor = redColor

        case .done:
            micButton.backgroundColor = amberColor
            micButton.setImage(UIImage(systemName: "mic.fill", withConfiguration: symConfig), for: .normal)
            micButton.tintColor = .black
            micButton.isEnabled = true
            statusLabel.text = "✓ Text inserted"
            statusLabel.textColor = greenColor
        }
    }

    // MARK: - Actions

    @objc private func modeTapped() {
        // Cycle through modes could be added later
        // For now, show a hint about long-press
        let prev = statusLabel.text
        statusLabel.text = "Long-press to switch keyboard"
        statusLabel.textColor = .secondaryLabel
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in
            self?.statusLabel.text = prev
        }
    }

    @objc private func switchKeyboard(_ gesture: UILongPressGestureRecognizer) {
        if gesture.state == .began {
            advanceToNextInputMode()
        }
    }

    @objc private func dismissTapped() {
        dismissKeyboard()
    }

    @objc private func micTapped() {
        switch keyboardState {
        case .ready, .error, .done:
            startRecording()
        case .recording:
            stopAndTranscribe()
        }
    }

    // MARK: - Recording

    private func startRecording() {
        guard hasFullAccess else {
            keyboardState = .error("Enable Full Access in Settings → Keyboards → Fonos")
            return
        }

        audioService.startCapture { [weak self] error in
            DispatchQueue.main.async {
                if let error {
                    kbLog.error("❌ \(error.localizedDescription)")
                    // AVAudioRecorder doesn't work — try opening main app
                    self?.openMainAppForRecording()
                } else {
                    self?.keyboardState = .recording
                }
            }
        }
    }

    private func stopAndTranscribe() {
        guard let result = audioService.stopCapture() else {
            keyboardState = .error("No audio captured")
            return
        }

        kbLog.info("⏹ KB: \(result.wavData.count) bytes")
        statusLabel.text = "Processing..."
        statusLabel.textColor = .secondaryLabel
        micButton.isEnabled = false

        let stt = sttService
        let fileURL = result.fileURL
        let wavData = result.wavData

        Task {
            do {
                let transcript = try await stt.transcribe(fileURL: fileURL, audioData: wavData)
                kbLog.info("✅ KB: \(transcript.prefix(50))...")
                await MainActor.run {
                    self.textDocumentProxy.insertText(transcript)
                    self.keyboardState = .done
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
                        if case .done = self?.keyboardState { self?.keyboardState = .ready }
                    }
                }
            } catch {
                kbLog.error("❌ KB STT: \(error.localizedDescription)")
                await MainActor.run {
                    self.keyboardState = .error(error.localizedDescription)
                }
            }
        }
    }

    // MARK: - Fallback: Open Main App

    private func openMainAppForRecording() {
        // When AVAudioRecorder doesn't work in extension, open main app
        guard let url = URL(string: "fonos://record") else { return }

        // Keyboard extensions can open URLs via the shared application
        var responder: UIResponder? = self
        while let r = responder {
            if let application = r as? UIApplication {
                application.open(url)
                keyboardState = .error("Opened Fonos app — record there, then paste")
                return
            }
            responder = r.next
        }

        // Can't open URL — show instruction
        keyboardState = .error("Open Fonos app to record, then paste here")
    }

    // MARK: - Config

    private var isFullAccessEnabled: Bool {
        hasFullAccess
    }

    private func readModeName() -> String {
        guard let data = UserDefaults.standard.data(forKey: "app_config") else {
            return "Dictate"
        }
        struct MinConfig: Decodable { var activeModeID: String? }
        guard let config = try? JSONDecoder().decode(MinConfig.self, from: data) else {
            return "Dictate"
        }
        let modeNames = ["raw": "Raw", "polish": "Polish", "formal": "Formal", "translate": "Translate"]
        return modeNames[config.activeModeID ?? "raw"] ?? "Dictate"
    }
}

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

        // Log extension state for debugging
        let perm = AVAudioSession.sharedInstance().recordPermission
        let fullAccess = hasFullAccess
        kbLog.info("🔑 KB viewDidLoad: fullAccess=\(fullAccess), micPermission=\(perm.rawValue) (0=undetermined, 1=denied, 2=granted)")
        statusLabel.text = "Tap to dictate (mic:\(perm.rawValue) fa:\(fullAccess))"

        // Pre-request mic permission on load so dialog appears early
        if fullAccess {
            AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
                DispatchQueue.main.async {
                    kbLog.info("🔑 KB mic request result: \(granted)")
                    if !granted {
                        self?.keyboardState = .error("Mic denied — enable in Settings → Privacy → Microphone")
                    }
                }
            }
        }
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
            containerView.heightAnchor.constraint(equalToConstant: 140),
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
        statusLabel.font = .systemFont(ofSize: 10, weight: .regular)
        statusLabel.textColor = .secondaryLabel
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0  // unlimited — show full diagnostic
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
            statusLabel.text = "Tap to dictate (v3)"
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

        statusLabel.text = "Starting..."
        statusLabel.textColor = .secondaryLabel

        audioService.startRecording(
            onPartial: { [weak self] text in
                // Live partial results (from App Group → main app's Apple Speech)
                DispatchQueue.main.async {
                    self?.statusLabel.text = text.isEmpty ? "Listening..." : text
                    self?.statusLabel.textColor = .label
                }
            },
            onStatus: { [weak self] status in
                // App Group result notification
                DispatchQueue.main.async {
                    guard let self else { return }
                    if status.hasPrefix("done:") {
                        let text = String(status.dropFirst(5))
                        if !text.isEmpty {
                            self.textDocumentProxy.insertText(text)
                            kbLog.info("✅ Inserted: \(text.prefix(50))...")
                            self.keyboardState = .done
                            self.autoReset()
                        } else {
                            self.keyboardState = .error("No speech detected")
                        }
                        self.audioService.cleanup()
                    } else if status.hasPrefix("error:") {
                        let err = String(status.dropFirst(6))
                        self.keyboardState = .error(err)
                        self.audioService.cleanup()
                    }
                }
            },
            completion: { [weak self] error in
                guard let self else { return }
                if let error {
                    let log = self.audioService.diagnosticLog.joined(separator: " → ")
                    self.keyboardState = .error("\(error.localizedDescription)\n[\(log)]")
                } else {
                    let strategy = self.audioService.activeStrategy == .direct ? "Direct" : "App Group"
                    self.keyboardState = .recording
                    self.statusLabel.text = "Recording [\(strategy)]..."
                    self.statusLabel.textColor = self.redColor
                }
            }
        )
    }

    private func stopAndTranscribe() {
        let strategy = audioService.activeStrategy
        audioService.stopRecording()

        if strategy == .direct {
            // Direct recording: transcribe locally
            statusLabel.text = "Processing..."
            statusLabel.textColor = .secondaryLabel

            audioService.transcribeDirectRecording(language: nil) { [weak self] text, error in
                DispatchQueue.main.async {
                    guard let self else { return }
                    if let text, !text.isEmpty {
                        self.textDocumentProxy.insertText(text)
                        kbLog.info("✅ Inserted: \(text.prefix(50))...")
                        self.keyboardState = .done
                        self.autoReset()
                    } else {
                        self.keyboardState = .error(error?.localizedDescription ?? "No speech")
                    }
                    self.audioService.cleanup()
                }
            }
        } else {
            // App Group: main app is processing, show status
            keyboardState = .ready  // allow re-tap
            statusLabel.text = "Processing (main app)..."
            statusLabel.textColor = .secondaryLabel
            // Result comes via onStatus callback — add timeout as safety
            DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self] in
                guard let self else { return }
                if self.statusLabel.text?.contains("Processing") == true {
                    self.keyboardState = .error("Timeout — check if Fonos app is open")
                    self.audioService.cleanup()
                }
            }
        }
    }

    private func autoReset() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            if case .done = self?.keyboardState { self?.keyboardState = .ready }
        }
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

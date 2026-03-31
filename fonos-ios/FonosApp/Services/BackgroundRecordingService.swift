import UIKit
import AVFoundation
import Speech
import os.log

private let bgLog = Logger(subsystem: "com.fonos.ios", category: "BackgroundRec")

/// Background recording service for keyboard extension.
///
/// Strategy: Start AVAudioRecorder while app is in FOREGROUND (on launch).
/// iOS allows audio to CONTINUE in background but not START.
/// When keyboard requests recording, we mark timestamps and extract the segment.
final class BackgroundRecordingService: NSObject, @unchecked Sendable {
    static let shared = BackgroundRecordingService()

    private let groupID = "group.com.fonos.ios"

    // Continuous recorder — started in foreground, runs in background
    private var recorder: AVAudioRecorder?
    private var recordingStartTime: TimeInterval = 0
    private var isKeyboardRecording = false

    private static nonisolated(unsafe) let kbStart = "com.fonos.ios.kb.start" as CFString
    private static nonisolated(unsafe) let kbStop  = "com.fonos.ios.kb.stop" as CFString

    private override init() { super.init() }

    // MARK: - Registration (called on app launch, while in FOREGROUND)

    func register() {
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        let observer = Unmanaged.passUnretained(self).toOpaque()

        CFNotificationCenterAddObserver(center, observer, { _, obs, _, _, _ in
            guard let obs else { return }
            Unmanaged<BackgroundRecordingService>.fromOpaque(obs).takeUnretainedValue().onStart()
        }, Self.kbStart, nil, .deliverImmediately)

        CFNotificationCenterAddObserver(center, observer, { _, obs, _, _, _ in
            guard let obs else { return }
            Unmanaged<BackgroundRecordingService>.fromOpaque(obs).takeUnretainedValue().onStop()
        }, Self.kbStop, nil, .deliverImmediately)

        // Verify App Group
        if let url = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: groupID) {
            bgLog.info("✅ App Group: \(url.path)")
        } else {
            bgLog.error("❌ App Group NOT accessible!")
        }

        // Configure + activate audio session (MUST happen in foreground)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers, .allowBluetoothHFP])
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            bgLog.info("✅ Audio session active")
        } catch {
            bgLog.error("❌ Audio session: \(error.localizedDescription)")
        }

        // Start continuous recording — runs in background with audio background mode
        startContinuousRecording()

        // Heartbeat — also checks recorder health
        writeFile("kb_heartbeat", "\(Date().timeIntervalSince1970)")
        Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.writeFile("kb_heartbeat", "\(Date().timeIntervalSince1970)")
            // Try to resume if recorder died (resume works, new record doesn't from background)
            if let rec = self.recorder, !rec.isRecording, !self.isKeyboardRecording {
                if rec.record() {
                    bgLog.info("🎙 BG: Recorder auto-resumed")
                }
                // If resume fails, will restart when app is foregrounded
            }
        }

        // Restart recorder when app comes to foreground
        NotificationCenter.default.addObserver(forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self else { return }
            if self.recorder == nil || self.recorder?.isRecording != true {
                bgLog.info("🎙 BG: App foregrounded, restarting recorder")
                self.startContinuousRecording()
            }
        }

        // Handle audio interruptions (phone calls, other apps)
        NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] notification in
            guard let self,
                  let info = notification.userInfo,
                  let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
                  let type = AVAudioSession.InterruptionType(rawValue: typeValue) else { return }

            if type == .began {
                bgLog.info("🎙 BG: Audio interrupted")
            } else if type == .ended {
                bgLog.info("🎙 BG: Interruption ended")
                try? AVAudioSession.sharedInstance().setActive(true, options: .notifyOthersOnDeactivation)
                // Resume EXISTING recorder (don't create new — new fails from background)
                if let rec = self.recorder, rec.record() {
                    bgLog.info("🎙 BG: ✅ Recorder resumed after interruption")
                } else {
                    bgLog.warning("🎙 BG: Resume failed, will restart when app foregrounded")
                    // Can't start new recorder from background — will restart on foreground
                }
            }
        }

        bgLog.info("✅ BackgroundRecordingService registered")
    }

    // MARK: - Continuous Recording

    private func startContinuousRecording() {
        guard let url = containerURL("kb_continuous.wav") else { return }
        try? FileManager.default.removeItem(at: url)

        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatLinearPCM),
            AVSampleRateKey: 16000,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]

        do {
            let rec = try AVAudioRecorder(url: url, settings: settings)
            rec.isMeteringEnabled = true
            if rec.record() {
                recorder = rec
                bgLog.info("🎙 Continuous recording started (keeps audio alive in background)")
            } else {
                bgLog.error("❌ Continuous recorder.record() failed")
            }
        } catch {
            bgLog.error("❌ Recorder init: \(error.localizedDescription)")
        }
    }

    // MARK: - Keyboard Start (just marks timestamp — recorder already running)

    private func onStart() {
        DispatchQueue.main.async { [self] in
            guard let rec = recorder, rec.isRecording else {
                bgLog.error("🎙 BG: Recorder not active! Open Fonos app to restart.")
                writeError("Recorder stopped. Open Fonos app, then try again.")
                return
            }

            guard !isKeyboardRecording else {
                bgLog.warning("Already in keyboard recording")
                return
            }

            recordingStartTime = rec.currentTime
            isKeyboardRecording = true
            writeStatus("recording")
            bgLog.info("🎙 BG: ✅ Keyboard recording started at \(String(format: "%.1f", self.recordingStartTime))s")
        }
    }

    // MARK: - Keyboard Stop (extracts segment, transcribes)

    private var bgTaskID: UIBackgroundTaskIdentifier = .invalid

    private func onStop() {
        DispatchQueue.main.async { [self] in
            guard isKeyboardRecording, let rec = recorder else {
                bgLog.warning("Not keyboard-recording, ignoring stop")
                return
            }

            let endTime = rec.currentTime
            isKeyboardRecording = false
            bgLog.info("🎙 BG: Stop. Segment: \(String(format: "%.1f", self.recordingStartTime))s → \(String(format: "%.1f", endTime))s")

            // Background task for transcription
            bgTaskID = UIApplication.shared.beginBackgroundTask {
                bgLog.warning("🎙 BG: Background task expiring")
            }

            writeStatus("processing")

            // Recorder keeps running — DON'T pause or stop it!
            // Reading the file while recording is safe because WAV is sequential
            // and we only read bytes up to endTime (already captured above).
            guard let url = containerURL("kb_continuous.wav"),
                  let fullData = try? Data(contentsOf: url) else {
                writeError("Cannot read WAV")
                endBackgroundTask()
                return
            }

            let wavData = extractWAVSegment(fullData, from: recordingStartTime, to: endTime, sampleRate: 16000)
            bgLog.info("🎙 BG: Extracted segment: \(wavData.count) bytes")

            guard wavData.count > 100 else {
                writeError("Recording too short")
                endBackgroundTask()
                return
            }

            // Save extracted segment for STT
            if let segURL = containerURL("kb_recording.wav") {
                try? wavData.write(to: segURL)
            }

            transcribeAndProcess(wavData: wavData)
        }
    }

    // MARK: - WAV Segment Extraction

    private func extractWAVSegment(_ data: Data, from startTime: TimeInterval, to endTime: TimeInterval, sampleRate: Int) -> Data {
        let bytesPerSample = 2  // 16-bit mono
        let headerSize = 44
        let bytesPerSecond = sampleRate * bytesPerSample

        let startByte = headerSize + Int(startTime * Double(bytesPerSecond))
        let endByte = min(data.count, headerSize + Int(endTime * Double(bytesPerSecond)))

        guard startByte < endByte, startByte < data.count else { return Data() }

        let pcmData = data[startByte..<endByte]
        let dataSize = UInt32(pcmData.count)

        // Build WAV with proper header
        var wav = Data(capacity: 44 + Int(dataSize))
        func u16(_ v: UInt16) { var x = v.littleEndian; Swift.withUnsafeBytes(of: &x) { wav.append(contentsOf: $0) } }
        func u32(_ v: UInt32) { var x = v.littleEndian; Swift.withUnsafeBytes(of: &x) { wav.append(contentsOf: $0) } }
        wav.append(contentsOf: "RIFF".utf8); u32(36 + dataSize)
        wav.append(contentsOf: "WAVE".utf8)
        wav.append(contentsOf: "fmt ".utf8); u32(16); u16(1); u16(1)
        u32(UInt32(sampleRate)); u32(UInt32(sampleRate * bytesPerSample)); u16(UInt16(bytesPerSample)); u16(16)
        wav.append(contentsOf: "data".utf8); u32(dataSize)
        wav.append(pcmData)
        return wav
    }

    // MARK: - Transcribe & Process

    private func transcribeAndProcess(wavData: Data) {
        let config = readAppConfig()
        let sttProvider = resolveSTTProvider(config: config)
        let mode = resolveMode(config: config)
        let language = config.sttLanguage == "auto" ? nil : config.sttLanguage

        bgLog.info("🎙 BG: STT=\(type(of: sttProvider)), mode=\(mode.displayName)")

        Task {
            do {
                let rawTranscript = try await sttProvider.transcribe(audioData: wavData, language: language)
                bgLog.info("🎙 BG: Transcript: \"\(rawTranscript.prefix(50))...\"")

                var finalText = rawTranscript
                if mode.requiresLLM, let llm = resolveLLMService(config: config) {
                    bgLog.info("🎙 BG: LLM (\(mode.displayName))...")
                    finalText = (try? await llm.process(text: rawTranscript, mode: mode)) ?? rawTranscript
                }

                writeFinal(finalText)
                bgLog.info("🎙 BG: ✅ Done: \"\(finalText.prefix(50))...\"")
            } catch {
                bgLog.error("🎙 BG: ❌ STT failed: \(error.localizedDescription)")
                writeError("STT: \(error.localizedDescription)")
            }
            self.endBackgroundTask()
        }
    }

    private func endBackgroundTask() {
        if bgTaskID != .invalid {
            UIApplication.shared.endBackgroundTask(bgTaskID)
            bgTaskID = .invalid
        }
    }

    // MARK: - Provider Resolution

    private func readAppConfig() -> AppConfig {
        guard let data = UserDefaults.standard.data(forKey: "app_config"),
              let config = try? JSONDecoder().decode(AppConfig.self, from: data) else {
            return AppConfig()
        }
        return config
    }

    private func resolveSTTProvider(config: AppConfig) -> any STTProvider {
        let profileID = config.sttProfile
        guard !profileID.isEmpty,
              let profile = config.modelProfiles.first(where: { $0.id == profileID }) else {
            return AppleSTT()
        }
        let key = (try? KeychainStore(service: "com.fonos.models").get(profile.id)) ?? ""
        let baseURL = profile.baseURL ?? ""
        if profile.hasSTT && !baseURL.isEmpty {
            return WhisperSTT(apiKey: key, baseURL: baseURL, modelID: profile.modelID)
        }
        return AppleSTT()
    }

    private func resolveLLMService(config: AppConfig) -> LLMService? {
        let profileID = config.llmProfile
        guard !profileID.isEmpty,
              let profile = config.modelProfiles.first(where: { $0.id == profileID }) else {
            return nil
        }
        let key = (try? KeychainStore(service: "com.fonos.models").get(profile.id)) ?? ""
        let baseURL = profile.baseURL ?? ""
        return LLMService(apiKey: key, modelID: profile.modelID, baseURL: baseURL.isEmpty ? "https://api.openai.com" : baseURL)
    }

    private func resolveMode(config: AppConfig) -> Mode {
        switch config.activeModeID {
        case "polish": return .polish
        case "formal": return .formal
        case "translate": return .translate(targetLanguage: config.translateTargetLanguage ?? "en")
        default: return .raw
        }
    }

    // MARK: - File-based IPC

    private func writeFile(_ name: String, _ content: String) {
        guard let url = containerURL(name) else { return }
        try? content.write(to: url, atomically: true, encoding: .utf8)
    }

    private func writeStatus(_ status: String) {
        writeFile("kb_status", status)
        bgLog.info("🎙 BG: status → \(status)")
    }

    private func writeFinal(_ text: String) {
        writeFile("kb_final.txt", text)
        writeFile("kb_status", "done")
        bgLog.info("🎙 BG: wrote result (\(text.count) chars)")
        postDarwin("com.fonos.ios.kb.result")
    }

    private func writeError(_ msg: String) {
        writeFile("kb_error.txt", msg)
        writeFile("kb_status", "error")
        bgLog.info("🎙 BG: wrote error: \(msg)")
        postDarwin("com.fonos.ios.kb.result")
    }

    private func postDarwin(_ name: String) {
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName(name as CFString),
            nil, nil, true
        )
    }

    private func containerURL(_ filename: String) -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: groupID)?
            .appendingPathComponent(filename)
    }
}

import Foundation
import AVFoundation
import os.log

private let log = Logger(subsystem: "com.fonos.ios", category: "AudioCapture")

// MARK: - Errors

enum AudioCaptureError: Error, LocalizedError {
    case engineStartFailed(Error)
    case sessionSetupFailed(Error)
    case permissionDenied
    case noInputAvailable
    case notRecording

    var errorDescription: String? {
        switch self {
        case .engineStartFailed(let err): return "Audio engine failed to start: \(err.localizedDescription)"
        case .sessionSetupFailed(let err): return "Audio session setup failed: \(err.localizedDescription)"
        case .permissionDenied: return "Microphone permission is required. Please enable it in Settings."
        case .noInputAvailable: return "No audio input available"
        case .notRecording: return "Not currently recording"
        }
    }
}

enum WAVError: Error, LocalizedError {
    case invalidData
    case unsupportedFormat
    case bufferCreationFailed

    var errorDescription: String? {
        switch self {
        case .invalidData: return "WAV data is invalid or too short"
        case .unsupportedFormat: return "WAV format is not supported"
        case .bufferCreationFailed: return "Failed to create audio buffer from WAV data"
        }
    }
}

// MARK: - AudioCaptureService

/// Records audio using AVAudioEngine and produces 16kHz 16-bit mono PCM buffers.
/// Stores up to 60 seconds of audio in a ring buffer.
///
/// Key design decisions:
/// - `captureFormat`, `isRecording`, `encodeToWAV`, and `decodeWAV` are intentionally
///   nonisolated so tests can call them synchronously without MainActor context.
/// - Internal state mutations happen on the main actor via DispatchQueue.main or Task.
final class AudioCaptureService: @unchecked Sendable {
    // MARK: - Public Properties

    /// The capture format: 16kHz, 16-bit integer PCM, mono.
    nonisolated let captureFormat: AVAudioFormat = {
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: false
        ) else {
            fatalError("Failed to create capture format")
        }
        return format
    }()

    /// True while recording.
    private(set) var isRecording: Bool = false

    /// Callback for audio level updates (0.0-1.0). Called on main thread.
    var onAudioLevelUpdate: ((Float) -> Void)?

    // MARK: - Private Properties

    private let engine = AVAudioEngine()
    private let lock = NSLock()

    /// Ring buffer storing raw Int16 PCM samples (16kHz mono = 16000 samples/sec)
    /// Max 60 seconds = 960_000 samples
    private static let maxSamples = 16_000 * 60
    private var ringBuffer: [Int16] = []
    private var ringWriteIndex = 0
    private var totalSamplesWritten = 0

    // MARK: - Init / Deinit

    init() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleInterruption(_:)),
            name: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance()
        )
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    // MARK: - Public API

    /// Check current microphone permission status.
    nonisolated func micPermissionStatus() -> AVAudioSession.RecordPermission {
        AVAudioSession.sharedInstance().recordPermission
    }

    /// Request microphone permission. Call only when status is .undetermined.
    func requestMicPermission() async -> Bool {
        await withCheckedContinuation { continuation in
            AVAudioSession.sharedInstance().requestRecordPermission { granted in
                continuation.resume(returning: granted)
            }
        }
    }

    /// Start audio capture. Must be called AFTER mic permission is granted.
    /// This is intentionally synchronous — AVAudioEngine setup must not
    /// run in an async context to avoid deadlocks with its internal threads.
    @MainActor
    func startCapture() throws {
        log.info("🎙 startCapture() called, isRecording=\(self.isRecording)")
        guard !isRecording else {
            log.warning("⚠️ Already recording")
            return
        }

        // Configure audio session
        log.info("📍 Step 1: Setting up AVAudioSession...")
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
            log.info("📍 Step 1a: setCategory OK")
            try session.setPreferredSampleRate(16_000)
            log.info("📍 Step 1b: setPreferredSampleRate OK")
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            log.info("📍 Step 1c: setActive OK")
        } catch {
            log.error("❌ Audio session setup failed: \(error.localizedDescription)")
            throw AudioCaptureError.sessionSetupFailed(error)
        }

        // Stop any previous engine state to prevent "only one tap per bus" crash
        log.info("📍 Step 2: Checking engine state, isRunning=\(self.engine.isRunning)")
        if engine.isRunning {
            log.info("📍 Step 2a: Stopping previous engine")
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }

        log.info("📍 Step 3: Accessing inputNode...")
        let inputNode = engine.inputNode
        let inputFormat = inputNode.outputFormat(forBus: 0)
        log.info("📍 Step 3: Input format: sampleRate=\(inputFormat.sampleRate), channels=\(inputFormat.channelCount), commonFormat=\(inputFormat.commonFormat.rawValue)")

        // Guard against invalid format (0 channels / 0 sample rate)
        guard inputFormat.channelCount > 0, inputFormat.sampleRate > 0 else {
            log.error("❌ Invalid input format")
            throw AudioCaptureError.noInputAvailable
        }

        // Reset ring buffer
        log.info("📍 Step 4: Resetting ring buffer...")
        resetRingBuffer()

        // Install tap with native input format — conversion to 16kHz mono happens in processTapBuffer
        log.info("📍 Step 5: Installing tap...")
        inputNode.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { [weak self] buffer, _ in
            self?.processTapBuffer(buffer)
        }

        log.info("📍 Step 6: Preparing and starting engine...")
        do {
            engine.prepare()
            log.info("📍 Step 6a: engine.prepare() OK")
            try engine.start()
            log.info("📍 Step 6b: engine.start() OK ✅")
        } catch {
            log.error("❌ Engine start failed: \(error.localizedDescription)")
            inputNode.removeTap(onBus: 0)
            throw AudioCaptureError.engineStartFailed(error)
        }

        isRecording = true
        log.info("🎙 Recording started successfully ✅")
    }

    /// Stop recording and return all captured audio as WAV data.
    @MainActor
    @discardableResult
    func stopCapture() -> Data? {
        guard isRecording else { return nil }

        engine.inputNode.removeTap(onBus: 0)
        engine.stop()

        do {
            try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        } catch {
            // Non-fatal
        }

        isRecording = false

        guard let buffer = buildPCMBuffer() else { return nil }
        return try? Self.encodeToWAV(buffer: buffer)
    }

    // MARK: - WAV Encoding / Decoding

    /// Encode an AVAudioPCMBuffer (must be Int16 PCM) to WAV Data.
    nonisolated static func encodeToWAV(buffer: AVAudioPCMBuffer) throws -> Data {
        let format = buffer.format
        let sampleRate = UInt32(format.sampleRate)
        let channelCount = UInt16(format.channelCount)
        let bitsPerSample: UInt16 = 16
        let frameCount = Int(buffer.frameLength)

        let byteRate = sampleRate * UInt32(channelCount) * UInt32(bitsPerSample) / 8
        let blockAlign = channelCount * bitsPerSample / 8
        let dataSize = UInt32(frameCount) * UInt32(channelCount) * UInt32(bitsPerSample / 8)
        let chunkSize = 36 + dataSize

        var data = Data(capacity: 44 + Int(dataSize))

        // RIFF header
        data.append(contentsOf: "RIFF".utf8)
        data.appendLittleEndian(chunkSize)
        data.append(contentsOf: "WAVE".utf8)

        // fmt chunk
        data.append(contentsOf: "fmt ".utf8)
        data.appendLittleEndian(UInt32(16))    // fmt chunk size
        data.appendLittleEndian(UInt16(1))      // PCM audio format
        data.appendLittleEndian(channelCount)
        data.appendLittleEndian(sampleRate)
        data.appendLittleEndian(byteRate)
        data.appendLittleEndian(blockAlign)
        data.appendLittleEndian(bitsPerSample)

        // data chunk
        data.append(contentsOf: "data".utf8)
        data.appendLittleEndian(dataSize)

        // PCM samples
        if let int16Data = buffer.int16ChannelData {
            for frame in 0..<frameCount {
                for ch in 0..<Int(channelCount) {
                    data.appendLittleEndian(int16Data[ch][frame])
                }
            }
        } else if let floatData = buffer.floatChannelData {
            for frame in 0..<frameCount {
                for ch in 0..<Int(channelCount) {
                    let clamped = max(-1.0, min(1.0, floatData[ch][frame]))
                    let sample = Int16(clamped * Float(Int16.max))
                    data.appendLittleEndian(sample)
                }
            }
        }

        return data
    }

    /// Decode WAV data back to AVAudioPCMBuffer.
    nonisolated static func decodeWAV(data: Data) throws -> AVAudioPCMBuffer {
        guard data.count >= 44 else { throw WAVError.invalidData }

        // Validate RIFF/WAVE header
        guard String(bytes: data[0..<4], encoding: .ascii) == "RIFF",
              String(bytes: data[8..<12], encoding: .ascii) == "WAVE" else {
            throw WAVError.invalidData
        }

        // Parse fmt chunk (must be at offset 12)
        guard String(bytes: data[12..<16], encoding: .ascii) == "fmt " else {
            throw WAVError.invalidData
        }

        let audioFormat = data.readLittleEndian(UInt16.self, at: 20)
        let channelCount = data.readLittleEndian(UInt16.self, at: 22)
        let sampleRate = data.readLittleEndian(UInt32.self, at: 24)
        let bitsPerSample = data.readLittleEndian(UInt16.self, at: 34)

        guard audioFormat == 1, bitsPerSample == 16 else {
            throw WAVError.unsupportedFormat
        }

        // Search for "data" chunk (starts at offset 36 in standard 44-byte WAV)
        var dataChunkOffset = 0
        var dataChunkSize: UInt32 = 0
        var searchOffset = 36
        while searchOffset + 8 <= data.count {
            let chunkID = String(bytes: data[searchOffset..<(searchOffset + 4)], encoding: .ascii)
            let chunkSize = data.readLittleEndian(UInt32.self, at: searchOffset + 4)
            if chunkID == "data" {
                dataChunkOffset = searchOffset + 8
                dataChunkSize = chunkSize
                break
            }
            searchOffset += 8 + Int(chunkSize)
        }

        guard dataChunkSize > 0, dataChunkOffset > 0 else { throw WAVError.invalidData }

        let bytesPerSample = Int(bitsPerSample / 8)
        let frameCount = Int(dataChunkSize) / (Int(channelCount) * bytesPerSample)

        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: Double(sampleRate),
            channels: AVAudioChannelCount(channelCount),
            interleaved: false
        ) else {
            throw WAVError.unsupportedFormat
        }

        guard let outBuffer = AVAudioPCMBuffer(
            pcmFormat: format,
            frameCapacity: AVAudioFrameCount(frameCount)
        ) else {
            throw WAVError.bufferCreationFailed
        }
        outBuffer.frameLength = AVAudioFrameCount(frameCount)

        guard let int16Data = outBuffer.int16ChannelData else {
            throw WAVError.bufferCreationFailed
        }

        // Read samples from data chunk
        var byteOffset = dataChunkOffset
        for frame in 0..<frameCount {
            for ch in 0..<Int(channelCount) {
                guard byteOffset + 2 <= data.count else { break }
                let sample = data.readLittleEndian(Int16.self, at: byteOffset)
                int16Data[ch][frame] = sample
                byteOffset += 2
            }
        }

        return outBuffer
    }

    // MARK: - Private Helpers

    /// Reset ring buffer state. Called synchronously (not in async context) to avoid NSLock warnings.
    private nonisolated func resetRingBuffer() {
        lock.lock()
        ringBuffer = [Int16](repeating: 0, count: Self.maxSamples)
        ringWriteIndex = 0
        totalSamplesWritten = 0
        lock.unlock()
    }

    private nonisolated(unsafe) static var tapCallCount = 0

    private func processTapBuffer(_ buffer: AVAudioPCMBuffer) {
        Self.tapCallCount += 1
        let callNum = Self.tapCallCount

        // Log first few calls to verify tap is working
        if callNum <= 3 {
            log.info("🔊 processTapBuffer #\(callNum): frames=\(buffer.frameLength), rate=\(buffer.format.sampleRate), channels=\(buffer.format.channelCount)")
        }

        // Compute audio level directly from the input buffer (float32 format)
        // This avoids needing the converter for visualization
        if let floatData = buffer.floatChannelData {
            let frameLength = Int(buffer.frameLength)
            var sumOfSquares: Float = 0
            for i in 0..<frameLength {
                let sample = floatData[0][i]
                sumOfSquares += sample * sample
            }
            let rms = sqrt(sumOfSquares / max(1, Float(frameLength)))
            let normalizedLevel = min(1.0, rms * 5.0)
            let callback = self.onAudioLevelUpdate
            DispatchQueue.main.async {
                callback?(normalizedLevel)
            }
        }

        // Convert to 16kHz Int16 for the ring buffer (for WAV export)
        guard let converter = AVAudioConverter(from: buffer.format, to: captureFormat) else {
            if callNum <= 3 { log.error("❌ Failed to create converter") }
            return
        }

        let ratio = captureFormat.sampleRate / buffer.format.sampleRate
        let outputFrameCount = AVAudioFrameCount(ceil(Double(buffer.frameLength) * ratio))

        guard let outputBuffer = AVAudioPCMBuffer(pcmFormat: captureFormat,
                                                   frameCapacity: outputFrameCount) else {
            if callNum <= 3 { log.error("❌ Failed to create output buffer") }
            return
        }

        var hasProvidedData = false
        var conversionError: NSError?
        let status = converter.convert(to: outputBuffer, error: &conversionError) { _, outStatus in
            if hasProvidedData {
                outStatus.pointee = .endOfStream
                return nil
            }
            hasProvidedData = true
            outStatus.pointee = .haveData
            return buffer
        }

        if callNum <= 3 {
            log.info("🔊 Conversion #\(callNum): status=\(status.rawValue), outFrames=\(outputBuffer.frameLength), error=\(conversionError?.localizedDescription ?? "none")")
        }

        guard status != .error, let int16Ptr = outputBuffer.int16ChannelData else { return }
        let frameLength = Int(outputBuffer.frameLength)

        lock.lock()
        for i in 0..<frameLength {
            ringBuffer[ringWriteIndex] = int16Ptr[0][i]
            ringWriteIndex = (ringWriteIndex + 1) % Self.maxSamples
            totalSamplesWritten += 1
        }
        lock.unlock()
    }

    /// Build a PCM buffer from the ring buffer contents.
    private func buildPCMBuffer() -> AVAudioPCMBuffer? {
        lock.lock()
        let sampleCount = min(totalSamplesWritten, Self.maxSamples)
        let capturedRingBuffer = ringBuffer
        let capturedWriteIndex = ringWriteIndex
        let capturedTotal = totalSamplesWritten
        lock.unlock()

        guard sampleCount > 0 else { return nil }

        guard let buffer = AVAudioPCMBuffer(pcmFormat: captureFormat,
                                            frameCapacity: AVAudioFrameCount(sampleCount)) else { return nil }
        buffer.frameLength = AVAudioFrameCount(sampleCount)

        guard let int16Ptr = buffer.int16ChannelData else { return nil }

        if capturedTotal <= Self.maxSamples {
            for i in 0..<sampleCount {
                int16Ptr[0][i] = capturedRingBuffer[i]
            }
        } else {
            for i in 0..<sampleCount {
                let srcIndex = (capturedWriteIndex + i) % Self.maxSamples
                int16Ptr[0][i] = capturedRingBuffer[srcIndex]
            }
        }

        return buffer
    }

    // MARK: - Interruption Handling

    @objc private func handleInterruption(_ notification: Notification) {
        guard let info = notification.userInfo,
              let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: typeValue),
              type == .began else { return }

        Task { @MainActor in
            if self.isRecording {
                _ = self.stopCapture()
            }
        }
    }
}

// MARK: - Data Extensions

private extension Data {
    mutating func appendLittleEndian(_ value: UInt16) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { self.append(contentsOf: $0) }
    }

    mutating func appendLittleEndian(_ value: Int16) {
        appendLittleEndian(UInt16(bitPattern: value))
    }

    mutating func appendLittleEndian(_ value: UInt32) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { self.append(contentsOf: $0) }
    }

    func readLittleEndian<T: FixedWidthInteger>(_ type: T.Type, at offset: Int) -> T {
        guard offset + MemoryLayout<T>.size <= count else { return 0 }
        return subdata(in: offset..<(offset + MemoryLayout<T>.size)).withUnsafeBytes {
            $0.loadUnaligned(as: T.self).littleEndian
        }
    }
}

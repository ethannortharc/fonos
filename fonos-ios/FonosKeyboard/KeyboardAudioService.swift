@preconcurrency import Foundation
@preconcurrency import AVFoundation
import os.log

private let kbAudioLog = Logger(subsystem: "com.fonos.ios.keyboard", category: "KeyboardAudio")

// MARK: - KeyboardAudioService

/// Lightweight audio capture for the keyboard extension.
/// - 16kHz 16-bit mono PCM (same as main app)
/// - 30 second ring buffer max (keyboard extensions have ~50MB memory limit)
/// - No ObservableObject — plain class to minimize overhead
final class KeyboardAudioService: @unchecked Sendable {

    // MARK: - Constants

    static let sampleRate: Double = 16_000
    static let maxSamples = Int(sampleRate) * 30   // 30 seconds

    // MARK: - Public State

    private(set) var isRecording: Bool = false

    // MARK: - Private

    private let engine = AVAudioEngine()
    private let lock = NSLock()

    private lazy var captureFormat: AVAudioFormat = {
        guard let fmt = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: Self.sampleRate,
            channels: 1,
            interleaved: false
        ) else {
            fatalError("KeyboardAudioService: cannot create capture format")
        }
        return fmt
    }()

    private var ringBuffer: [Int16] = []
    private var ringWriteIndex = 0
    private var totalSamplesWritten = 0

    // MARK: - Init

    init() {}

    // MARK: - Public API

    /// Start recording. Completion called on background thread.
    func startCapture(completion: @escaping @Sendable (Error?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.startCaptureSync(completion: completion)
        }
    }

    /// Stop recording and return WAV data (if any audio was captured).
    func stopCapture() -> Data? {
        guard isRecording else { return nil }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()

        do {
            try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        } catch {
            kbAudioLog.warning("Audio session deactivation: \(error.localizedDescription)")
        }

        isRecording = false

        guard let buffer = buildPCMBuffer() else { return nil }
        return try? encodeToWAV(buffer: buffer)
    }

    // MARK: - Private: Engine Setup

    private func startCaptureSync(completion: @escaping @Sendable (Error?) -> Void) {
        guard !isRecording else { completion(nil); return }

        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.record, mode: .default, options: [])
            try session.setPreferredSampleRate(Self.sampleRate)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
        } catch {
            completion(error)
            return
        }

        if engine.isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }

        let inputNode = engine.inputNode
        let inputFormat = inputNode.outputFormat(forBus: 0)

        guard inputFormat.channelCount > 0, inputFormat.sampleRate > 0 else {
            completion(NSError(domain: "KeyboardAudio", code: 1,
                               userInfo: [NSLocalizedDescriptionKey: "No audio input available"]))
            return
        }

        resetRingBuffer()
        engine.prepare()

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: nil) { [weak self] buffer, _ in
            self?.processTap(buffer)
        }

        do {
            try engine.start()
        } catch {
            inputNode.removeTap(onBus: 0)
            completion(error)
            return
        }

        isRecording = true
        kbAudioLog.info("KeyboardAudioService: recording started")
        completion(nil)
    }

    // MARK: - Private: Ring Buffer

    private func resetRingBuffer() {
        lock.lock()
        ringBuffer = [Int16](repeating: 0, count: Self.maxSamples)
        ringWriteIndex = 0
        totalSamplesWritten = 0
        lock.unlock()
    }

    private func processTap(_ buffer: AVAudioPCMBuffer) {
        guard let converter = AVAudioConverter(from: buffer.format, to: captureFormat) else {
            return
        }

        let ratio = captureFormat.sampleRate / buffer.format.sampleRate
        let outFrameCount = AVAudioFrameCount(ceil(Double(buffer.frameLength) * ratio))
        guard let outBuffer = AVAudioPCMBuffer(pcmFormat: captureFormat, frameCapacity: outFrameCount) else {
            return
        }

        final class Box: @unchecked Sendable { var value: Bool = false }
        let provided = Box()
        var convErr: NSError?
        let status = converter.convert(to: outBuffer, error: &convErr) { _, outStatus in
            if provided.value {
                outStatus.pointee = .endOfStream
                return nil
            }
            provided.value = true
            outStatus.pointee = .haveData
            return buffer
        }

        guard status != .error, let int16Ptr = outBuffer.int16ChannelData else { return }
        let frameLen = Int(outBuffer.frameLength)

        lock.lock()
        for i in 0..<frameLen {
            ringBuffer[ringWriteIndex] = int16Ptr[0][i]
            ringWriteIndex = (ringWriteIndex + 1) % Self.maxSamples
            totalSamplesWritten += 1
        }
        lock.unlock()
    }

    private func buildPCMBuffer() -> AVAudioPCMBuffer? {
        lock.lock()
        let sampleCount = min(totalSamplesWritten, Self.maxSamples)
        let capturedRing = ringBuffer
        let capturedWrite = ringWriteIndex
        let capturedTotal = totalSamplesWritten
        lock.unlock()

        guard sampleCount > 0 else { return nil }

        guard let buffer = AVAudioPCMBuffer(pcmFormat: captureFormat,
                                            frameCapacity: AVAudioFrameCount(sampleCount)) else { return nil }
        buffer.frameLength = AVAudioFrameCount(sampleCount)
        guard let int16Ptr = buffer.int16ChannelData else { return nil }

        if capturedTotal <= Self.maxSamples {
            for i in 0..<sampleCount {
                int16Ptr[0][i] = capturedRing[i]
            }
        } else {
            for i in 0..<sampleCount {
                let srcIdx = (capturedWrite + i) % Self.maxSamples
                int16Ptr[0][i] = capturedRing[srcIdx]
            }
        }

        return buffer
    }

    // MARK: - WAV Encoding

    private func encodeToWAV(buffer: AVAudioPCMBuffer) throws -> Data {
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

        data.append(contentsOf: "RIFF".utf8)
        data.appendLittleEndian32(chunkSize)
        data.append(contentsOf: "WAVE".utf8)

        data.append(contentsOf: "fmt ".utf8)
        data.appendLittleEndian32(UInt32(16))
        data.appendLittleEndian16(UInt16(1))
        data.appendLittleEndian16(channelCount)
        data.appendLittleEndian32(sampleRate)
        data.appendLittleEndian32(byteRate)
        data.appendLittleEndian16(blockAlign)
        data.appendLittleEndian16(bitsPerSample)

        data.append(contentsOf: "data".utf8)
        data.appendLittleEndian32(dataSize)

        if let int16Data = buffer.int16ChannelData {
            for frame in 0..<frameCount {
                for ch in 0..<Int(channelCount) {
                    data.appendLittleEndian16(UInt16(bitPattern: int16Data[ch][frame]))
                }
            }
        }

        return data
    }
}

// MARK: - Data helpers

private extension Data {
    mutating func appendLittleEndian16(_ value: UInt16) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { self.append(contentsOf: $0) }
    }
    mutating func appendLittleEndian32(_ value: UInt32) {
        var v = value.littleEndian
        Swift.withUnsafeBytes(of: &v) { self.append(contentsOf: $0) }
    }
}

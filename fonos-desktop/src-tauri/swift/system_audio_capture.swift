// fonos-audio-capture: Capture system audio output via ScreenCaptureKit.
//
// Usage:
//   fonos-audio-capture check   — print availability JSON and exit
//   fonos-audio-capture start   — stream raw 16kHz mono Int16 PCM to stdout
//
// check output (stdout): {"available": true/false, "reason": "..."}
//
// start output: raw little-endian Int16 PCM samples at 16000 Hz, mono,
//               written continuously to stdout until the process is killed.
//
// Build:
//   swiftc -O -o ../resources/fonos-audio-capture system_audio_capture.swift \
//       -framework ScreenCaptureKit -framework AVFoundation \
//       -framework CoreMedia -framework Foundation
//
// Requires macOS 13.0+. On older macOS the check command returns available:false.

import Foundation
import AVFoundation
import CoreMedia

// ScreenCaptureKit is macOS 12.3+ but audio capture requires 13.0+.
// We import it conditionally and gate behind a runtime version check.
#if canImport(ScreenCaptureKit)
import ScreenCaptureKit
#endif

// ── Helpers ───────────────────────────────────────────────────────────────────

func writeJSON(_ dict: [String: Any]) {
    if let data = try? JSONSerialization.data(withJSONObject: dict, options: []) {
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write("\n".data(using: .utf8)!)
    }
}

func exitWithJSON(_ dict: [String: Any], code: Int32 = 0) -> Never {
    writeJSON(dict)
    exit(code)
}

// ── Runtime availability check ────────────────────────────────────────────────

func isScreenCaptureKitAudioAvailable() -> (Bool, String) {
    guard #available(macOS 13.0, *) else {
        return (false, "Requires macOS 13.0 or later")
    }
    #if canImport(ScreenCaptureKit)
    // Check if the SCShareableContent class exists at runtime (belt-and-suspenders
    // for any stripped builds). On macOS 13+ this is always true.
    guard NSClassFromString("SCShareableContent") != nil else {
        return (false, "ScreenCaptureKit not available at runtime")
    }
    return (true, "ScreenCaptureKit available")
    #else
    return (false, "ScreenCaptureKit not compiled in")
    #endif
}

// ── Command dispatch ──────────────────────────────────────────────────────────

let args = CommandLine.arguments
guard args.count >= 2 else {
    exitWithJSON(["error": "Usage: fonos-audio-capture <check|start>"], code: 1)
}

let command = args[1]

switch command {
case "check":
    let (available, reason) = isScreenCaptureKitAudioAvailable()
    exitWithJSON(["available": available, "reason": reason])

case "start":
    guard #available(macOS 13.0, *) else {
        writeJSON(["error": "Requires macOS 13.0+"])
        exit(1)
    }
    #if canImport(ScreenCaptureKit)
    startCapture()
    #else
    writeJSON(["error": "ScreenCaptureKit not available"])
    exit(1)
    #endif

default:
    exitWithJSON(["error": "Unknown command: \(command)"], code: 1)
}

// ── SCK audio capture (macOS 13+) ─────────────────────────────────────────────

#if canImport(ScreenCaptureKit)

// Keep strong references at module scope so ARC does not deallocate them when
// the SCShareableContent completion handler returns.  SCStream does NOT self-
// retain while capturing — the caller must hold the reference.
@available(macOS 13.0, *)
var activeStream: SCStream?
var activeDelegate: AudioCaptureDelegate?

@available(macOS 13.0, *)
func startCapture() {
    let delegate = AudioCaptureDelegate()
    activeDelegate = delegate              // prevent ARC deallocation
    let semaphore = DispatchSemaphore(value: 0)

    // Request shareable content to trigger the permission prompt and verify access.
    SCShareableContent.getWithCompletionHandler { content, error in
        if let error = error {
            fputs("fonos-audio-capture: SCShareableContent error: \(error.localizedDescription)\n",
                  stderr)
            exit(1)
        }

        guard let content = content else {
            fputs("fonos-audio-capture: no shareable content returned\n", stderr)
            exit(1)
        }

        // We only need audio — use the first display as a dummy filter target.
        // SCStreamConfiguration with capturesAudio=true and excludesCurrentProcessAudio=false
        // gives us the full system audio mix.
        let filter: SCContentFilter
        if let display = content.displays.first {
            filter = SCContentFilter(display: display, excludingWindows: [])
        } else {
            fputs("fonos-audio-capture: no displays found\n", stderr)
            exit(1)
        }

        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = false

        // Request 16kHz mono to avoid resampling where possible. In practice SCK
        // may give us a different rate; we resample in the delegate.
        config.sampleRate = 16000
        config.channelCount = 1

        // Minimise video overhead — we don't want video frames at all.
        // Setting a very small size and low frame rate reduces CPU usage.
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1) // 1 fps

        let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
        activeStream = stream              // prevent ARC deallocation

        do {
            try stream.addStreamOutput(delegate,
                                       type: .audio,
                                       sampleHandlerQueue: DispatchQueue(label: "fonos.audio"))
            stream.startCapture { error in
                if let error = error {
                    fputs("fonos-audio-capture: startCapture error: \(error.localizedDescription)\n",
                          stderr)
                    exit(1)
                }
                fputs("fonos-audio-capture: capture started (16kHz mono i16 PCM on stdout)\n",
                      stderr)
            }
        } catch {
            fputs("fonos-audio-capture: addStreamOutput error: \(error.localizedDescription)\n",
                  stderr)
            exit(1)
        }

        semaphore.signal()
    }

    semaphore.wait()
    // Keep the process alive — SCK delivers callbacks on a background queue.
    RunLoop.main.run()
}

// ── Stream output delegate ────────────────────────────────────────────────────

@available(macOS 13.0, *)
class AudioCaptureDelegate: NSObject, SCStreamDelegate, SCStreamOutput {

    // Resampler state for converting from device rate to 16kHz.
    private var resamplePos: Double = 0.0
    private var prevSample: Float = 0.0

    // stdout handle — pre-opened once for performance.
    private let stdout = FileHandle.standardOutput

    private var bufferCount = 0

    func stream(_ stream: SCStream,
                didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
                of outputType: SCStreamOutputType) {
        guard outputType == .audio else { return }
        guard sampleBuffer.isValid else { return }

        bufferCount += 1
        if bufferCount == 1 {
            fputs("fonos-audio-capture: first audio buffer received\n", stderr)
        }
        if bufferCount % 100 == 0 {
            fputs("fonos-audio-capture: \(bufferCount) audio buffers processed\n", stderr)
        }

        processSampleBuffer(sampleBuffer)
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        fputs("fonos-audio-capture: stream stopped: \(error.localizedDescription)\n", stderr)
        exit(1)
    }

    private func processSampleBuffer(_ sampleBuffer: CMSampleBuffer) {
        // Extract the audio buffer list from the CMSampleBuffer.
        guard let blockBuffer = sampleBuffer.dataBuffer else { return }

        // Get format description to know sample rate and channel count.
        guard let formatDesc = sampleBuffer.formatDescription else { return }
        let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc)
        guard let asbd = asbd else { return }

        let deviceRate = asbd.pointee.mSampleRate
        let deviceChannels = Int(asbd.pointee.mChannelsPerFrame)
        let targetRate: Double = 16000.0

        guard deviceChannels > 0 else { return }

        // Get the raw bytes from the block buffer.
        var dataLength = 0
        var dataPtr: UnsafeMutablePointer<CChar>? = nil
        let status = CMBlockBufferGetDataPointer(blockBuffer,
                                                 atOffset: 0,
                                                 lengthAtOffsetOut: nil,
                                                 totalLengthOut: &dataLength,
                                                 dataPointerOut: &dataPtr)
        guard status == kCMBlockBufferNoErr, let ptr = dataPtr, dataLength > 0 else { return }

        // Determine sample format from ASBD flags and bit depth.
        let formatFlags = asbd.pointee.mFormatFlags
        let bitsPerChannel = asbd.pointee.mBitsPerChannel

        // Convert to f32 mono, then resample to 16kHz, then to i16.
        let monoF32 = extractMonoF32(ptr: ptr,
                                      byteCount: dataLength,
                                      channels: deviceChannels,
                                      formatFlags: formatFlags,
                                      bitsPerChannel: bitsPerChannel)

        guard !monoF32.isEmpty else { return }

        let resampled = resampleTo16k(frames: monoF32,
                                       fromRate: deviceRate,
                                       toRate: targetRate)

        guard !resampled.isEmpty else { return }

        // Convert f32 to i16 and write to stdout.
        var i16Samples = [Int16](repeating: 0, count: resampled.count)
        for (i, s) in resampled.enumerated() {
            let clamped = max(-1.0, min(1.0, s))
            i16Samples[i] = Int16(clamped * Float(Int16.max))
        }

        i16Samples.withUnsafeBytes { rawBuf in
            let data = Data(rawBuf)
            stdout.write(data)
        }
    }

    /// Convert raw PCM bytes to mono f32 frames.
    private func extractMonoF32(ptr: UnsafeMutablePointer<CChar>,
                                  byteCount: Int,
                                  channels: Int,
                                  formatFlags: AudioFormatFlags,
                                  bitsPerChannel: UInt32) -> [Float] {
        // Interleaved PCM. Determine format from flags and bit depth.
        // kAudioFormatFlagIsFloat = 0x01, kAudioFormatFlagIsSignedInteger = 0x04
        let isFloat = (formatFlags & kAudioFormatFlagIsFloat) != 0
        let isSignedInt = (formatFlags & kAudioFormatFlagIsSignedInteger) != 0
        let isBigEndian = (formatFlags & kAudioFormatFlagIsBigEndian) != 0

        // Determine bytes per sample.
        let bytesPerSample = Int(bitsPerChannel) / 8
        guard bytesPerSample > 0 else { return [] }

        let totalSamples = byteCount / bytesPerSample
        let frameCount = totalSamples / channels

        guard frameCount > 0 else { return [] }

        var monoFrames = [Float](repeating: 0, count: frameCount)

        let rawBytes = UnsafeRawPointer(ptr)

        for frame in 0..<frameCount {
            var monoVal: Float = 0.0
            for ch in 0..<channels {
                let offset = (frame * channels + ch) * bytesPerSample
                let sampleF32: Float

                if isFloat && bytesPerSample == 4 {
                    // 32-bit float — most common SCK output
                    var val: Float32 = 0
                    withUnsafeMutableBytes(of: &val) { dest in
                        dest.copyMemory(from: UnsafeRawBufferPointer(
                            start: rawBytes.advanced(by: offset),
                            count: 4))
                    }
                    if isBigEndian {
                        let bits = val.bitPattern.byteSwapped
                        val = Float(bitPattern: bits)
                    }
                    sampleF32 = val
                } else if isFloat && bytesPerSample == 8 {
                    // 64-bit float
                    var val: Float64 = 0
                    withUnsafeMutableBytes(of: &val) { dest in
                        dest.copyMemory(from: UnsafeRawBufferPointer(
                            start: rawBytes.advanced(by: offset),
                            count: 8))
                    }
                    sampleF32 = Float(val)
                } else if bytesPerSample == 2 {
                    // 16-bit integer
                    var val: Int16 = 0
                    withUnsafeMutableBytes(of: &val) { dest in
                        dest.copyMemory(from: UnsafeRawBufferPointer(
                            start: rawBytes.advanced(by: offset),
                            count: 2))
                    }
                    if isBigEndian { val = Int16(bitPattern: UInt16(bitPattern: val).byteSwapped) }
                    sampleF32 = isSignedInt
                        ? Float(val) / Float(Int16.max)
                        : Float(val) / Float(UInt16.max) * 2.0 - 1.0
                } else if bytesPerSample == 4 {
                    // 32-bit integer
                    var val: Int32 = 0
                    withUnsafeMutableBytes(of: &val) { dest in
                        dest.copyMemory(from: UnsafeRawBufferPointer(
                            start: rawBytes.advanced(by: offset),
                            count: 4))
                    }
                    if isBigEndian { val = Int32(bitPattern: UInt32(bitPattern: val).byteSwapped) }
                    sampleF32 = isSignedInt
                        ? Float(val) / Float(Int32.max)
                        : Float(val) / Float(UInt32.max) * 2.0 - 1.0
                } else {
                    sampleF32 = 0.0
                }

                monoVal += sampleF32
            }
            monoFrames[frame] = monoVal / Float(channels)
        }

        return monoFrames
    }

    /// Linear interpolation resampler: convert `frames` from `fromRate` to `toRate`.
    private func resampleTo16k(frames: [Float],
                                fromRate: Double,
                                toRate: Double) -> [Float] {
        guard !frames.isEmpty else { return [] }
        if abs(fromRate - toRate) < 1.0 {
            // Rates are identical — pass through.
            return frames
        }

        let ratio = fromRate / toRate
        var output = [Float]()
        output.reserveCapacity(Int(Double(frames.count) / ratio) + 1)

        while resamplePos < Double(frames.count) {
            let idx = Int(resamplePos)
            let frac = Float(resamplePos - Double(idx))

            let s0 = idx < frames.count ? frames[idx] : prevSample
            let s1 = idx + 1 < frames.count ? frames[idx + 1] : s0

            output.append(s0 + (s1 - s0) * frac)
            resamplePos += ratio
        }

        // Carry fractional remainder and last sample into the next callback.
        resamplePos -= Double(frames.count)
        if let last = frames.last { prevSample = last }

        return output
    }
}
#endif

// fonos-voice-capture: microphone capture with Apple Voice Processing I/O (VPIO).
//
// Enabling voice processing on an AVAudioEngine input node engages Apple's
// hardware/system echo canceller: it subtracts the device's own output (e.g.
// the assistant's TTS playing through the speakers) from the mic signal, and
// additionally applies noise suppression and automatic gain control. This is
// the platform-correct way to keep the assistant's own voice from bleeding into
// the mic and self-triggering barge-in during call mode.
//
// Usage:
//   fonos-voice-capture            — capture from the default input, stream PCM
//   fonos-voice-capture <device>   — v1: the device name is logged; the system
//                                    default input is still used (VPIO is built
//                                    around the default input/output pair).
//
// Output (stdout): raw little-endian Int16 PCM, 16 kHz, mono, written
//   continuously until the process is killed or stdout is closed.
//
// Build:
//   swiftc -O -o ../resources/fonos-voice-capture voice_capture.swift \
//       -framework AVFoundation -framework Foundation
//
// Requires macOS 10.15+ (setVoiceProcessingEnabled). The other-audio ducking
// override requires macOS 14+ and is applied only when available.

import Foundation
import AVFoundation

// stdout handle, pre-opened once. FileHandle writes go straight to the fd
// (unbuffered), so each buffer is flushed to the reader as it is written.
let stdoutHandle = FileHandle.standardOutput

func logErr(_ s: String) {
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
}

// ── Optional device-name arg ───────────────────────────────────────────────
// v1 uses the system default input: selecting a specific device on a VPIO audio
// unit (kAudioOutputUnitProperty_CurrentDevice) is fiddly and can break the
// echo canceller, which is designed around the default input/output pair. We
// accept and log the requested name for forward-compat.
let requestedDevice: String? = CommandLine.arguments.count >= 2 ? CommandLine.arguments[1] : nil
if let d = requestedDevice, !d.isEmpty, d != "auto", d != "default" {
    logErr("fonos-voice-capture: requested device '\(d)' — v1 uses the system default input (VPIO)")
}

// ── Engine + VPIO ──────────────────────────────────────────────────────────

let engine = AVAudioEngine()
let input = engine.inputNode

// Engage Apple Voice Processing I/O BEFORE touching any formats. This turns on
// echo cancellation of the device output + noise suppression + AGC on the mic.
do {
    try input.setVoiceProcessingEnabled(true)
} catch {
    logErr("fonos-voice-capture: setVoiceProcessingEnabled failed: \(error.localizedDescription)")
    exit(1)
}

// Don't let VPIO duck other audio (macOS 14+): the assistant's TTS keeps its
// full volume while we cancel it from the mic, instead of being volume-ducked.
if #available(macOS 14.0, *) {
    input.voiceProcessingOtherAudioDuckingConfiguration =
        AVAudioVoiceProcessingOtherAudioDuckingConfiguration(
            enableAdvancedDucking: false, duckingLevel: .min)
    logErr("fonos-voice-capture: advanced ducking disabled (duckingLevel=min)")
}

// Target: 16 kHz mono interleaved Int16 — the format every other fonos capture
// path produces, so the Rust consumer is byte-compatible with the mic path.
guard let outFormat = AVAudioFormat(
    commonFormat: .pcmFormatInt16,
    sampleRate: 16_000,
    channels: 1,
    interleaved: true
) else {
    logErr("fonos-voice-capture: could not build 16kHz mono Int16 output format")
    exit(1)
}

// AVAudioConverter is stateful across calls (it carries resampler state), so we
// build it once, lazily, from the real post-VPIO input format and reuse it.
var converter: AVAudioConverter?

input.installTap(onBus: 0, bufferSize: 1024, format: nil) { buffer, _ in
    if converter == nil {
        converter = AVAudioConverter(from: buffer.format, to: outFormat)
        logErr("fonos-voice-capture: input \(Int(buffer.format.sampleRate))Hz " +
               "\(buffer.format.channelCount)ch → 16000Hz mono i16")
    }
    guard let conv = converter else { return }

    let ratio = outFormat.sampleRate / buffer.format.sampleRate
    let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
    guard capacity > 0,
          let outBuffer = AVAudioPCMBuffer(pcmFormat: outFormat, frameCapacity: capacity)
    else { return }

    // Feed exactly one input buffer per conversion call; the converter keeps its
    // own resampler state between calls.
    var supplied = false
    var convError: NSError?
    let status = conv.convert(to: outBuffer, error: &convError) { _, inputStatus in
        if supplied {
            inputStatus.pointee = .noDataNow
            return nil
        }
        supplied = true
        inputStatus.pointee = .haveData
        return buffer
    }
    if status == .error {
        if let e = convError {
            logErr("fonos-voice-capture: convert error: \(e.localizedDescription)")
        }
        return
    }

    let frames = Int(outBuffer.frameLength)
    guard frames > 0, let ch = outBuffer.int16ChannelData else { return }
    // Mono interleaved: ch[0] is `frames` contiguous Int16 samples.
    let data = Data(bytes: ch[0], count: frames * MemoryLayout<Int16>.size)

    do {
        try stdoutHandle.write(contentsOf: data)
    } catch {
        // Parent closed the read end — nothing left to feed. Exit cleanly.
        logErr("fonos-voice-capture: stdout closed, exiting")
        exit(0)
    }
}

engine.prepare()
do {
    try engine.start()
} catch {
    logErr("fonos-voice-capture: engine.start failed: \(error.localizedDescription)")
    exit(1)
}
logErr("fonos-voice-capture: capture started (VPIO AEC, 16kHz mono i16 PCM on stdout)")

// ── Clean shutdown on SIGTERM/SIGINT ───────────────────────────────────────
// Use DispatchSource (its handler may capture, unlike a raw C signal handler).
signal(SIGTERM, SIG_IGN)
signal(SIGINT, SIG_IGN)
let stop: () -> Void = {
    engine.stop()
    input.removeTap(onBus: 0)
    exit(0)
}
let sigTerm = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
sigTerm.setEventHandler(handler: stop)
sigTerm.resume()
let sigInt = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
sigInt.setEventHandler(handler: stop)
sigInt.resume()

RunLoop.main.run()

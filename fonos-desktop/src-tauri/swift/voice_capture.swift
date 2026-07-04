// fonos-voice-capture: full-duplex voice-processed audio for call mode —
// microphone capture AND assistant playback on one AVAudioEngine with Apple
// Voice Processing I/O (VPIO) enabled.
//
// Why full duplex: VPIO's echo canceller uses the *same engine's output* as
// its cancellation reference. If the assistant's TTS plays through another
// process (rodio in the Rust shell), the reference is silence and nothing is
// cancelled — system-wide, VPIO only *ducks* other audio, which we explicitly
// disable to keep TTS volume. Playing the TTS through this engine's output
// node gives the canceller the true reference, so the assistant's own voice is
// genuinely subtracted from the mic and can't self-trigger barge-in.
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
// Input (stdin), little-endian frames of [u32 len][len bytes of 16 kHz mono
//   i16 PCM] scheduled for playback through the engine. A frame with len == 0
//   is a control frame meaning FLUSH: stop the player immediately and discard
//   everything scheduled (barge cut / hangup). EOF on stdin leaves playback
//   idle; capture continues until SIGTERM.
//
// Control lines (stderr, line-buffered):
//   READY  — engine running (capture live, playback armed)
//   DRAIN  — the playback queue emptied (everything scheduled has played)
//   anything else is a human-readable log line.
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

// stderr writes come from several threads (tap callback, stdin reader,
// playback completions) — serialize them so control lines never interleave.
let stderrLock = NSLock()
func logErr(_ s: String) {
    stderrLock.lock()
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
    stderrLock.unlock()
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
let output = engine.outputNode

// Engage Apple Voice Processing I/O BEFORE wiring any nodes or touching
// formats. Apple requires the pair: voice processing must be enabled on BOTH
// the input and the output node of the same engine. This turns on echo
// cancellation of THIS engine's output + noise suppression + AGC on the mic.
do {
    try input.setVoiceProcessingEnabled(true)
} catch {
    logErr("fonos-voice-capture: input setVoiceProcessingEnabled failed: \(error.localizedDescription)")
    exit(1)
}
do {
    try output.setVoiceProcessingEnabled(true)
} catch {
    logErr("fonos-voice-capture: output setVoiceProcessingEnabled failed: \(error.localizedDescription)")
    exit(1)
}

// Don't let VPIO duck other audio (macOS 14+): system sounds keep their volume
// while our own playback is cancelled from the mic via the engine reference.
if #available(macOS 14.0, *) {
    input.voiceProcessingOtherAudioDuckingConfiguration =
        AVAudioVoiceProcessingOtherAudioDuckingConfiguration(
            enableAdvancedDucking: false, duckingLevel: .min)
    logErr("fonos-voice-capture: advanced ducking disabled (duckingLevel=min)")
}

// ── Playback graph: player → mainMixer → output ────────────────────────────
// The player is fed 16 kHz mono float buffers (converted from the i16 stdin
// frames); the engine resamples to whatever the output hardware runs at.
// Accessing mainMixerNode implicitly wires it to the output node.
guard let playFormat = AVAudioFormat(standardFormatWithSampleRate: 16_000, channels: 1) else {
    logErr("fonos-voice-capture: could not build 16kHz mono float playback format")
    exit(1)
}
let player = AVAudioPlayerNode()
engine.attach(player)
engine.connect(player, to: engine.mainMixerNode, format: playFormat)

// Playback bookkeeping lives on one serial queue: buffer completions and
// stdin-frame scheduling both hop onto it. `generation` invalidates the
// completion handlers of buffers discarded by a FLUSH, so a flush never emits
// a spurious DRAIN.
let playQueue = DispatchQueue(label: "fonos.voice.playback")
var pendingBuffers = 0
var generation: UInt64 = 0

/// Split a payload of 16 kHz mono i16 PCM into ≤0.5 s float buffers and
/// schedule them; emits DRAIN when the last scheduled buffer finishes.
func schedulePayload(_ payload: Data) {
    let totalSamples = payload.count / 2
    guard totalSamples > 0 else { return }
    let chunkSamples = 8_000 // 0.5 s @ 16 kHz
    var buffers: [AVAudioPCMBuffer] = []
    payload.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
        var offset = 0
        while offset < totalSamples {
            let n = min(chunkSamples, totalSamples - offset)
            guard let buf = AVAudioPCMBuffer(
                pcmFormat: playFormat, frameCapacity: AVAudioFrameCount(n)
            ), let dst = buf.floatChannelData?[0] else { break }
            buf.frameLength = AVAudioFrameCount(n)
            for i in 0..<n {
                let lo = UInt16(raw[(offset + i) * 2])
                let hi = UInt16(raw[(offset + i) * 2 + 1])
                let s = Int16(bitPattern: lo | (hi << 8))
                dst[i] = Float(s) / 32768.0
            }
            buffers.append(buf)
            offset += n
        }
    }
    playQueue.sync {
        let gen = generation
        for buf in buffers {
            pendingBuffers += 1
            player.scheduleBuffer(buf) {
                playQueue.async {
                    guard gen == generation else { return } // flushed — stale
                    pendingBuffers -= 1
                    if pendingBuffers <= 0 {
                        pendingBuffers = 0
                        logErr("DRAIN")
                    }
                }
            }
        }
        if !player.isPlaying {
            player.play()
        }
    }
}

/// FLUSH control frame: stop the player now, discard everything scheduled,
/// and reset. Discarded buffers' completions are invalidated by the generation
/// bump, so no DRAIN is emitted for audio that never played.
func flushPlayback() {
    playQueue.sync {
        generation += 1
        pendingBuffers = 0
        player.stop()
    }
}

// ── Capture: mic tap → 16 kHz mono i16 → stdout ────────────────────────────

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
logErr("fonos-voice-capture: full-duplex started (VPIO AEC; mic → stdout, stdin frames → speaker)")
logErr("READY")

// ── stdin reader: playback frames ──────────────────────────────────────────

/// Read exactly `count` bytes from `handle`; nil on EOF/error before `count`.
func readExact(_ handle: FileHandle, _ count: Int) -> Data? {
    var data = Data(capacity: count)
    while data.count < count {
        guard let chunk = try? handle.read(upToCount: count - data.count),
              !chunk.isEmpty else {
            return nil
        }
        data.append(chunk)
    }
    return data
}

/// Sanity cap on a single playback frame (16 MB ≈ 8.7 min of 16 kHz mono i16)
/// so a corrupt length prefix can't trigger a huge allocation.
let maxFrameBytes = 16 * 1024 * 1024

let stdinThread = Thread {
    let stdinHandle = FileHandle.standardInput
    while true {
        guard let lenData = readExact(stdinHandle, 4) else { break } // EOF
        let len = Int(UInt32(lenData[0]) | (UInt32(lenData[1]) << 8) |
                      (UInt32(lenData[2]) << 16) | (UInt32(lenData[3]) << 24))
        if len == 0 {
            flushPlayback()
            continue
        }
        if len > maxFrameBytes {
            logErr("fonos-voice-capture: playback frame too large (\(len) bytes) — stopping playback input")
            break
        }
        guard let payload = readExact(stdinHandle, len) else { break }
        schedulePayload(payload)
    }
    // EOF: playback goes idle; capture continues until SIGTERM as before.
    logErr("fonos-voice-capture: stdin closed — playback idle, capture continues")
}
stdinThread.name = "fonos.stdin-playback"
stdinThread.start()

// ── Clean shutdown on SIGTERM/SIGINT ───────────────────────────────────────
// Use DispatchSource (its handler may capture, unlike a raw C signal handler).
signal(SIGTERM, SIG_IGN)
signal(SIGINT, SIG_IGN)
let stop: () -> Void = {
    player.stop()
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

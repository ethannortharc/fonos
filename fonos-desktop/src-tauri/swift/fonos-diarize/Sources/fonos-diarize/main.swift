import Foundation
import FluidAudio

// fonos-diarize — 会议说话人分离 helper。
// 协议（spec §5a）：
//   check --models-dir <dir>
//     -> {"available":bool,"models_present":bool,"reason":"…"}
//   download-models --models-dir <dir> [--endpoint <url>]
//     -> {"type":"progress","pct":0-100}… 终态 {"type":"done"} / {"type":"error","message":…}
//   stream --models-dir <dir>
//     stdin: 16kHz s16le mono PCM；stdout: {"type":"ready"} 后持续
//     {"type":"segment","speaker":"s1","start_ms":…,"end_ms":…}
//     同 speaker+start_ms 的后续事件是延长（end_ms 更新），Rust 侧 upsert。

func jsonLine(_ obj: [String: Any]) {
    if let d = try? JSONSerialization.data(withJSONObject: obj),
       let s = String(data: d, encoding: .utf8) {
        print(s)
        fflush(stdout)
    }
}

func argValue(_ name: String) -> String? {
    let a = CommandLine.arguments
    guard let i = a.firstIndex(of: name), i + 1 < a.count else { return nil }
    return a[i + 1]
}

func modelsPresent(in dir: URL) -> Bool {
    // 启发式：目录下存在任何已编译 CoreML 包即视为就绪；
    // 内容正确性由 download-models 的成功终态保证。
    guard let e = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil) else { return false }
    for case let url as URL in e where url.pathExtension == "mlmodelc" { return true }
    return false
}

func loadModel(dir: URL, progress: ((Int) -> Void)?) async throws -> LSEENDModel {
    var last = -1
    // ADAPTED: the brief's inline `progress == nil ? nil : { p in ... }` ternary
    // (both branches type-less: `nil` literal + unannotated closure) makes the
    // call to loadFromHuggingFace's `progressHandler: ProgressHandler?` param
    // ambiguous under the resolved FluidAudio's actual signature ("type of
    // expression is ambiguous without a type annotation" at the call site,
    // reproduced even after binding to an explicitly `ProgressHandler?`-typed
    // local — the ternary itself is what the type checker rejects here).
    // Fix: use if/let instead of a ternary. Same semantics (dedupe repeated
    // percentages), same protocol output — recorded in report.
    var handler: ProgressHandler? = nil
    if let cb = progress {
        handler = { (p: DownloadProgress) in
            let pct = Int(p.fractionCompleted * 100)
            if pct != last { last = pct; cb(pct) }
        }
    }
    return try await LSEENDModel.loadFromHuggingFace(
        variant: .dihard3,
        stepSize: .step100ms,
        cacheDirectory: dir,
        computeUnits: .cpuOnly,   // 文档：此模型 cpuOnly 最快
        progressHandler: handler
    )
}

func emit(_ update: DiarizerTimelineUpdate, ordinals: inout [String: Int]) {
    // ADAPTED: brief's DiarizerSegment.speakerId/startTimeSeconds/endTimeSeconds
    // don't exist on resolved FluidAudio 0.15.5's DiarizerSegment — the real
    // fields are speakerIndex (Int) and startTime/endTime (Float seconds, per
    // LS-EEND.md's own usage example: `segment.startTime`s – `segment.endTime`s).
    // Protocol output (NDJSON shape, ms integers, s<N> ordinal labeling) is
    // unchanged; only the source-field names/types differ. Recorded in report.
    for seg in update.finalizedSegments {
        let raw = "\(seg.speakerIndex)"
        let n: Int
        if let existing = ordinals[raw] { n = existing } else { n = ordinals.count + 1; ordinals[raw] = n }
        jsonLine([
            "type": "segment",
            "speaker": "s\(n)",
            "start_ms": Int(seg.startTime * 1000),
            "end_ms": Int(seg.endTime * 1000),
        ])
    }
}

@main
struct Main {
    static func main() async {
        let a = CommandLine.arguments
        guard a.count >= 2, let dirStr = argValue("--models-dir") else {
            jsonLine(["type": "error",
                      "message": "usage: fonos-diarize <check|download-models|stream> --models-dir <dir> [--endpoint <url>]"])
            exit(2)
        }
        let dir = URL(fileURLWithPath: dirStr)

        switch a[1] {
        case "check":
            // 本二进制能跑（platforms .v14 保证，见 Package.swift 注释）即 available。
            print("{\"available\":true,\"models_present\":\(modelsPresent(in: dir)),\"reason\":\"ok\"}")
            fflush(stdout)

        case "download-models":
            if let ep = argValue("--endpoint"), !ep.isEmpty {
                ModelRegistry.baseURL = ep   // HF 镜像端点
            }
            do {
                try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                _ = try await loadModel(dir: dir) { pct in jsonLine(["type": "progress", "pct": pct]) }
                jsonLine(["type": "done"])
            } catch {
                jsonLine(["type": "error", "message": "\(error)"])
                exit(1)
            }

        case "stream":
            do {
                let model = try await loadModel(dir: dir, progress: nil) // 已缓存→纯本地加载
                let diarizer = LSEENDDiarizer()
                try diarizer.initialize(model: model)
                // Verified against resolved FluidAudio 0.15.5's LSEENDDiarizer.swift:
                // `public func initialize(model: LSEENDModel) throws` exists exactly
                // as assumed — no adaptation needed for this call.
                jsonLine(["type": "ready"])

                var ordinals: [String: Int] = [:]
                let stdin = FileHandle.standardInput
                var pending = Data()
                while true {
                    guard let data = try stdin.read(upToCount: 6400), !data.isEmpty else { break } // 200ms/块
                    pending.append(data)
                    let usable = pending.count - (pending.count % 2)
                    if usable == 0 { continue }
                    let chunk = pending.prefix(usable)
                    pending.removeFirst(usable)
                    var samples = [Float](repeating: 0, count: usable / 2)
                    chunk.withUnsafeBytes { raw in
                        let i16 = raw.bindMemory(to: Int16.self)
                        for i in 0..<samples.count { samples[i] = Float(Int16(littleEndian: i16[i])) / 32768.0 }
                    }
                    try diarizer.addAudio(samples, sourceSampleRate: 16_000)
                    if let update = try diarizer.process() { emit(update, ordinals: &ordinals) }
                }
                if let update = try diarizer.process() { emit(update, ordinals: &ordinals) } // EOF flush
            } catch {
                jsonLine(["type": "error", "message": "\(error)"])
                exit(1)
            }

        default:
            jsonLine(["type": "error", "message": "unknown subcommand \(a[1])"])
            exit(2)
        }
    }
}

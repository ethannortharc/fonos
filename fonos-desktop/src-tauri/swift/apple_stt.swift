// fonos-stt-apple: Transcribe a WAV file using macOS SFSpeechRecognizer.
//
// Usage: fonos-stt-apple <wav-path> [locale] [contextual-json]
//   locale: BCP-47 language tag (e.g. "en-US", "zh-CN", "ja-JP"). Default: "en-US"
//   contextual-json: JSON array of vocabulary strings passed to the
//     recognizer as contextualStrings (biases recognition toward them)
//
// Output (stdout): {"text": "transcribed text"}
// Error  (stdout): {"error": "description"}
//
// Build: swiftc -O -o fonos-stt-apple apple_stt.swift -framework Speech -framework Foundation

import Foundation
import Speech

// ── Helpers ──────────────────────────────────────────────────────────────────

func output(_ dict: [String: Any]) -> Never {
    let data = try! JSONSerialization.data(withJSONObject: dict, options: [])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)
    let hasError = dict["error"] != nil
    exit(hasError ? 1 : 0)
}

func fail(_ msg: String) -> Never {
    output(["error": msg])
}

// ── Args ─────────────────────────────────────────────────────────────────────

guard CommandLine.arguments.count >= 2 else {
    fail("Usage: fonos-stt-apple <wav-path> [locale]")
}

let wavPath = CommandLine.arguments[1]
let localeId = CommandLine.arguments.count >= 3 ? CommandLine.arguments[2] : "en-US"
let contextualStrings: [String] = {
    guard CommandLine.arguments.count >= 4,
          let data = CommandLine.arguments[3].data(using: .utf8),
          let arr = try? JSONDecoder().decode([String].self, from: data) else {
        return []
    }
    return arr
}()

guard FileManager.default.fileExists(atPath: wavPath) else {
    fail("Audio file not found: \(wavPath)")
}

// ── Authorization ────────────────────────────────────────────────────────────

let authSema = DispatchSemaphore(value: 0)
var authStatus: SFSpeechRecognizerAuthorizationStatus = .notDetermined

SFSpeechRecognizer.requestAuthorization { status in
    authStatus = status
    authSema.signal()
}
authSema.wait()

switch authStatus {
case .authorized:
    break
case .denied:
    fail("Speech recognition permission denied. Grant access in System Settings > Privacy > Speech Recognition.")
case .restricted:
    fail("Speech recognition is restricted on this device.")
case .notDetermined:
    fail("Speech recognition authorization not determined.")
@unknown default:
    fail("Unknown speech recognition authorization status.")
}

// ── Recognize ────────────────────────────────────────────────────────────────

let locale = Locale(identifier: localeId)
guard let recognizer = SFSpeechRecognizer(locale: locale) else {
    fail("SFSpeechRecognizer not available for locale: \(localeId)")
}

guard recognizer.isAvailable else {
    fail("Speech recognizer is not available (offline model may not be downloaded for \(localeId))")
}

// ── Recognize (on-device first, server fallback) ─────────────────────────────

var supportsOnDevice = false
if #available(macOS 13.0, *) {
    supportsOnDevice = recognizer.supportsOnDeviceRecognition
}

func recognize(forceOnDevice: Bool) -> (text: String, error: String) {
    let fileUrl = URL(fileURLWithPath: wavPath)
    let req = SFSpeechURLRecognitionRequest(url: fileUrl)
    req.shouldReportPartialResults = false
    if !contextualStrings.isEmpty {
        req.contextualStrings = contextualStrings
    }
    if #available(macOS 13.0, *) {
        req.requiresOnDeviceRecognition = forceOnDevice
    }

    var text = ""
    var err = ""
    var finished = false

    recognizer.recognitionTask(with: req) { result, error in
        if let error = error {
            err = error.localizedDescription
            finished = true
            return
        }
        if let result = result, result.isFinal {
            text = result.bestTranscription.formattedString
            finished = true
        }
    }

    let deadline = Date().addingTimeInterval(30)
    while !finished && Date() < deadline {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
    }

    if !finished { err = "Speech recognition timed out after 30s" }
    return (text, err)
}

var engine = "server"
var resultText = ""
var errorMessage = ""

// Try on-device first if supported
if supportsOnDevice {
    let (text, err) = recognize(forceOnDevice: true)
    if err.isEmpty && !text.isEmpty {
        engine = "on-device"
        resultText = text
    } else {
        // On-device failed — fall back to server
        let (text2, err2) = recognize(forceOnDevice: false)
        resultText = text2
        errorMessage = err2
        engine = "server"
    }
} else {
    // No on-device support — use server directly
    let (text, err) = recognize(forceOnDevice: false)
    resultText = text
    errorMessage = err
    engine = "server"
}

if !errorMessage.isEmpty {
    fail(errorMessage)
}

output([
    "text": resultText,
    "engine": engine,
    "locale": localeId,
    "on_device_available": supportsOnDevice,
] as [String: Any])

import Foundation

/// How the user triggers recording.
enum RecordMode: String, Codable, Equatable, Hashable, Sendable {
    case tap
    case hold
}

/// Application-level configuration. Persisted via Codable → UserDefaults.
/// API keys are NOT stored here — they live in the Keychain only.
struct AppConfig: Codable, Equatable {
    var sttProvider: String = "apple"
    var sttLanguage: String = "auto"
    var llmProvider: String = "openai"
    var llmBaseURL: String = ""
    var destinations: [AnyTextDestination] = [AnyTextDestination(ClipboardDestination())]
    var modelProfiles: [ModelProfile] = []
    var modes: [Mode] = Mode.builtInModes
    var recordMode: RecordMode = .tap
    var autoSendDestination: String = ""
    var historyRetentionDays: Int = 30

    /// Convenience: the first mode in the list, or .raw if empty.
    var defaultMode: Mode {
        modes.first ?? .raw
    }
}

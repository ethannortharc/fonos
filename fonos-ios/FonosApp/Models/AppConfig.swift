import Foundation

/// How the user triggers recording.
enum RecordMode: String, Codable, Equatable, Hashable, Sendable {
    case tap
    case hold
}

/// Application-level configuration. Persisted via Codable → UserDefaults.
/// API keys are NOT stored here — they live in the Keychain only.
struct AppConfig: Codable, Equatable {
    // MARK: - STT

    var sttProvider: String = "apple"     // Legacy: keep for backward compat
    var sttProfile: String = ""           // Default STT model profile ID
    var sttLanguage: String = "auto"

    // MARK: - LLM

    var llmProvider: String = "openai"    // Legacy: keep for backward compat
    var llmProfile: String = ""           // Default LLM model profile ID
    var llmBaseURL: String = ""

    // MARK: - Modes

    /// Active mode identifier (matches ModeConfig.id or Mode.id).
    var activeModeID: String = "raw"

    /// Target language for Translate mode.
    var translateTargetLanguage: String = "English"

    /// Legacy modes array — kept for Codable backward compat and existing tests.
    var modes: [Mode] = Mode.builtInModes

    /// Mode configurations with per-mode model overrides and settings.
    var modeConfigs: [ModeConfig] = ModeConfig.builtInConfigs

    // MARK: - Models

    var modelProfiles: [ModelProfile] = []

    // MARK: - Recording

    var recordMode: RecordMode = .tap
    var autoSendDestination: String = ""

    // MARK: - Destinations

    var destinations: [AnyTextDestination] = [AnyTextDestination(ClipboardDestination())]

    // MARK: - History

    var historyRetentionDays: Int = 30

    // MARK: - Convenience

    /// The active mode derived from `modeConfigs`, falling back to the first mode in `modes`.
    var defaultMode: Mode {
        if let config = modeConfigs.first(where: { $0.id == activeModeID }) {
            return config.mode
        }
        return modes.first ?? .raw
    }

    /// The active ModeConfig, or nil if not found.
    var activeModeConfig: ModeConfig? {
        modeConfigs.first(where: { $0.id == activeModeID })
    }
}

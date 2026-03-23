import AppIntents

/// Siri/Shortcuts intent for launching Fonos into recording state.
/// Scaffold placeholder — implementation goes in wp-executor.
struct StartDictationIntent: AppIntent {
    static let title: LocalizedStringResource = "Start Dictation"
    static let description = IntentDescription("Opens Fonos and starts recording.")

    func perform() async throws -> some IntentResult {
        return .result()
    }
}

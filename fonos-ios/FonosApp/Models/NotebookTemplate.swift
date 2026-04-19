import Foundation

/// Seed prompts shown when creating a new notebook and used to back-fill old
/// `processingMode` values into `systemPrompt` during v0.2.0 migration.
///
/// Templates are seeds, not types — once a notebook is created only its
/// `systemPrompt` matters. Editing a template here does not retroactively change
/// any existing notebook.
enum NotebookTemplate: String, CaseIterable, Identifiable {
    case raw
    case polish
    case meetingNotes
    case translate
    case blank

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .raw:           return "Raw"
        case .polish:        return "Polish"
        case .meetingNotes:  return "Meeting Notes"
        case .translate:     return "Translate"
        case .blank:         return "Blank"
        }
    }

    var symbolName: String {
        switch self {
        case .raw:           return "waveform"
        case .polish:        return "sparkles"
        case .meetingNotes:  return "list.bullet.rectangle"
        case .translate:     return "globe"
        case .blank:         return "doc"
        }
    }

    var systemPromptSeed: String {
        switch self {
        case .raw, .blank:
            return ""
        case .polish:
            return "Clean up filler words and disfluencies. Preserve the speaker's original meaning and tone."
        case .meetingNotes:
            return "Summarize as bullet-point meeting minutes with action items grouped at the bottom."
        case .translate:
            return "Translate the text accurately to the target language. Preserve tone."
        }
    }
}

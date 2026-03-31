import AppIntents
import Foundation
import UIKit

// MARK: - DictateIntent

/// AppIntent for Siri Shortcuts: "Dictate with Fonos"
///
/// Users can add this intent to Shortcuts or invoke it via Siri.
/// Optional parameters allow pre-selecting a processing mode and destination.
struct DictateIntent: AppIntent {
    // MARK: - Metadata

    static let title: LocalizedStringResource = "Dictate with Fonos"

    static let description = IntentDescription(
        "Opens Fonos and starts a dictation session.",
        categoryName: "Dictation"
    )

    /// Intent opens the app to begin recording.
    static let openAppWhenRun: Bool = true

    // MARK: - Parameters

    /// Processing mode: "raw", "polish", "formal", "translate", or "custom".
    @Parameter(title: "Mode", description: "Processing mode for the dictation.", default: "raw")
    var mode: String?

    /// Destination: "clipboard", "messages", "email", etc.
    @Parameter(title: "Destination", description: "Where to send the transcribed text.", default: "clipboard")
    var destination: String?

    // MARK: - Perform

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        // Build the deep link URL with optional parameters
        var components = URLComponents(string: "fonos://record")!
        var queryItems: [URLQueryItem] = []

        if let mode = mode, !mode.isEmpty {
            queryItems.append(URLQueryItem(name: "mode", value: mode))
        }
        if let destination = destination, !destination.isEmpty {
            queryItems.append(URLQueryItem(name: "destination", value: destination))
        }
        if !queryItems.isEmpty {
            components.queryItems = queryItems
        }

        // Open the deep link — the app handles recording in FonosApp.swift
        if let url = components.url {
            await UIApplication.shared.open(url)
        }

        // Return a descriptive result string for Shortcuts automation
        let resultDescription = "Fonos dictation started"
        return .result(value: resultDescription)
    }
}

// MARK: - Shortcuts Provider

/// Registers intents as app shortcuts so they appear automatically in Shortcuts.
struct FonosShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: DictateIntent(),
            phrases: [
                "Dictate with \(.applicationName)",
                "Start dictation in \(.applicationName)",
                "Record with \(.applicationName)"
            ],
            shortTitle: "Dictate",
            systemImageName: "mic.fill"
        )
        AppShortcut(
            intent: RecordNoteIntent(),
            phrases: [
                "Record a note in \(.applicationName)",
                "Take a note with \(.applicationName)"
            ],
            shortTitle: "Record Note",
            systemImageName: "note.text"
        )
    }
}

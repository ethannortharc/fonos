import AppIntents
import Foundation
import UIKit

// MARK: - RecordNoteIntent

/// AppIntent for Shortcuts / Back Tap: "Record a Note"
///
/// Users can add this intent to Shortcuts or invoke it via Back Tap.
/// An optional notebookId parameter allows recording directly into a specific notebook.
struct RecordNoteIntent: AppIntent {
    // MARK: - Metadata

    static let title: LocalizedStringResource = "Record a Note"

    static let description = IntentDescription(
        "Opens Fonos and starts recording a voice note.",
        categoryName: "Notes"
    )

    /// Intent opens the app to begin recording.
    static let openAppWhenRun: Bool = true

    // MARK: - Parameters

    /// Optional target notebook UUID string.
    @Parameter(title: "Notebook", description: "Which notebook to record into.", default: nil)
    var notebookId: String?

    // MARK: - Instance accessors (for testability)

    /// Instance-level title accessor forwarding to the static declaration.
    var title: LocalizedStringResource { Self.title }

    /// Instance-level openAppWhenRun accessor forwarding to the static declaration.
    var openAppWhenRun: Bool { Self.openAppWhenRun }

    /// Instance-level description accessor returning the category name string.
    var intentDescription: String? { "Opens Fonos and starts recording a voice note." }

    // MARK: - Perform

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        var components = URLComponents(string: "fonos://note")!
        if let notebookId, !notebookId.isEmpty {
            components.queryItems = [URLQueryItem(name: "notebook", value: notebookId)]
        }
        if let url = components.url {
            await UIApplication.shared.open(url)
        }
        return .result(value: "Fonos note recording started")
    }
}

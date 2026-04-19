import AppIntents
import Foundation
import UIKit

// MARK: - NotebookOptionsProvider

/// Reads the SharedNotebookCatalog so Siri / Shortcuts.app can list real
/// notebook titles instead of asking the user for raw UUIDs.
struct NotebookOptionsProvider: DynamicOptionsProvider {
    /// Override target for tests. Production uses SharedNotebookCatalog.defaultURL.
    let catalogURL: URL?

    init(catalogURL: URL? = SharedNotebookCatalog.defaultURL) {
        self.catalogURL = catalogURL
    }

    func results() async throws -> [String] {
        SharedNotebookCatalog.read(from: catalogURL).map(\.title)
    }
}

// MARK: - RecordNoteIntent

/// AppIntent for Shortcuts / Siri / Back Tap: "Record a Note"
///
/// Users can add this intent to Shortcuts or invoke it via Back Tap.
/// The notebook parameter is exposed as a dynamic options list so the user
/// can pick by title rather than entering a UUID.
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

    /// User-facing title of the target notebook. Resolved to a UUID at perform time
    /// via `SharedNotebookCatalog`.
    ///
    /// Kept as a free-text `String?` parameter rather than wired to a
    /// `DynamicOptionsProvider` because that API requires iOS 26. v1 ships with
    /// the AppShortcuts phrase + URL-based deep link; the curated picker will
    /// follow when we drop iOS <26 support or upgrade to AppEntity.
    @Parameter(
        title: "Notebook",
        description: "Which notebook to record into.",
        default: nil
    )
    var notebookId: String?

    // MARK: - Instance accessors (for testability)

    var title: LocalizedStringResource { Self.title }
    var openAppWhenRun: Bool { Self.openAppWhenRun }
    var intentDescription: String? { "Opens Fonos and starts recording a voice note." }

    // MARK: - Perform

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        var components = URLComponents(string: "fonos://note")!

        if let notebookId, !notebookId.isEmpty {
            // notebookId may be a title (from DynamicOptionsProvider) or a literal UUID.
            // Resolve title → uuid via the catalog so the URL scheme stays UUID-based.
            let entries = SharedNotebookCatalog.read()
            if let entry = entries.first(where: { $0.title == notebookId }) {
                components.queryItems = [URLQueryItem(name: "notebook", value: entry.id)]
            } else {
                components.queryItems = [URLQueryItem(name: "notebook", value: notebookId)]
            }
        }

        if let url = components.url {
            await UIApplication.shared.open(url)
        }
        return .result(value: "Fonos note recording started")
    }
}

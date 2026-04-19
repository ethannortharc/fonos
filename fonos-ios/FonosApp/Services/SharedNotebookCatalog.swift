import Foundation
import os.log

private let catalogLog = Logger(subsystem: "com.fonos.ios", category: "SharedNotebookCatalog")

/// A small JSON catalog of (notebook id, title) pairs persisted in the shared
/// App Group container so AppShortcuts / DynamicOptionsProvider can list
/// notebooks without booting the SwiftData stack.
enum SharedNotebookCatalog {

    static let appGroupID = "group.com.fonos.ios"
    static let filename   = "notebooks.json"

    struct Entry: Codable, Equatable, Sendable {
        let id: String
        let title: String
    }

    /// Default URL inside the shared App Group container.
    /// Returns nil if the App Group is unavailable for the running target.
    static var defaultURL: URL? {
        FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: appGroupID)?
            .appendingPathComponent(filename)
    }

    static func write(_ entries: [Entry], to url: URL? = defaultURL) throws {
        guard let url else {
            catalogLog.warning("App Group container unavailable; catalog not persisted.")
            return
        }
        let data = try JSONEncoder().encode(entries)
        try data.write(to: url, options: [.atomic])
    }

    static func read(from url: URL? = defaultURL) -> [Entry] {
        guard let url, let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Entry].self, from: data)) ?? []
    }
}

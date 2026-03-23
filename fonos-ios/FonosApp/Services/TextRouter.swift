import Foundation

/// Routes processed text to a configured destination.
/// Destination types are defined in Models/Destination.swift.
struct TextRouter {
    /// Sends text to the first available destination.
    static func route(_ text: String, to destination: any TextDestination) async throws {
        try await destination.send(text)
    }
}

import Foundation
import SwiftData

/// Persisted record of a completed dictation session.
@Model
final class DictationSession {
    var id: UUID
    var date: Date
    var mode: String
    var inputText: String
    var outputText: String
    var destination: String
    var latencyMs: Double

    init(
        id: UUID = UUID(),
        date: Date = Date(),
        mode: String = "raw",
        inputText: String = "",
        outputText: String = "",
        destination: String = "clipboard",
        latencyMs: Double = 0
    ) {
        self.id = id
        self.date = date
        self.mode = mode
        self.inputText = inputText
        self.outputText = outputText
        self.destination = destination
        self.latencyMs = latencyMs
    }
}

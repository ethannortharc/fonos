import Foundation

/// Represents a configured LLM provider profile.
struct ModelProfile: Codable, Equatable, Hashable, Sendable {
    var id: String
    var name: String
    var provider: String
    var modelID: String
    var baseURL: String?
    var temperature: Double?
    var maxTokens: Int?
}

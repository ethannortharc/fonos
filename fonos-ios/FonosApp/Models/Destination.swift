import Foundation
import UIKit

// MARK: - Protocol

/// A destination to which processed text can be sent.
protocol TextDestination: Sendable {
    var id: String { get }
    func send(_ text: String) async throws
}

// MARK: - Clipboard

/// Sends text to the system clipboard.
struct ClipboardDestination: TextDestination, Codable, Equatable, Sendable {
    var id: String = "clipboard"

    func send(_ text: String) async throws {
        await MainActor.run {
            UIPasteboard.general.string = text
        }
    }
}

// MARK: - URL Scheme

/// Sends text by opening a URL built from a template.
/// Use `{text}` as the placeholder in the template string.
struct URLSchemeDestination: TextDestination, Codable, Equatable, Sendable {
    var template: String

    var id: String { "url_scheme" }

    /// Builds the URL by substituting {text} in the template.
    func buildURL(for text: String) -> URL? {
        let encoded = text.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? text
        let urlString = template.replacingOccurrences(of: "{text}", with: encoded)
        return URL(string: urlString)
    }

    func send(_ text: String) async throws {
        guard let url = buildURL(for: text) else {
            throw TextRoutingError.invalidURLTemplate(template)
        }
        await MainActor.run {
            UIApplication.shared.open(url)
        }
    }
}

// MARK: - Messages

/// Sends text to the iOS Messages app via URL scheme.
struct MessagesDestination: TextDestination, Codable, Equatable, Sendable {
    var id: String = "messages"

    func send(_ text: String) async throws {
        let encoded = text.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? text
        guard let url = URL(string: "sms:&body=\(encoded)") else {
            throw TextRoutingError.destinationUnavailable("messages")
        }
        await MainActor.run {
            UIApplication.shared.open(url)
        }
    }
}

// MARK: - Type-erased wrapper

/// Type-erased wrapper enabling `[AnyTextDestination]` to be stored as a Codable array.
struct AnyTextDestination: Codable, Equatable, Sendable {
    private enum TypeKey: String, Codable {
        case clipboard
        case urlScheme = "url_scheme"
        case messages
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case clipboard
        case urlScheme
        case messages
    }

    private let _id: String
    private let _destination: any TextDestination & Sendable

    var id: String { _id }

    // MARK: - Initialisers

    init(_ destination: ClipboardDestination) {
        _destination = destination
        _id = destination.id
    }

    init(_ destination: URLSchemeDestination) {
        _destination = destination
        _id = destination.id
    }

    init(_ destination: MessagesDestination) {
        _destination = destination
        _id = destination.id
    }

    func send(_ text: String) async throws {
        try await _destination.send(text)
    }

    // MARK: - Codable

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(TypeKey.self, forKey: .type)
        switch type {
        case .clipboard:
            let dest = try container.decode(ClipboardDestination.self, forKey: .clipboard)
            _destination = dest
            _id = dest.id
        case .urlScheme:
            let dest = try container.decode(URLSchemeDestination.self, forKey: .urlScheme)
            _destination = dest
            _id = dest.id
        case .messages:
            let dest = try container.decode(MessagesDestination.self, forKey: .messages)
            _destination = dest
            _id = dest.id
        }
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch _destination {
        case let dest as ClipboardDestination:
            try container.encode(TypeKey.clipboard, forKey: .type)
            try container.encode(dest, forKey: .clipboard)
        case let dest as URLSchemeDestination:
            try container.encode(TypeKey.urlScheme, forKey: .type)
            try container.encode(dest, forKey: .urlScheme)
        case let dest as MessagesDestination:
            try container.encode(TypeKey.messages, forKey: .type)
            try container.encode(dest, forKey: .messages)
        default:
            // Fallback: encode as clipboard
            try container.encode(TypeKey.clipboard, forKey: .type)
            try container.encode(ClipboardDestination(), forKey: .clipboard)
        }
    }

    // MARK: - Equatable

    static func == (lhs: AnyTextDestination, rhs: AnyTextDestination) -> Bool {
        lhs._id == rhs._id &&
        type(of: lhs._destination) == type(of: rhs._destination)
    }
}

// MARK: - Errors

enum TextRoutingError: LocalizedError {
    case invalidURLTemplate(String)
    case destinationUnavailable(String)

    var errorDescription: String? {
        switch self {
        case .invalidURLTemplate(let template): "Invalid URL template: \(template)"
        case .destinationUnavailable(let destination): "Destination unavailable: \(destination)"
        }
    }
}

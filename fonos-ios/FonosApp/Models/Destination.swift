import Foundation
import UIKit

// MARK: - Protocol

/// A destination to which processed text can be sent.
protocol TextDestination: Sendable {
    var id: String { get }
    func send(text: String) async throws
}

// MARK: - Errors

/// Errors produced by destination operations.
enum DestinationError: LocalizedError, Equatable {
    case invalidURLTemplate
    case destinationUnavailable(String)

    var errorDescription: String? {
        switch self {
        case .invalidURLTemplate:
            return "The URL template is invalid and could not produce a valid URL."
        case .destinationUnavailable(let destination):
            return "Destination unavailable: \(destination)"
        }
    }
}

// MARK: - Clipboard

/// Sends text to the system clipboard.
struct ClipboardDestination: TextDestination, Codable, Equatable, Sendable {
    var id: String = "clipboard"

    @MainActor
    func send(text: String) async throws {
        UIPasteboard.general.string = text
    }
}

// MARK: - URL Scheme

/// Sends text by opening a URL built from a template.
/// Use `{text}` as the placeholder in the template string.
struct URLSchemeDestination: TextDestination, Codable, Equatable, Sendable {
    var template: String

    var id: String { "url_scheme" }

    /// Builds the URL by substituting `{text}` in the template with the percent-encoded text.
    ///
    /// - Parameter text: The text to encode and substitute.
    /// - Returns: A valid `URL`.
    /// - Throws: `DestinationError.invalidURLTemplate` when the result is not a valid URL.
    func buildURL(for text: String) throws -> URL {
        // Validate the template: it must have a scheme and no invalid URL characters.
        // We check this first before any substitution.
        if !isValidURLTemplate(template) {
            throw DestinationError.invalidURLTemplate
        }

        if !template.contains("{text}") {
            // No placeholder — return the template URL as-is.
            guard let url = URL(string: template) else {
                throw DestinationError.invalidURLTemplate
            }
            return url
        }

        // Percent-encode all characters that are not allowed in a URL query value.
        // We use a custom character set that excludes characters that must be escaped
        // inside a query parameter value (& = + # etc.).
        var queryValueAllowed = CharacterSet.urlQueryAllowed
        queryValueAllowed.remove(charactersIn: "&=+#%")

        guard let encoded = text.addingPercentEncoding(withAllowedCharacters: queryValueAllowed) else {
            throw DestinationError.invalidURLTemplate
        }

        let urlString = template.replacingOccurrences(of: "{text}", with: encoded)
        guard let url = URL(string: urlString) else {
            throw DestinationError.invalidURLTemplate
        }
        return url
    }

    /// Returns `true` if the template string is a plausible URL template.
    /// A valid template must:
    ///  - contain a scheme (e.g., `myapp://`)
    ///  - not contain spaces
    ///  - not contain invalid percent-encoding sequences
    private func isValidURLTemplate(_ template: String) -> Bool {
        // Must have a scheme separator
        guard template.contains("://") else { return false }
        // Must not contain literal spaces
        if template.contains(" ") { return false }
        // Must not contain `%%%` (invalid percent encoding)
        if template.contains("%%%") { return false }
        return true
    }

    func send(text: String) async throws {
        let url = try buildURL(for: text)
        await MainActor.run {
            UIApplication.shared.open(url)
        }
    }
}

// MARK: - Messages

/// Sends text to the iOS Messages app via the `sms:` URL scheme.
struct MessagesDestination: TextDestination, Codable, Equatable, Sendable {
    var id: String = "messages"

    /// Builds the `sms:` URL with the text as the body.
    ///
    /// - Parameter text: The message body.
    /// - Returns: A valid `sms:` URL.
    /// - Throws: `DestinationError.destinationUnavailable` when a valid URL cannot be formed.
    func buildURL(for text: String) throws -> URL {
        var queryValueAllowed = CharacterSet.urlQueryAllowed
        queryValueAllowed.remove(charactersIn: "&=+#%")
        let encoded = text.addingPercentEncoding(withAllowedCharacters: queryValueAllowed) ?? ""
        guard let url = URL(string: "sms:&body=\(encoded)") else {
            throw DestinationError.destinationUnavailable("messages")
        }
        return url
    }

    func send(text: String) async throws {
        let url = try buildURL(for: text)
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

    func send(text: String) async throws {
        try await _destination.send(text: text)
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

// MARK: - Legacy type alias

/// Backward-compatible alias retained for any code using the old name.
typealias TextRoutingError = DestinationError

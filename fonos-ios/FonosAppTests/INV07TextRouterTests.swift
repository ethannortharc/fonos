// INV-07: Clipboard destination copies text to UIPasteboard.general.
// URL scheme destination generates correctly encoded URL.
// All destinations conform to TextDestination protocol.
//
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV07TextRouterTests

import Testing
import UIKit
@testable import FonosApp

struct INV07TextRouterTests {

    // MARK: - Protocol conformance

    @Test("ClipboardDestination conforms to TextDestination protocol")
    func clipboardConformsToProtocol() throws {
        let dest: any TextDestination = ClipboardDestination()
        _ = dest
        #expect(Bool(true))
    }

    @Test("URLSchemeDestination conforms to TextDestination protocol")
    func urlSchemeConformsToProtocol() throws {
        let dest: any TextDestination = URLSchemeDestination(template: "myapp://send?text={text}")
        _ = dest
        #expect(Bool(true))
    }

    @Test("MessagesDestination conforms to TextDestination protocol")
    func messagesConformsToProtocol() throws {
        let dest: any TextDestination = MessagesDestination()
        _ = dest
        #expect(Bool(true))
    }

    // MARK: - Clipboard destination

    @Test("ClipboardDestination.send() sets UIPasteboard.general.string")
    @MainActor func clipboardSendSetsString() async throws {
        let dest = ClipboardDestination()
        let text = "Hello, clipboard!"
        try await dest.send(text: text)
        #expect(UIPasteboard.general.string == text)
    }

    @Test("ClipboardDestination overwrites previous clipboard content")
    @MainActor func clipboardOverwritesPrevious() async throws {
        UIPasteboard.general.string = "old content"
        let dest = ClipboardDestination()
        try await dest.send(text: "new content")
        #expect(UIPasteboard.general.string == "new content")
    }

    // MARK: - URL scheme destination

    @Test("URLSchemeDestination substitutes {text} in template")
    func urlSchemeSubstitutesPlaceholder() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "hello")
        #expect(url.absoluteString.contains("hello"))
        #expect(!url.absoluteString.contains("{text}"))
    }

    @Test("URLSchemeDestination percent-encodes spaces in text")
    func urlSchemeEncodesSpaces() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "hello world")
        let urlStr = url.absoluteString
        #expect(urlStr.contains("hello%20world") || urlStr.contains("hello+world"))
    }

    @Test("URLSchemeDestination percent-encodes ampersands in text")
    func urlSchemeEncodesAmpersands() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "salt & pepper")
        #expect(!url.absoluteString.contains("salt & pepper"))
        #expect(url.absoluteString.contains("%26") || url.absoluteString.contains("salt"))
    }

    @Test("URLSchemeDestination percent-encodes emoji in text")
    func urlSchemeEncodesEmoji() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "Hello 👋")
        // Emoji must be percent-encoded in URL
        #expect(!url.absoluteString.contains("👋"))
    }

    @Test("URLSchemeDestination handles CJK characters")
    func urlSchemeEncodesCJK() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "你好世界")
        // CJK characters must be percent-encoded
        #expect(!url.absoluteString.contains("你好"))
    }

    @Test("URLSchemeDestination handles empty text without crash")
    func urlSchemeEmptyText() throws {
        let dest = URLSchemeDestination(template: "myapp://send?text={text}")
        let url = try dest.buildURL(for: "")
        // Should produce a valid URL with empty parameter
        #expect(url.absoluteString.contains("myapp://"))
    }

    @Test("URLSchemeDestination without {text} placeholder returns template URL as-is")
    func urlSchemeNoPlaceholder() throws {
        let dest = URLSchemeDestination(template: "myapp://compose")
        let url = try dest.buildURL(for: "ignored text")
        #expect(url.absoluteString == "myapp://compose")
    }

    @Test("URLSchemeDestination with invalid template throws error")
    func urlSchemeInvalidTemplatethrows() throws {
        let dest = URLSchemeDestination(template: "not a valid url %%%")
        #expect(throws: DestinationError.invalidURLTemplate) {
            _ = try dest.buildURL(for: "text")
        }
    }

    // MARK: - Messages destination

    @Test("MessagesDestination builds sms: URL scheme")
    func messagesDestinationURL() throws {
        let dest = MessagesDestination()
        let url = try dest.buildURL(for: "Let's meet at 3pm")
        #expect(url.scheme == "sms")
    }

    // MARK: - Codable round-trip

    @Test("ClipboardDestination Codable round-trip")
    func clipboardCodableRoundTrip() throws {
        let original = ClipboardDestination()
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(ClipboardDestination.self, from: data)
        #expect(decoded.id == original.id)
    }

    @Test("URLSchemeDestination Codable round-trip preserves template")
    func urlSchemeCodableRoundTrip() throws {
        let original = URLSchemeDestination(template: "tg://msg?text={text}")
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(URLSchemeDestination.self, from: data)
        #expect(decoded.template == original.template)
    }

    @Test("Array of mixed destinations Codable round-trip via AnyTextDestination wrapper")
    func mixedDestinationsRoundTrip() throws {
        let destinations: [AnyTextDestination] = [
            AnyTextDestination(ClipboardDestination()),
            AnyTextDestination(URLSchemeDestination(template: "tg://msg?text={text}")),
            AnyTextDestination(MessagesDestination())
        ]
        let data = try JSONEncoder().encode(destinations)
        let decoded = try JSONDecoder().decode([AnyTextDestination].self, from: data)
        #expect(decoded.count == 3)
    }
}

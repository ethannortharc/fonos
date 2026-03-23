// INV-08: Text routing URL scheme destinations generate correctly percent-encoded URLs
// with {text} substitution for real-world app templates.
//
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV08URLSchemeTests

import Testing
import Foundation
@testable import FonosApp

struct INV08URLSchemeTests {

    // MARK: - WeChat / Weixin

    @Test("WeChat template with Chinese text produces percent-encoded URL")
    func weChatChineseText() throws {
        let dest = URLSchemeDestination(template: "weixin://dl/moments?text={text}")
        let url = try dest.buildURL(for: "你好世界")
        // The URL must be valid
        #expect(url.scheme == "weixin")
        // CJK characters must be percent-encoded
        #expect(!url.absoluteString.contains("你好"))
        // The percent-encoded representation of "你" starts with %E4%BD%A0
        #expect(url.absoluteString.contains("%E4") || url.absoluteString.contains("%e4"))
    }

    // MARK: - Telegram

    @Test("Telegram template with ampersand and equals sign produces correct encoding")
    func telegramSpecialChars() throws {
        let dest = URLSchemeDestination(template: "tg://msg?text={text}")
        let url = try dest.buildURL(for: "key=value&another=thing")
        // & and = in the text value must be encoded so they don't break URL parsing
        let query = url.query ?? url.absoluteString
        #expect(query.contains("%26") || !query.contains("&another"))
        #expect(query.contains("%3D") || !query.contains("=thing"))
    }

    @Test("Telegram template with plain text produces valid tg:// URL")
    func telegramPlainText() throws {
        let dest = URLSchemeDestination(template: "tg://msg?text={text}")
        let url = try dest.buildURL(for: "hello from fonos")
        #expect(url.scheme == "tg")
        #expect(url.absoluteString.contains("hello"))
    }

    // MARK: - Slack

    @Test("Slack template with multiline text encodes newlines")
    func slackMultilineText() throws {
        let dest = URLSchemeDestination(template: "slack://channel?text={text}")
        let url = try dest.buildURL(for: "Line one\nLine two\nLine three")
        // Newlines (\n = %0A) must be encoded
        let urlStr = url.absoluteString
        #expect(!urlStr.contains("\n"))
        #expect(urlStr.contains("%0A") || urlStr.contains("%0a") || urlStr.contains("+"))
    }

    // MARK: - Edge cases

    @Test("Empty text produces URL with empty parameter value — no crash")
    func emptyTextNocrash() throws {
        let dest = URLSchemeDestination(template: "myapp://share?msg={text}")
        let url = try dest.buildURL(for: "")
        // URL must be constructible and valid
        #expect(url.scheme == "myapp")
    }

    @Test("Text with only whitespace is encoded, not stripped")
    func whitespaceOnlyText() throws {
        let dest = URLSchemeDestination(template: "myapp://share?msg={text}")
        let url = try dest.buildURL(for: "   ")
        // Spaces must appear encoded in the URL
        let urlStr = url.absoluteString
        #expect(urlStr.contains("%20") || urlStr.contains("+"))
    }

    @Test("Template without {text} placeholder returns unmodified template URL")
    func templateWithoutPlaceholder() throws {
        let dest = URLSchemeDestination(template: "slack://open?team=T123")
        let url = try dest.buildURL(for: "should be ignored")
        #expect(url.absoluteString == "slack://open?team=T123")
    }

    @Test("Text with emoji round-trips through URL encoding")
    func emojiRoundTrip() throws {
        let dest = URLSchemeDestination(template: "myapp://share?msg={text}")
        let inputText = "Great job! 🎉🚀"
        let url = try dest.buildURL(for: inputText)
        // Reconstruct the text by decoding the query parameter
        guard let query = url.query,
              let encodedValue = query.components(separatedBy: "msg=").last,
              let decoded = encodedValue.removingPercentEncoding?.replacingOccurrences(of: "+", with: " ") else {
            Issue.record("Could not extract query from URL: \(url)")
            return
        }
        #expect(decoded == inputText)
    }

    @Test("Very long text (> 2000 chars) does not crash URL encoder")
    func veryLongTextNoCrash() throws {
        let dest = URLSchemeDestination(template: "myapp://share?msg={text}")
        let longText = String(repeating: "Hello world. ", count: 200) // ~2600 chars
        let url = try dest.buildURL(for: longText)
        #expect(url.scheme == "myapp")
    }

    @Test("Text with hash symbol is encoded correctly")
    func hashSymbolEncoded() throws {
        let dest = URLSchemeDestination(template: "myapp://share?msg={text}")
        let url = try dest.buildURL(for: "chapter #5")
        // # in a query value must be encoded as %23
        #expect(!url.absoluteString.contains("chapter #5"))
    }
}

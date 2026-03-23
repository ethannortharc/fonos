// QD-02: STT response parsing correctness.
// Tests all response format variants: standard, verbose_json, Fonos server format,
// empty transcripts, and Unicode/CJK text.
//
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/QD02STTParsingTests

import Testing
import Foundation
@testable import FonosApp

struct QD02STTParsingTests {

    // MARK: - OpenAI Whisper standard format

    @Test("WhisperSTT parses standard {\"text\": \"...\"} response")
    func parsesStandardWhisperResponse() throws {
        let json = #"{"text":"hello world"}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "hello world")
    }

    @Test("WhisperSTT parses response with leading/trailing whitespace in text field")
    func parsesWhisperResponseWithWhitespace() throws {
        let json = #"{"text":"  hello world  "}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        // Should trim or preserve — must not crash
        #expect(transcript.contains("hello world"))
    }

    @Test("WhisperSTT parses response where text field is an empty string")
    func parsesEmptyTranscript() throws {
        let json = #"{"text":""}"#.data(using: .utf8)!
        // Should return empty string, not throw
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "")
    }

    // MARK: - OpenAI Whisper verbose_json format

    @Test("WhisperSTT parses verbose_json format and extracts top-level text field")
    func parsesVerboseJSONFormat() throws {
        let json = """
        {
          "task": "transcribe",
          "language": "english",
          "duration": 5.23,
          "text": "This is a test transcription.",
          "segments": [
            {
              "id": 0,
              "seek": 0,
              "start": 0.0,
              "end": 5.23,
              "text": "This is a test transcription.",
              "tokens": [50364, 639, 307, 257, 1500, 22690, 13, 50524],
              "temperature": 0.0,
              "avg_logprob": -0.2,
              "compression_ratio": 1.3,
              "no_speech_prob": 0.01
            }
          ]
        }
        """.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "This is a test transcription.")
    }

    @Test("WhisperSTT verbose_json with multiple segments returns concatenated text")
    func parsesVerboseJSONMultipleSegments() throws {
        let json = """
        {
          "text": "First sentence. Second sentence.",
          "segments": [
            {"text": " First sentence.", "start": 0.0, "end": 2.0},
            {"text": " Second sentence.", "start": 2.0, "end": 4.5}
          ]
        }
        """.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript.contains("First sentence"))
        #expect(transcript.contains("Second sentence"))
    }

    // MARK: - Fonos server response format

    @Test("FonosSTT parses Fonos server response format")
    func parsesFonosServerResponse() throws {
        // Fonos server returns a slightly different JSON structure
        let json = """
        {
          "transcript": "hello from fonos server",
          "confidence": 0.97,
          "language": "en"
        }
        """.data(using: .utf8)!
        let transcript = try FonosResponseParser.parse(data: json)
        #expect(transcript == "hello from fonos server")
    }

    // MARK: - Unicode / CJK text

    @Test("WhisperSTT parses response containing CJK characters correctly")
    func parsesCJKTranscript() throws {
        let json = #"{"text":"你好世界，这是一段测试文本。"}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "你好世界，这是一段测试文本。")
    }

    @Test("WhisperSTT parses response containing Japanese text")
    func parsesJapaneseTranscript() throws {
        let json = #"{"text":"こんにちは、世界です。"}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "こんにちは、世界です。")
    }

    @Test("WhisperSTT parses response containing Arabic RTL text")
    func parsesArabicTranscript() throws {
        let json = #"{"text":"مرحبا بالعالم"}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "مرحبا بالعالم")
    }

    @Test("WhisperSTT parses response containing emoji")
    func parsesEmojiTranscript() throws {
        let json = #"{"text":"Hello world 🎉🚀"}"#.data(using: .utf8)!
        let transcript = try WhisperResponseParser.parse(data: json)
        #expect(transcript == "Hello world 🎉🚀")
    }

    // MARK: - Error cases

    @Test("WhisperResponseParser throws parseError on malformed JSON")
    func throwsOnMalformedJSON() throws {
        let badJSON = "not valid json".data(using: .utf8)!
        #expect(throws: STTError.parseError) {
            _ = try WhisperResponseParser.parse(data: badJSON)
        }
    }

    @Test("WhisperResponseParser throws parseError when text field is missing")
    func throwsWhenTextFieldMissing() throws {
        let json = #"{"result":"hello"}"#.data(using: .utf8)! // wrong key
        #expect(throws: STTError.parseError) {
            _ = try WhisperResponseParser.parse(data: json)
        }
    }

    @Test("WhisperResponseParser throws parseError on empty Data")
    func throwsOnEmptyData() throws {
        #expect(throws: STTError.parseError) {
            _ = try WhisperResponseParser.parse(data: Data())
        }
    }
}

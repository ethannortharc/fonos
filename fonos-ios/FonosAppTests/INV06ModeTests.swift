// INV-06: All built-in modes load correctly. Custom mode with user-defined prompt
// serializes/deserializes via Codable round-trip.
//
// Verifier: auto
// Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/INV06ModeTests

import Testing
import Foundation
@testable import FonosApp

struct INV06ModeTests {

    // MARK: - Built-in mode instantiation

    @Test("All 5 built-in modes are present in Mode.builtInModes")
    func builtInModeCount() throws {
        let modes = Mode.builtInModes
        #expect(modes.count == 5)
    }

    @Test("Built-in modes include raw, polish, formal, translate, custom")
    func builtInModeIDs() throws {
        let modes = Mode.builtInModes
        let ids = Set(modes.map(\.id))
        #expect(ids.contains("raw"))
        #expect(ids.contains("polish"))
        #expect(ids.contains("formal"))
        #expect(ids.contains("translate"))
        #expect(ids.contains("custom"))
    }

    @Test("Each built-in mode has a non-empty icon name")
    func builtInModeIcons() throws {
        for mode in Mode.builtInModes {
            #expect(!mode.icon.isEmpty, "Mode \(mode.id) has empty icon")
        }
    }

    // MARK: - LLM flag per mode

    @Test("Raw mode has requiresLLM == false")
    func rawModeNoLLM() throws {
        #expect(Mode.raw.requiresLLM == false)
    }

    @Test("Polish mode has requiresLLM == true")
    func polishModeRequiresLLM() throws {
        #expect(Mode.polish.requiresLLM == true)
    }

    @Test("Formal mode has requiresLLM == true")
    func formalModeRequiresLLM() throws {
        #expect(Mode.formal.requiresLLM == true)
    }

    @Test("Translate mode has requiresLLM == true")
    func translateModeRequiresLLM() throws {
        // translate with a target language
        let mode = Mode.translate(targetLanguage: "Spanish")
        #expect(mode.requiresLLM == true)
    }

    @Test("Custom mode has requiresLLM == true")
    func customModeRequiresLLM() throws {
        let mode = Mode.custom(systemPrompt: "Do something",
                               userTemplate: "{text}",
                               temperature: 0.5,
                               maxTokens: 200)
        #expect(mode.requiresLLM == true)
    }

    // MARK: - System prompt content

    @Test("Mode.systemPrompt for polish contains expected keywords")
    func polishSystemPromptKeywords() throws {
        let prompt = Mode.polish.systemPrompt
        #expect(prompt.localizedCaseInsensitiveContains("filler") ||
                prompt.localizedCaseInsensitiveContains("polish") ||
                prompt.localizedCaseInsensitiveContains("clean"))
    }

    @Test("Mode.systemPrompt for formal contains professional/business keywords")
    func formalSystemPromptKeywords() throws {
        let prompt = Mode.formal.systemPrompt
        #expect(prompt.localizedCaseInsensitiveContains("professional") ||
                prompt.localizedCaseInsensitiveContains("business") ||
                prompt.localizedCaseInsensitiveContains("formal"))
    }

    @Test("Mode.systemPrompt for translate includes target language")
    func translateSystemPromptIncludesLanguage() throws {
        let mode = Mode.translate(targetLanguage: "Japanese")
        let prompt = mode.systemPrompt
        #expect(prompt.contains("Japanese"))
    }

    @Test("Custom mode.systemPrompt returns user-provided prompt verbatim")
    func customSystemPromptVerbatim() throws {
        let myPrompt = "You are a Shakespearean writer."
        let mode = Mode.custom(systemPrompt: myPrompt,
                               userTemplate: "{text}",
                               temperature: 0.8,
                               maxTokens: 512)
        #expect(mode.systemPrompt == myPrompt)
    }

    // MARK: - User template substitution

    @Test("Mode.userTemplate substitutes {text} placeholder with input")
    func userTemplatePlaceholderSubstitution() throws {
        let mode = Mode.custom(systemPrompt: "Rewrite",
                               userTemplate: "Input: {text}",
                               temperature: 0.5,
                               maxTokens: 100)
        let result = mode.applyTemplate(to: "hello world")
        #expect(result == "Input: hello world")
        #expect(!result.contains("{text}"))
    }

    @Test("Built-in polish mode user template substitutes {text}")
    func polishUserTemplate() throws {
        let result = Mode.polish.applyTemplate(to: "um yeah so")
        #expect(result.contains("um yeah so"))
        #expect(!result.contains("{text}"))
    }

    // MARK: - Codable round-trip

    @Test("Custom mode Codable round-trip preserves all fields")
    func customModeCodableRoundTrip() throws {
        let original = Mode.custom(systemPrompt: "Be concise",
                                   userTemplate: "Rewrite: {text}",
                                   temperature: 0.3,
                                   maxTokens: 150)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Mode.self, from: data)
        #expect(decoded == original)
    }

    @Test("Raw mode Codable round-trip preserves identity")
    func rawModeCodableRoundTrip() throws {
        let original = Mode.raw
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Mode.self, from: data)
        #expect(decoded == original)
    }

    @Test("Translate mode Codable round-trip preserves target language")
    func translateModeCodableRoundTrip() throws {
        let original = Mode.translate(targetLanguage: "Korean")
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Mode.self, from: data)
        #expect(decoded == original)
    }

    @Test("Array of mixed modes Codable round-trip preserves all elements")
    func mixedModesArrayRoundTrip() throws {
        let modes: [Mode] = [
            .raw,
            .polish,
            .formal,
            .translate(targetLanguage: "French"),
            .custom(systemPrompt: "Be brief", userTemplate: "{text}", temperature: 0.5, maxTokens: 200)
        ]
        let data = try JSONEncoder().encode(modes)
        let decoded = try JSONDecoder().decode([Mode].self, from: data)
        #expect(decoded == modes)
    }
}

// NotebookPipeline: NoteContainer → (sttLanguage, llmConfig?) translation.
//
// Verifier: auto · Level: unit (pure function, no I/O)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NotebookPipelineTests

import Testing
import Foundation
@testable import FonosApp

@MainActor
struct NotebookPipelineTests {

    @Test("empty systemPrompt → llm == nil (Raw)")
    func emptyPromptRaw() {
        let nb = NoteContainer(title: "x", systemPrompt: "")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm == nil)
    }

    @Test("whitespace-only systemPrompt → llm == nil (Raw)")
    func whitespacePromptRaw() {
        let nb = NoteContainer(title: "x", systemPrompt: "   \n\t  ")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm == nil)
    }

    @Test("non-empty prompt → llm with trimmed prompt")
    func nonEmptyPrompt() {
        let nb = NoteContainer(title: "x", systemPrompt: "  Polish.  ")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.systemPrompt == "Polish.")
    }

    @Test("outputLanguage falls back to sttLanguage when nil")
    func outputLangFallsBackToStt() {
        let nb = NoteContainer(
            title: "x",
            systemPrompt: "Polish.",
            sttLanguage: "zh-CN",
            outputLanguage: nil
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.outputLanguage == "zh-CN")
    }

    @Test("explicit outputLanguage overrides sttLanguage")
    func outputLangOverride() {
        let nb = NoteContainer(
            title: "x",
            systemPrompt: "Translate.",
            sttLanguage: "zh-CN",
            outputLanguage: "en-US"
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.outputLanguage == "en-US")
    }

    @Test("sttLanguage is forwarded directly")
    func sttLangForwarded() {
        let nb = NoteContainer(title: "x", systemPrompt: "", sttLanguage: "ja-JP")
        let r = NotebookPipeline.resolve(nb)
        #expect(r.sttLanguage == "ja-JP")
    }

    @Test("llmModelOverride flows into llm config")
    func modelOverrideFlows() {
        let nb = NoteContainer(
            title: "x",
            llmModelOverride: "gpt-4o-mini",
            systemPrompt: "Polish."
        )
        let r = NotebookPipeline.resolve(nb)
        #expect(r.llm?.modelOverride == "gpt-4o-mini")
    }
}

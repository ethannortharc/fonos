// NoteINV08: Per-notebook configuration — processingMode, sttModelOverride, llmModelOverride,
// and customPrompt are stored on NoteContainer and survive a save/fetch round-trip.
//
// Verifier: auto
// Level: unit (in-memory SwiftData ModelContainer)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV08NotebookConfigTests
//
// TDD status: FAILING until NoteContainer fields and NoteService config API are implemented.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeConfigTestContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteINV08NotebookConfigTests {

    // MARK: - processingMode

    @Test("NoteContainer.processingMode defaults to 'raw' on creation")
    func processingModeDefault() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Default Mode Notebook")
        #expect(notebook.processingMode == "raw")
    }

    @Test("NoteService can set processingMode to 'light_polish'")
    func setProcessingModeLightPolish() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Polish Notebook")
        // TODO: Adjust method name/signature once NoteService config API is defined
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "light_polish",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.processingMode == "light_polish")
    }

    @Test("NoteService can set processingMode to 'summarize'")
    func setProcessingModeSummarize() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Summary Notebook")
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "summarize",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.processingMode == "summarize")
    }

    // MARK: - sttModelOverride

    @Test("NoteContainer.sttModelOverride can be set and fetched")
    func sttModelOverridePersists() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Custom STT Notebook")
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "raw",
            sttModelOverride: "whisper-large-v3",
            llmModelOverride: nil,
            customPrompt: nil
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.sttModelOverride == "whisper-large-v3")
    }

    @Test("NoteContainer.sttModelOverride can be cleared back to nil")
    func sttModelOverrideClearable() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Clear STT Override")
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "raw",
            sttModelOverride: "whisper-large-v3",
            llmModelOverride: nil,
            customPrompt: nil
        )
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "raw",
            sttModelOverride: nil, // clear it
            llmModelOverride: nil,
            customPrompt: nil
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.sttModelOverride == nil)
    }

    // MARK: - llmModelOverride

    @Test("NoteContainer.llmModelOverride can be set and fetched")
    func llmModelOverridePersists() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Custom LLM Notebook")
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "light_polish",
            sttModelOverride: nil,
            llmModelOverride: "gpt-4o-mini",
            customPrompt: nil
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.llmModelOverride == "gpt-4o-mini")
    }

    // MARK: - customPrompt

    @Test("NoteContainer.customPrompt can be set and fetched")
    func customPromptPersists() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Custom Prompt Notebook")
        let prompt = "Summarize in bullet points."
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "summarize",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: prompt
        )

        let context = modelContainer.mainContext
        let fetched = try context.fetch(FetchDescriptor<NoteContainer>()).first
        #expect(fetched?.customPrompt == prompt)
    }

    // MARK: - Full config round-trip

    @Test("All per-notebook config fields survive a save/fetch round-trip")
    func fullConfigRoundTrip() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebook = service.createNotebook(title: "Full Config Notebook")
        service.updateNotebookConfig(
            notebook.id,
            processingMode: "summarize",
            sttModelOverride: "fonos-whisper",
            llmModelOverride: "claude-3-haiku",
            customPrompt: "Be very brief."
        )

        let context = modelContainer.mainContext
        try context.save()

        let fetched = try context.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == notebook.id })
        #expect(fetched?.processingMode == "summarize")
        #expect(fetched?.sttModelOverride == "fonos-whisper")
        #expect(fetched?.llmModelOverride == "claude-3-haiku")
        #expect(fetched?.customPrompt == "Be very brief.")
    }

    // MARK: - Config independence between notebooks

    @Test("Per-notebook config is independent — changing one notebook does not affect another")
    func configIndependence() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)

        let notebookA = service.createNotebook(title: "Notebook A")
        let notebookB = service.createNotebook(title: "Notebook B")

        service.updateNotebookConfig(
            notebookA.id,
            processingMode: "summarize",
            sttModelOverride: nil,
            llmModelOverride: nil,
            customPrompt: nil
        )

        // Notebook B should still have default processingMode
        let context = modelContainer.mainContext
        let all = try context.fetch(FetchDescriptor<NoteContainer>())
        let fetchedB = all.first(where: { $0.id == notebookB.id })
        #expect(fetchedB?.processingMode == "raw")
    }

    // MARK: - v2 fields

    @Test("New v2 fields default to expected values")
    func newFieldDefaults() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)
        let nb = service.createNotebook(title: "Defaults")
        #expect(nb.systemPrompt == "")
        #expect(nb.sttLanguage == nil)
        #expect(nb.outputLanguage == nil)
        #expect(nb.showRawInline == false)
        #expect(nb.siriPhrase == nil)
    }

    @Test("v2 fields persist via updateNotebookConfigV2")
    func v2FieldsPersist() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)
        let nb = service.createNotebook(title: "Persist")
        service.updateNotebookConfigV2(
            nb.id,
            systemPrompt: "Be terse.",
            sttLanguage: .some("zh-CN"),
            outputLanguage: .some("en-US"),
            sttModelOverride: nil,
            llmModelOverride: nil,
            showRawInline: true,
            siriPhrase: .some("Note to Persist")
        )
        let fetched = try modelContainer.mainContext
            .fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "Be terse.")
        #expect(fetched?.sttLanguage == "zh-CN")
        #expect(fetched?.outputLanguage == "en-US")
        #expect(fetched?.showRawInline == true)
        #expect(fetched?.siriPhrase == "Note to Persist")
    }

    @Test("updateNotebookConfigV2 leaves unspecified fields untouched")
    func v2PartialUpdate() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)
        let nb = service.createNotebook(title: "Partial")
        service.updateNotebookConfigV2(nb.id, systemPrompt: "First.")
        service.updateNotebookConfigV2(nb.id, sttLanguage: .some("ja-JP"))

        let fetched = try modelContainer.mainContext
            .fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "First.") // preserved
        #expect(fetched?.sttLanguage == "ja-JP")    // updated
    }

    @Test("updateNotebookConfigV2 can clear sttLanguage with .some(nil)")
    func v2ClearLanguage() throws {
        let modelContainer = try makeConfigTestContainer()
        let service = NoteService(modelContainer: modelContainer)
        let nb = service.createNotebook(title: "Clear")
        service.updateNotebookConfigV2(nb.id, sttLanguage: .some("zh-CN"))
        service.updateNotebookConfigV2(nb.id, sttLanguage: .some(nil))

        let fetched = try modelContainer.mainContext
            .fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.sttLanguage == nil)
    }
}

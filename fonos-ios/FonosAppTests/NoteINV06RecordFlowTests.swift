// NoteINV06: Record flow pipeline — AudioCaptureService → STTService → optional LLMService
// → NoteService.addEntry() — produces a NoteEntry with correct text and containerId.
//
// Verifier: auto
// Level: unit (mocked STT, LLM, and NoteService via protocol injection)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV06RecordFlowTests
//
// TDD status: FAILING until NoteViewModel (or equivalent orchestrator) is created.

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - Mock STT provider

/// Immediately returns a canned transcript without touching the microphone.
private final class MockSTTProvider: STTProvider, @unchecked Sendable {
    var transcript: String
    var shouldThrow: Bool

    init(transcript: String = "mock transcribed text", shouldThrow: Bool = false) {
        self.transcript = transcript
        self.shouldThrow = shouldThrow
    }

    func transcribe(audioData: Data, language: String?) async throws -> String {
        if shouldThrow { throw STTError.transcriptionFailed }
        return transcript
    }
}

// MARK: - Mock LLM provider

/// Immediately returns a canned processed string without hitting a network.
private final class MockLLMProvider: NoteLLMProvider, @unchecked Sendable {
    var processedText: String
    var callCount: Int = 0
    var shouldThrow: Bool

    init(processedText: String = "polished mock text", shouldThrow: Bool = false) {
        self.processedText = processedText
        self.shouldThrow = shouldThrow
    }

    func process(text: String, prompt: String?) async throws -> String {
        callCount += 1
        if shouldThrow { throw LLMError.requestFailed }
        return processedText
    }
}

// MARK: - In-memory container helper

@MainActor
private func makeRecordFlowContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Tests

@MainActor
struct NoteINV06RecordFlowTests {

    // MARK: - Happy path: raw mode

    @Test("Record flow with raw mode creates NoteEntry with STT transcript as rawText")
    func recordFlowRawMode() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "Test Notebook")

        let sttMock = MockSTTProvider(transcript: "hello world from mic")
        // TODO: Replace NoteViewModel initialiser args once its signature is finalised
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: nil // raw mode — no LLM
        )

        // Simulate the full record → transcribe → store pipeline
        await viewModel.recordAndStore(
            to: notebook.id,
            mode: "raw",
            audioData: Data(repeating: 0, count: 64)
        )

        let entries = noteService.entriesForNotebook(notebook.id)
        #expect(entries.count == 1)
        #expect(entries.first?.rawText == "hello world from mic")
        #expect(entries.first?.containerId == notebook.id)
        #expect(entries.first?.mode == "raw")
        // Raw mode: processedText should be nil or equal to rawText
        let processedText = entries.first?.processedText
        #expect(processedText == nil || processedText == "hello world from mic")
    }

    // MARK: - Happy path: light_polish mode

    @Test("Record flow with non-empty systemPrompt stores both raw and processed text")
    func recordFlowLightPolishMode() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "Polish Notebook")
        noteService.updateNotebookConfigV2(notebook.id, systemPrompt: "Polish.")

        let sttMock = MockSTTProvider(transcript: "um yeah so i think we need to ship")
        let llmMock = MockLLMProvider(processedText: "We need to ship.")
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: llmMock
        )

        await viewModel.recordAndStore(
            to: notebook.id,
            audioData: Data(repeating: 0, count: 64)
        )

        let entries = noteService.entriesForNotebook(notebook.id)
        #expect(entries.count == 1)
        #expect(entries.first?.rawText == "um yeah so i think we need to ship")
        #expect(entries.first?.processedText == "We need to ship.")
        #expect(entries.first?.mode == "llm")
    }

    // MARK: - LLM is called exactly once per recording

    @Test("Record flow calls LLM provider exactly once when systemPrompt is non-empty")
    func llmCalledOnce() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "Call Count Test")
        noteService.updateNotebookConfigV2(notebook.id, systemPrompt: "Summarize.")

        let sttMock = MockSTTProvider(transcript: "test input")
        let llmMock = MockLLMProvider(processedText: "test output")
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: llmMock
        )

        await viewModel.recordAndStore(
            to: notebook.id,
            audioData: Data(repeating: 0, count: 64)
        )

        #expect(llmMock.callCount == 1)
    }

    // MARK: - STT failure: no entry created

    @Test("Record flow does not create NoteEntry when STT throws")
    func recordFlowSTTFailure() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "STT Failure Notebook")

        let sttMock = MockSTTProvider(shouldThrow: true)
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: nil
        )

        await viewModel.recordAndStore(
            to: notebook.id,
            mode: "raw",
            audioData: Data(repeating: 0, count: 64)
        )

        let entries = noteService.entriesForNotebook(notebook.id)
        // No entry should be created when STT fails
        #expect(entries.isEmpty)
    }

    // MARK: - LLM failure: falls back to raw transcript

    @Test("Record flow falls back to raw transcript when LLM throws")
    func recordFlowLLMFallback() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "LLM Fallback Notebook")
        noteService.updateNotebookConfigV2(notebook.id, systemPrompt: "Polish.")

        let sttMock = MockSTTProvider(transcript: "raw fallback text")
        let llmMock = MockLLMProvider(shouldThrow: true)
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: llmMock
        )

        await viewModel.recordAndStore(
            to: notebook.id,
            audioData: Data(repeating: 0, count: 64)
        )

        let entries = noteService.entriesForNotebook(notebook.id)
        // Entry must still be created using raw transcript as fallback
        #expect(entries.count == 1)
        #expect(entries.first?.rawText == "raw fallback text")
    }

    // MARK: - v2 pipeline coverage

    /// Captures the language hint passed to STT so tests can assert it propagates.
    private final class CapturingSTT: STTProvider, @unchecked Sendable {
        var lastLanguage: String?
        var stub: String = "raw transcript"
        func transcribe(audioData: Data, language: String?) async throws -> String {
            lastLanguage = language
            return stub
        }
    }

    @Test("recordAndStore forwards notebook.sttLanguage to STT")
    func sttLanguageForwarded() async throws {
        let mc = try makeRecordFlowContainer()
        let service = NoteService(modelContainer: mc)
        let nb = service.createNotebook(title: "ZH")
        service.updateNotebookConfigV2(nb.id, sttLanguage: .some("zh-CN"))

        let stt = CapturingSTT()
        let vm = NoteViewModel(noteService: service, sttProvider: stt, llmProvider: nil)
        await vm.recordAndStore(to: nb.id, audioData: Data())

        #expect(stt.lastLanguage == "zh-CN")
    }

    @Test("recordAndStore skips LLM when systemPrompt is empty (Raw notebook)")
    func emptyPromptSkipsLLM() async throws {
        let mc = try makeRecordFlowContainer()
        let service = NoteService(modelContainer: mc)
        let nb = service.createNotebook(title: "Raw")
        // systemPrompt defaults to ""

        let llm = MockLLMProvider(processedText: "should not be called")
        let vm = NoteViewModel(noteService: service, sttProvider: CapturingSTT(), llmProvider: llm)
        await vm.recordAndStore(to: nb.id, audioData: Data())

        #expect(llm.callCount == 0)
    }

    @Test("recordAndStore invokes LLM when systemPrompt is non-empty")
    func nonEmptyPromptInvokesLLM() async throws {
        let mc = try makeRecordFlowContainer()
        let service = NoteService(modelContainer: mc)
        let nb = service.createNotebook(title: "Polish")
        service.updateNotebookConfigV2(nb.id, systemPrompt: "Polish.")

        let llm = MockLLMProvider(processedText: "polished")
        let vm = NoteViewModel(noteService: service, sttProvider: CapturingSTT(), llmProvider: llm)
        await vm.recordAndStore(to: nb.id, audioData: Data())

        #expect(llm.callCount == 1)
    }

    // MARK: - containerId propagation

    @Test("NoteEntry.containerId matches the notebook the recording was made into")
    func containerIdPropagated() async throws {
        let modelContainer = try makeRecordFlowContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebookA = noteService.createNotebook(title: "Notebook A")
        let notebookB = noteService.createNotebook(title: "Notebook B")

        let sttMock = MockSTTProvider(transcript: "belongs to A")
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: nil
        )

        await viewModel.recordAndStore(
            to: notebookA.id,
            mode: "raw",
            audioData: Data(repeating: 0, count: 64)
        )

        let aEntries = noteService.entriesForNotebook(notebookA.id)
        let bEntries = noteService.entriesForNotebook(notebookB.id)

        #expect(aEntries.count == 1)
        #expect(aEntries.first?.containerId == notebookA.id)
        #expect(bEntries.isEmpty)
    }
}

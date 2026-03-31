// NoteINV11: Audio is NOT saved — NoteEntry stores only text.
// No audioData or audioURL property on NoteEntry; record flow discards audio after STT.
//
// Verifier: auto
// Level: unit (reflection / Mirror + record flow verification)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV11NoAudioTests
//
// TDD status: FAILING until NoteEntry.swift is created (compile-time checks).

import Testing
import SwiftData
import Foundation
@testable import FonosApp

// MARK: - In-memory container helper

@MainActor
private func makeNoAudioContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

// MARK: - Mock providers (reuse pattern from INV06)

private final class NoAudioMockSTT: STTProvider, @unchecked Sendable {
    var transcribedAudioData: Data? = nil
    var transcript = "no audio stored"

    func transcribe(audioData: Data, language: String?) async throws -> String {
        transcribedAudioData = audioData // capture so we can verify it was passed
        return transcript
    }
}

// MARK: - Tests

@MainActor
struct NoteINV11NoAudioTests {

    // MARK: - Level 1: Schema — NoteEntry has no audioData property

    @Test("NoteEntry does not have an audioData property")
    func noteEntryHasNoAudioData() throws {
        let modelContainer = try makeNoAudioContainer()
        let context = modelContainer.mainContext

        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "check for audio property",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)

        // Use Mirror to inspect stored properties at runtime.
        // If audioData exists, the mirror will contain it and this test should fail.
        let mirror = Mirror(reflecting: entry)
        let propertyNames = mirror.children.compactMap { $0.label }
        #expect(!propertyNames.contains("audioData"),
                "NoteEntry must not have an 'audioData' property — notes are text-only")
    }

    @Test("NoteEntry does not have an audioURL property")
    func noteEntryHasNoAudioURL() throws {
        let modelContainer = try makeNoAudioContainer()
        let context = modelContainer.mainContext

        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "check for audioURL property",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)

        let mirror = Mirror(reflecting: entry)
        let propertyNames = mirror.children.compactMap { $0.label }
        #expect(!propertyNames.contains("audioURL"),
                "NoteEntry must not have an 'audioURL' property — no audio file references")
    }

    @Test("NoteEntry does not have an audioFilePath property")
    func noteEntryHasNoAudioFilePath() throws {
        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "check for file path",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )

        let mirror = Mirror(reflecting: entry)
        let propertyNames = mirror.children.compactMap { $0.label }
        #expect(!propertyNames.contains("audioFilePath"))
    }

    // MARK: - Level 2: Record flow discards audio after STT

    @Test("Record flow does not persist audio data anywhere after STT completes")
    func recordFlowDiscardsAudio() async throws {
        let modelContainer = try makeNoAudioContainer()
        let noteService = NoteService(modelContainer: modelContainer)
        let notebook = noteService.createNotebook(title: "No Audio Test Notebook")

        let sttMock = NoAudioMockSTT()
        let viewModel = NoteViewModel(
            noteService: noteService,
            sttProvider: sttMock,
            llmProvider: nil
        )

        let originalAudioData = Data(repeating: 0xFF, count: 128)
        await viewModel.recordAndStore(
            to: notebook.id,
            mode: "raw",
            audioData: originalAudioData
        )

        // STT was called (audio was passed through for transcription)
        #expect(sttMock.transcribedAudioData != nil)

        // The resulting NoteEntry must not hold any reference to the audio
        let entries = noteService.entriesForNotebook(notebook.id)
        #expect(entries.count == 1)
        if let entry = entries.first {
            let mirror = Mirror(reflecting: entry)
            let propertyNames = mirror.children.compactMap { $0.label }
            #expect(!propertyNames.contains("audioData"))
            #expect(!propertyNames.contains("audioURL"))
        }
    }

    @Test("NoteEntry in SwiftData does not encode audio bytes in its stored representation")
    func noteEntrySwiftDataRowContainsNoAudio() throws {
        let modelContainer = try makeNoAudioContainer()
        let context = modelContainer.mainContext

        let entry = NoteEntry(
            id: UUID(),
            createdAt: Date(),
            sourceType: "note",
            rawText: "text only entry",
            processedText: nil,
            containerId: UUID(),
            mode: "raw",
            durationMs: nil,
            language: nil
        )
        context.insert(entry)
        try context.save()

        // Fetch back and confirm no audio-related keys in a JSON snapshot
        let fetched = try context.fetch(FetchDescriptor<NoteEntry>()).first
        guard let fetched else {
            Issue.record("No NoteEntry found after save")
            return
        }
        let mirror = Mirror(reflecting: fetched)
        let labels = Set(mirror.children.compactMap { $0.label })
        let audioRelatedKeys: Set<String> = ["audioData", "audioURL", "audioFilePath",
                                              "audioBytes", "recordingURL", "wavData"]
        let intersection = labels.intersection(audioRelatedKeys)
        #expect(intersection.isEmpty,
                "NoteEntry has unexpected audio-related properties: \(intersection)")
    }
}

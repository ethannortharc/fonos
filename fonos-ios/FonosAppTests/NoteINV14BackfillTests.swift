// NoteINV14: One-shot v1→v2 backfill of processingMode/customPrompt into systemPrompt.
//
// Verifier: auto · Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV14BackfillTests

import Testing
import SwiftData
import Foundation
@testable import FonosApp

@MainActor
private func makeContainer() throws -> ModelContainer {
    let schema = Schema([NoteContainer.self, NoteEntry.self])
    let config = ModelConfiguration(isStoredInMemoryOnly: true)
    return try ModelContainer(for: schema, configurations: [config])
}

@MainActor
struct NoteINV14BackfillTests {

    private func freshFlagKey() -> String {
        "notebookConfig.migrated.v2.test.\(UUID().uuidString)"
    }

    @Test("backfill maps processingMode='polish' to Polish template seed")
    func backfillPolish() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.polish.systemPromptSeed)
    }

    @Test("backfill maps processingMode='light_polish' to Polish template seed")
    func backfillLightPolish() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "light_polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.polish.systemPromptSeed)
    }

    @Test("backfill maps processingMode='summarize' to Meeting Notes seed")
    func backfillSummarize() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "summarize")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == NotebookTemplate.meetingNotes.systemPromptSeed)
    }

    @Test("backfill maps customPrompt verbatim when processingMode is unknown")
    func backfillCustomPrompt() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "weirdo", customPrompt: "Make haiku.")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "Make haiku.")
    }

    @Test("backfill leaves systemPrompt='' when processingMode='raw'")
    func backfillRaw() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "raw")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "")
    }

    @Test("backfill is idempotent — second run does not overwrite user edits")
    func backfillIdempotent() throws {
        let key = freshFlagKey()
        let mc = try makeContainer()
        let ctx = mc.mainContext
        let nb = NoteContainer(title: "Old", processingMode: "polish")
        ctx.insert(nb)
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        nb.systemPrompt = "Edited by user."
        try ctx.save()

        NoteService.runBackfill(modelContainer: mc, flagKey: key)

        let fetched = try ctx.fetch(FetchDescriptor<NoteContainer>())
            .first(where: { $0.id == nb.id })
        #expect(fetched?.systemPrompt == "Edited by user.")
    }
}

// SharedNotebookCatalog: JSON read/write of (id, title) pairs into a shared
// App Group container. Tests use a temp directory in lieu of App Group.
//
// Verifier: auto · Level: unit
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/SharedNotebookCatalogTests

import Testing
import Foundation
@testable import FonosApp

@MainActor
struct SharedNotebookCatalogTests {

    private func tempURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("json")
    }

    @Test("write then read returns the same entries")
    func roundTrip() throws {
        let url = tempURL()
        let entries = [
            SharedNotebookCatalog.Entry(id: UUID().uuidString, title: "Work"),
            SharedNotebookCatalog.Entry(id: UUID().uuidString, title: "Personal")
        ]
        try SharedNotebookCatalog.write(entries, to: url)

        let read = SharedNotebookCatalog.read(from: url)
        #expect(read.count == 2)
        #expect(read.map(\.title).sorted() == ["Personal", "Work"])
    }

    @Test("read on missing file returns empty array")
    func readMissingReturnsEmpty() {
        let url = tempURL()
        #expect(SharedNotebookCatalog.read(from: url).isEmpty)
    }

    @Test("write replaces existing file content")
    func writeReplaces() throws {
        let url = tempURL()
        try SharedNotebookCatalog.write([
            SharedNotebookCatalog.Entry(id: "1", title: "A")
        ], to: url)
        try SharedNotebookCatalog.write([
            SharedNotebookCatalog.Entry(id: "2", title: "B")
        ], to: url)

        let read = SharedNotebookCatalog.read(from: url)
        #expect(read.count == 1)
        #expect(read[0].title == "B")
    }
}

// NoteINV10: Deep link fonos://note and fonos://note?notebook=<id> are handled by FonosApp,
// posting the correct notification so the Notes tab and optional notebook are opened.
//
// Verifier: auto
// Level: unit (notification observation, no simulator needed)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV10DeepLinkTests
//
// TDD status: FAILING until deep link handling for fonos://note is added to FonosApp/ContentView.

import Testing
import Foundation
@testable import FonosApp

// MARK: - Tests

@MainActor
struct NoteINV10DeepLinkTests {

    // MARK: - fonos://note base URL

    @Test("handleDeepLink with fonos://note posts a notification")
    func deepLinkNotePostsNotification() async throws {
        // Register an observer before triggering the deep link
        let expectation = NotificationExpectation(name: .fonosOpenNotes)
        await withCheckedContinuation { continuation in
            expectation.onFire = { continuation.resume() }
            let url = URL(string: "fonos://note")!
            // TODO: Replace with actual deep link handler entry point once it is known.
            // Candidates: FonosApp.handleOpenURL(_:), AppDelegate.application(_:open:),
            // or a static DeepLinkRouter.handle(_:) method.
            DeepLinkRouter.handle(url)
        }
        #expect(expectation.fired, "Expected .fonosOpenNotes notification to be posted")
    }

    @Test("handleDeepLink with fonos://note posts notification with no notebookId userInfo")
    func deepLinkNoteNoNotebookId() throws {
        var receivedUserInfo: [AnyHashable: Any]? = nil
        let token = NotificationCenter.default.addObserver(
            forName: .fonosOpenNotes,
            object: nil,
            queue: .main
        ) { notification in
            receivedUserInfo = notification.userInfo
        }
        defer { NotificationCenter.default.removeObserver(token) }

        let url = URL(string: "fonos://note")!
        DeepLinkRouter.handle(url)

        // No notebookId query parameter → userInfo should not contain one
        let notebookId = receivedUserInfo?["notebookId"] as? String
        #expect(notebookId == nil)
    }

    // MARK: - fonos://note?notebook=<id>

    @Test("handleDeepLink with fonos://note?notebook=<uuid> posts notification with correct notebookId")
    func deepLinkNoteWithNotebookId() throws {
        let targetId = UUID()
        var receivedNotebookId: UUID? = nil
        let token = NotificationCenter.default.addObserver(
            forName: .fonosOpenNotes,
            object: nil,
            queue: .main
        ) { notification in
            if let idString = notification.userInfo?["notebookId"] as? String {
                receivedNotebookId = UUID(uuidString: idString)
            }
        }
        defer { NotificationCenter.default.removeObserver(token) }

        let url = URL(string: "fonos://note?notebook=\(targetId.uuidString)")!
        DeepLinkRouter.handle(url)

        #expect(receivedNotebookId == targetId)
    }

    @Test("handleDeepLink with malformed UUID in notebook param does not crash")
    func deepLinkNoteWithMalformedUUID() throws {
        let url = URL(string: "fonos://note?notebook=not-a-valid-uuid")!
        // Must not throw or crash
        DeepLinkRouter.handle(url)
        #expect(Bool(true)) // sentinel — reaching here means no crash
    }

    // MARK: - Unrelated URL is not handled as a note deep link

    @Test("handleDeepLink with fonos://record does not post .fonosOpenNotes notification")
    func deepLinkRecordDoesNotPostNotesNotification() throws {
        var fired = false
        let token = NotificationCenter.default.addObserver(
            forName: .fonosOpenNotes,
            object: nil,
            queue: .main
        ) { _ in fired = true }
        defer { NotificationCenter.default.removeObserver(token) }

        let url = URL(string: "fonos://record")!
        DeepLinkRouter.handle(url)

        #expect(!fired, "fonos://record must not trigger the Notes deep link handler")
    }

    @Test("handleDeepLink with unrecognised scheme does not crash")
    func deepLinkUnrecognisedScheme() throws {
        let url = URL(string: "https://example.com/note")!
        DeepLinkRouter.handle(url)
        #expect(Bool(true))
    }
}

// MARK: - Notification name extension

private extension Notification.Name {
    // TODO: Move this extension to the production code (e.g. Notifications.swift)
    // and import it here via @testable import FonosApp.
    static let fonosOpenNotes = Notification.Name("fonos.openNotes")
}

// MARK: - NotificationExpectation helper

/// A lightweight observer that records whether a notification fired.
private final class NotificationExpectation {
    let name: Notification.Name
    var fired: Bool = false
    var onFire: (() -> Void)?
    private var token: NSObjectProtocol?

    init(name: Notification.Name) {
        self.name = name
        token = NotificationCenter.default.addObserver(
            forName: name,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.fired = true
            self?.onFire?()
        }
    }

    deinit {
        if let token { NotificationCenter.default.removeObserver(token) }
    }
}

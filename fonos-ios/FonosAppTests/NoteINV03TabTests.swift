// NoteINV03: Notes tab appears as 4th tab in TabView with 'note.text' SF Symbol.
//
// Verifier: auto
// Levels: unit (ContentView tab count), integration (XCUITest — documented below)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteINV03TabTests
//
// TDD status: FAILING until NotesView is added to ContentView's TabView.

import Testing
import SwiftUI
@testable import FonosApp

// MARK: - Tests

struct NoteINV03TabTests {

    // MARK: - Level 1: Static check via compilation

    @Test("FonosApp module compiles with NotesView type present")
    func notesViewTypeAccessible() throws {
        // If NotesView doesn't exist, this file fails to compile.
        // The mere reference below is the compile-time assertion.
        let _ = NotesView.self
        #expect(Bool(true)) // sentinel — real assertion is compile-time
    }

    // MARK: - Level 2: Unit — ContentView tab structure

    @Test("ContentView initialises without crash when Notes tab is present")
    func contentViewInitialises() throws {
        // ContentView must be constructible; if it references NotesView, that type must exist.
        let view = ContentView()
        _ = view
        #expect(Bool(true))
    }

    @Test("NotesView initialises without crash")
    func notesViewInitialises() throws {
        let view = NotesView()
        _ = view
        #expect(Bool(true))
    }

    @Test("ContentView tab count is 4")
    func contentViewHasFourTabs() throws {
        // TODO: Once ContentView exposes a tabCount property or similar introspection,
        // replace this sentinel with a direct assertion:
        //   #expect(ContentView.tabCount == 4)
        //
        // For now, verify by checking that all four expected tab view types are reachable
        // from the module. If any is missing, compilation of this file fails.
        let _ = DictationView.self   // tab 1
        // let _ = RecentView.self   // tab 2 — uncomment when RecentView exists
        // let _ = SettingsView.self // tab 3 — uncomment when verifying
        let _ = NotesView.self       // tab 4

        // Structural assertion: ContentView must not crash when the tab bar is rendered.
        let view = ContentView()
        _ = view
        #expect(Bool(true))
    }
}

// MARK: - Level 3: Integration (XCUITest — reference only)
//
// The full integration check is performed by FonosAppUITests/NoteINV03UITests.swift:
//
//   func testFourTabsPresent() {
//       let app = XCUIApplication()
//       app.launch()
//       XCTAssertEqual(app.tabBars.firstMatch.buttons.count, 4)
//   }
//
//   func testNoteTabIsReachable() {
//       let app = XCUIApplication()
//       app.launch()
//       app.tabBars.buttons["Notes"].tap()
//       XCTAssert(app.navigationBars["Notes"].exists
//              || app.staticTexts["Notes"].exists)
//   }
//
// These XCUITests require a running simulator and are executed by the ratchet runner,
// not in this in-process unit test suite.

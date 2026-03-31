import XCTest

final class FonosAppUITests: XCTestCase {

    var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launch()
    }

    override func tearDownWithError() throws {
        app = nil
    }

    func testAppLaunches() throws {
        // Verify the app launches and the root window is present
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 10))
    }

    func testTabBarExists() throws {
        // Verify a tab bar is present in the launched app
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.waitForExistence(timeout: 5), "Expected a tab bar to exist")
    }
}

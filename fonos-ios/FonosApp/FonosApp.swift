import SwiftUI
import SwiftData

@main
struct FonosApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .modelContainer(for: DictationSession.self)
        }
    }
}

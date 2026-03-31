import SwiftUI
import SwiftData

@main
struct FonosApp: App {

    let modelContainer: ModelContainer

    init() {
        // Register background recording service for keyboard extension IPC
        BackgroundRecordingService.shared.register()
        do {
            modelContainer = try ModelContainer(
                for: DictationSession.self, NoteContainer.self, NoteEntry.self
            )
        } catch {
            fatalError("Failed to create ModelContainer: \(error)")
        }
    }

    var body: some Scene {
        WindowGroup {
            ContentView(modelContainer: modelContainer)
                .modelContainer(modelContainer)
                .onOpenURL { url in
                    handleDeepLink(url)
                }
        }
    }

    // MARK: - Deep Link Handling

    /// Handles incoming URLs, e.g. `fonos://record` from the Home Screen widget or Siri Shortcuts.
    private func handleDeepLink(_ url: URL) {
        DeepLinkRouter.handle(url)
    }
}

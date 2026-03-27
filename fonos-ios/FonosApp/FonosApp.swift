import SwiftUI
import SwiftData

@main
struct FonosApp: App {

    init() {
        // Register background recording service for keyboard extension IPC
        BackgroundRecordingService.shared.register()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .modelContainer(for: DictationSession.self)
                .onOpenURL { url in
                    handleDeepLink(url)
                }
        }
    }

    // MARK: - Deep Link Handling

    /// Handles incoming URLs, e.g. `fonos://record` from the Home Screen widget or Siri Shortcuts.
    private func handleDeepLink(_ url: URL) {
        guard url.scheme == "fonos" else { return }

        switch url.host {
        case "record":
            // Parse optional query parameters
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
            let mode = components?.queryItems?.first(where: { $0.name == "mode" })?.value
            let destination = components?.queryItems?.first(where: { $0.name == "destination" })?.value

            // Post notification so DictationViewModel can start recording immediately
            NotificationCenter.default.post(
                name: .fonosStartRecording,
                object: nil,
                userInfo: [
                    "mode": mode as Any,
                    "destination": destination as Any
                ]
            )

        default:
            break
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    /// Posted when the app receives a `fonos://record` deep link.
    static let fonosStartRecording = Notification.Name("com.fonos.ios.startRecording")
}

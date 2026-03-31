import Foundation

// MARK: - Notification Names

extension Notification.Name {
    /// Posted when the app receives a `fonos://record` deep link.
    static let fonosStartRecording = Notification.Name("com.fonos.ios.startRecording")

    /// Posted when the app receives a `fonos://note` deep link.
    static let fonosOpenNotes = Notification.Name("fonos.openNotes")
}

// MARK: - DeepLinkRouter

/// Routes incoming `fonos://` deep links to the appropriate notification.
enum DeepLinkRouter {

    /// Handles an incoming URL, posting the appropriate notification for recognised schemes.
    static func handle(_ url: URL) {
        guard url.scheme == "fonos" else { return }

        let components = URLComponents(url: url, resolvingAgainstBaseURL: false)

        switch url.host {
        case "record":
            let mode = components?.queryItems?.first(where: { $0.name == "mode" })?.value
            let destination = components?.queryItems?.first(where: { $0.name == "destination" })?.value
            NotificationCenter.default.post(
                name: .fonosStartRecording,
                object: nil,
                userInfo: [
                    "mode": mode as Any,
                    "destination": destination as Any
                ]
            )

        case "note":
            let notebookId = components?.queryItems?.first(where: { $0.name == "notebook" })?.value
            NotificationCenter.default.post(
                name: .fonosOpenNotes,
                object: nil,
                userInfo: ["notebookId": notebookId as Any]
            )

        default:
            break
        }
    }
}

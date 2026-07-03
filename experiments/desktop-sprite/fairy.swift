// 3D flower fairy — a full-screen, click-through, transparent overlay in
// which a WebGL fairy (see fairy.html) flies freely across the entire
// display. Standalone demo for fonos issue #25.
//
// Build & run:  ./run-fairy.sh      Quit:  pkill -f fairy-sprite
//
// The window ignores all mouse events, so it never blocks normal work —
// the fairy simply lives above your desktop.

import Cocoa
import WebKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var webView: WKWebView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard CommandLine.arguments.count >= 2 else {
            fputs("usage: fairy-sprite <path-to-fairy.html>\n", stderr)
            NSApp.terminate(nil)
            return
        }
        let htmlPath = CommandLine.arguments[1]
        let screen = NSScreen.main!.frame

        window = NSWindow(contentRect: screen, styleMask: [.borderless],
                          backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.level = .floating
        window.ignoresMouseEvents = true // click-through: never blocks work
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]

        webView = WKWebView(frame: NSRect(origin: .zero, size: screen.size))
        webView.setValue(false, forKey: "drawsBackground")
        let url = URL(fileURLWithPath: htmlPath)
        webView.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
        window.contentView = webView

        window.makeKeyAndOrderFront(nil)
        NSApp.setActivationPolicy(.accessory)
        print("WID \(window.windowNumber)")
        fflush(stdout)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()

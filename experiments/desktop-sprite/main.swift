// Desktop sprite simulator — a cute panda that wanders the bottom of the
// screen in a transparent, always-on-top window. Standalone demo for the
// companion-sprite concept (fonos issue #25); no fonos backend involved.
//
// Build & run:  ./run.sh      Quit: right-click the panda (or Ctrl+C).
//
// The character/animation layer lives in panda.html (pure SVG/CSS/JS); this
// shell owns the window and the behavior brain (wander/nibble/wave schedule),
// mirroring how the real sprite would be driven by pipeline events.

import Cocoa
import WebKit

let SPRITE_W: CGFloat = 220
let SPRITE_H: CGFloat = 230
let ROLL_SPEED: CGFloat = 170 // px/sec along the ground

final class SpriteWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}

final class AppDelegate: NSObject, NSApplicationDelegate, WKScriptMessageHandler {
    var window: SpriteWindow!
    var webView: WKWebView!
    var moveTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard CommandLine.arguments.count >= 2 else {
            fputs("usage: panda-sprite <path-to-panda.html>\n", stderr)
            NSApp.terminate(nil)
            return
        }
        let htmlPath = CommandLine.arguments[1]

        let screen = NSScreen.main!.visibleFrame
        let startX = screen.midX - SPRITE_W / 2
        let groundY = screen.minY

        window = SpriteWindow(
            contentRect: NSRect(x: startX, y: groundY, width: SPRITE_W, height: SPRITE_H),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let config = WKWebViewConfiguration()
        config.userContentController.add(self, name: "quit")
        webView = WKWebView(frame: NSRect(x: 0, y: 0, width: SPRITE_W, height: SPRITE_H),
                            configuration: config)
        webView.setValue(false, forKey: "drawsBackground")
        let url = URL(fileURLWithPath: htmlPath)
        webView.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
        window.contentView = webView

        window.makeKeyAndOrderFront(nil)
        NSApp.setActivationPolicy(.accessory)
        print("WID \(window.windowNumber)")
        print("FRAME \(Int(window.frame.minX)) \(Int(window.frame.minY)) \(Int(SPRITE_W)) \(Int(SPRITE_H))")
        fflush(stdout)

        scheduleNextAction(after: 2.0)
    }

    func userContentController(_ userContentController: WKUserContentController,
                               didReceive message: WKScriptMessage) {
        if message.name == "quit" { NSApp.terminate(nil) }
    }

    // ── behavior brain ───────────────────────────────────────────────────────

    func scheduleNextAction(after: TimeInterval? = nil) {
        let delay = after ?? Double.random(in: 3.0...7.5)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            self?.performRandomAction()
        }
    }

    func performRandomAction() {
        let roll = Double.random(in: 0..<1)
        if roll < 0.5 {
            wander()
        } else if roll < 0.75 {
            js("doNibble()")
            scheduleNextAction()
        } else {
            js("doWave()")
            scheduleNextAction()
        }
    }

    /// Curl into a ball and roll to a random spot along the screen bottom.
    func wander() {
        guard let screen = NSScreen.main?.visibleFrame else { return scheduleNextAction() }
        let minX = screen.minX + 4
        let maxX = screen.maxX - SPRITE_W - 4
        let targetX = CGFloat.random(in: minX...maxX)
        let fromX = window.frame.minX
        let distance = targetX - fromX
        guard abs(distance) > 60 else { return scheduleNextAction(after: 1.0) }

        let duration = TimeInterval(abs(distance) / ROLL_SPEED)
        let signedSpeed = ROLL_SPEED * (distance > 0 ? 1 : -1)
        js("setState('roll', \(signedSpeed))")

        let start = Date()
        moveTimer?.invalidate()
        moveTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] t in
            guard let self else { return t.invalidate() }
            let p = min(Date().timeIntervalSince(start) / duration, 1.0)
            // ease-in-out sine
            let eased = 0.5 - 0.5 * cos(p * .pi)
            let x = fromX + distance * CGFloat(eased)
            self.window.setFrameOrigin(NSPoint(x: x, y: screen.minY))
            if p >= 1.0 {
                t.invalidate()
                self.js("setState('sit')")
                print("FRAME \(Int(x)) \(Int(screen.minY)) \(Int(SPRITE_W)) \(Int(SPRITE_H))")
                fflush(stdout)
                self.scheduleNextAction()
            }
        }
    }

    func js(_ script: String) {
        webView.evaluateJavaScript(script, completionHandler: nil)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()

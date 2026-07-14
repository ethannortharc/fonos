//! Main-window raise/activate helper, shared by `main.rs`'s own call sites
//! (Dock reopen, float pill "show-main-window") and `tray.rs`'s click
//! routing. Lives in its own module (rather than inline in `main.rs`) so it
//! is reachable via `crate::window::raise_main_window` from both crate roots
//! this workspace builds from this source tree: the `fonos-desktop` binary
//! (`main.rs`) and the `fonos_desktop` library (`lib.rs`, used by the
//! integration tests under `tests/`) — `tray.rs` is compiled into both.

/// Bring the main window to the front on the *currently active* Space.
///
/// Dock clicks (`RunEvent::Reopen`), the tray "Open Fonos" item and the float
/// pill all funnel through here. tao's `set_focus` relies on the deprecated
/// `activateIgnoringOtherApps:`; under macOS 14+ cooperative activation that
/// request can lose the race against the Dock-click activation already in
/// flight and hand focus back to the previously active app. The window also
/// re-opens on whichever Space it last lived on, which from another desktop
/// reads as "nothing happened" (the always-visible float pill suppresses the
/// system's own Space switch, since the app already "has visible windows").
pub fn raise_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("main") else { return };

    #[cfg(target_os = "macos")]
    {
        let win = w;
        // NSWindow calls must run on the main thread; Reopen and tray events
        // already do, the float-pill event listener may not.
        let _ = win.clone().run_on_main_thread(move || {
            use objc2::runtime::AnyObject;
            let Ok(ptr) = win.ns_window() else { return };
            if ptr.is_null() {
                return;
            }
            let ns_window = ptr as *mut AnyObject;
            let nil = std::ptr::null_mut::<AnyObject>();
            // SAFETY: `ptr` is the live NSWindow backing `win` (the captured
            // handle keeps it alive) and we are on the main thread — same
            // contract as `commands::refresh_ns_window`.
            unsafe {
                // NSWindowCollectionBehaviorMoveToActiveSpace (1 << 1): when
                // ordered in, the window joins the Space the user is on. Must
                // be set before the window is ordered front.
                let behavior: usize = objc2::msg_send![ns_window, collectionBehavior];
                let _: () =
                    objc2::msg_send![ns_window, setCollectionBehavior: behavior | (1_usize << 1)];
                // MoveToActiveSpace only applies while ordering in — a window
                // still visible on another Space must be ordered out first.
                let visible: bool = objc2::msg_send![ns_window, isVisible];
                let on_active: bool = objc2::msg_send![ns_window, isOnActiveSpace];
                if visible && !on_active {
                    let _: () = objc2::msg_send![ns_window, orderOut: nil];
                }
            }
            let _ = win.show();
            let _ = win.unminimize();
            unsafe {
                // Cooperative, non-deprecated activation — a no-op when the
                // Dock click already activated us, so it cannot bounce focus
                // back to the previously active app the way the deprecated
                // call inside tao's `set_focus` can.
                let ns_app: *mut AnyObject =
                    objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
                let responds: bool =
                    objc2::msg_send![ns_app, respondsToSelector: objc2::sel!(activate)];
                if responds {
                    let _: () = objc2::msg_send![ns_app, activate];
                } else {
                    // macOS < 14 never had cooperative activation.
                    let _: () = objc2::msg_send![ns_app, activateIgnoringOtherApps: true];
                }
                let _: () = objc2::msg_send![ns_window, makeKeyAndOrderFront: nil];
                // Raise even if the activation request is declined.
                let _: () = objc2::msg_send![ns_window, orderFrontRegardless];
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

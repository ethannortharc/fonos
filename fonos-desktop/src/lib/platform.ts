// Best-effort platform detection for gating UI that only applies to one OS.
//
// Synchronous and dependency-free: reads the webview's navigator instead of
// pulling in @tauri-apps/plugin-os (which would need a Rust plugin + capability
// grant). This only *hides* options that cannot work on the current platform —
// the authoritative guard lives in the Rust backend (e.g. Apple STT returns an
// explicit error off macOS), so a wrong guess here is never a correctness risk.

/** True when running on macOS (or an Apple mobile webview). */
export const isMacOS: boolean = (() => {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent || "";
  // `navigator.platform` is deprecated but still the most reliable signal in
  // WebKit/WebKitGTK: "MacIntel" on macOS, "Linux x86_64" on Linux.
  const platform = (navigator as unknown as { platform?: string }).platform || "";
  return /Mac|iPhone|iPad|iPod/.test(platform) || /Macintosh|Mac OS X/.test(ua);
})();

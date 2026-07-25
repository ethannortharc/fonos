// The one copyright line the app shows anywhere in its UI.
//
// Deliberately a single line, and deliberately not translated: it names the
// copyright holder, and the holder's name is the same in every locale. Never
// pair it with a separate "by …" author row — the name is both the author and
// the holder. The year is the release year, not `new Date()`; it is bumped by
// hand, not derived from the clock.
//
// The native About panel (macOS ⌘-menu → About Fonos) and the Linux/Windows
// package metadata get the same string from `bundle.copyright` in
// tauri.conf.json — keep the two in sync when the year changes.
export const COPYRIGHT = "© 2026 Ethan H.B. Zhou";

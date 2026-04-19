import Foundation

/// Curated locale list shown in NotebookSettingsView language pickers.
/// Keeping this short avoids the 700-locale full list and matches the languages
/// users actually need.
struct SupportedLocale: Identifiable, Hashable {
    let id: String          // BCP-47 identifier, e.g. "zh-CN"
    let displayName: String

    static let all: [SupportedLocale] = [
        .init(id: "en-US", displayName: "English (US)"),
        .init(id: "en-GB", displayName: "English (UK)"),
        .init(id: "zh-CN", displayName: "中文 (简体)"),
        .init(id: "zh-TW", displayName: "中文 (繁體)"),
        .init(id: "ja-JP", displayName: "日本語"),
        .init(id: "ko-KR", displayName: "한국어"),
        .init(id: "fr-FR", displayName: "Français"),
        .init(id: "de-DE", displayName: "Deutsch"),
        .init(id: "es-ES", displayName: "Español"),
        .init(id: "es-MX", displayName: "Español (MX)"),
        .init(id: "pt-BR", displayName: "Português (BR)"),
        .init(id: "ru-RU", displayName: "Русский"),
        .init(id: "it-IT", displayName: "Italiano"),
        .init(id: "ar-SA", displayName: "العربية"),
        .init(id: "hi-IN", displayName: "हिन्दी"),
        .init(id: "vi-VN", displayName: "Tiếng Việt")
    ]

    /// Display label for a stored locale id. Returns "Auto" when nil/empty,
    /// the curated display name when known, or the id itself as a fallback.
    static func displayName(for id: String?) -> String {
        guard let id, !id.isEmpty else { return "Auto" }
        return all.first(where: { $0.id == id })?.displayName ?? id
    }
}

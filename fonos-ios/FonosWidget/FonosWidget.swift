import WidgetKit
import SwiftUI

// MARK: - Timeline Provider

struct FonosWidgetProvider: TimelineProvider {
    func placeholder(in context: Context) -> FonosWidgetEntry {
        FonosWidgetEntry(date: Date())
    }

    func getSnapshot(in context: Context, completion: @escaping (FonosWidgetEntry) -> Void) {
        completion(FonosWidgetEntry(date: Date()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<FonosWidgetEntry>) -> Void) {
        // Static widget — no time-based updates needed
        let entry = FonosWidgetEntry(date: Date())
        let timeline = Timeline(entries: [entry], policy: .never)
        completion(timeline)
    }
}

// MARK: - Entry

struct FonosWidgetEntry: TimelineEntry {
    let date: Date
}

// MARK: - Widget View

struct FonosWidgetEntryView: View {
    var entry: FonosWidgetProvider.Entry
    @Environment(\.widgetFamily) private var family

    // Design colors: bg #1a1917, accent #fbbf24
    private let backgroundColor = Color(red: 0x1a / 255.0, green: 0x19 / 255.0, blue: 0x17 / 255.0)
    private let amberColor = Color(red: 0xfb / 255.0, green: 0xbf / 255.0, blue: 0x24 / 255.0)

    var body: some View {
        Group {
            switch family {
            case .systemSmall:
                smallWidgetView
            case .systemMedium:
                mediumWidgetView
            default:
                smallWidgetView
            }
        }
        .containerBackground(for: .widget) {
            backgroundColor
        }
    }

    // MARK: Small Widget — single centered mic button

    private var smallWidgetView: some View {
        ZStack {
            backgroundColor
            VStack(spacing: 8) {
                ZStack {
                    Circle()
                        .fill(amberColor)
                        .frame(width: 56, height: 56)
                    Image(systemName: "mic.fill")
                        .font(.system(size: 24, weight: .semibold))
                        .foregroundColor(backgroundColor)
                }
                Text("Dictate")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(.white.opacity(0.7))
            }
        }
        .widgetURL(URL(string: "fonos://record"))
    }

    // MARK: Medium Widget — mic button + label

    private var mediumWidgetView: some View {
        ZStack {
            backgroundColor
            HStack(spacing: 20) {
                // Mic button
                ZStack {
                    Circle()
                        .fill(amberColor)
                        .frame(width: 64, height: 64)
                    Image(systemName: "mic.fill")
                        .font(.system(size: 28, weight: .semibold))
                        .foregroundColor(backgroundColor)
                }

                // Text
                VStack(alignment: .leading, spacing: 4) {
                    Text("Fonos")
                        .font(.system(size: 18, weight: .bold))
                        .foregroundColor(.white)
                    Text("Tap to start dictating")
                        .font(.system(size: 13, weight: .regular))
                        .foregroundColor(.white.opacity(0.6))
                }
                Spacer()
            }
            .padding(.horizontal, 20)
        }
        .widgetURL(URL(string: "fonos://record"))
    }
}

// MARK: - Widget Configuration

struct FonosWidget: Widget {
    let kind: String = "FonosWidget"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: FonosWidgetProvider()) { entry in
            FonosWidgetEntryView(entry: entry)
        }
        .configurationDisplayName("Fonos")
        .description("Tap to start dictating with Fonos.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

// MARK: - Previews

#Preview(as: .systemSmall) {
    FonosWidget()
} timeline: {
    FonosWidgetEntry(date: .now)
}

#Preview(as: .systemMedium) {
    FonosWidget()
} timeline: {
    FonosWidgetEntry(date: .now)
}

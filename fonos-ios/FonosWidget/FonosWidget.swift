import WidgetKit
import SwiftUI

/// Fonos home screen widget — scaffold placeholder.
struct FonosWidget: Widget {
    let kind: String = "FonosWidget"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: Provider()) { _ in
            FonosWidgetEntryView()
        }
        .configurationDisplayName("Fonos")
        .description("Tap to start dictating.")
        .supportedFamilies([.systemSmall])
    }
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> SimpleEntry {
        SimpleEntry(date: Date())
    }

    func getSnapshot(in context: Context, completion: @escaping (SimpleEntry) -> Void) {
        completion(SimpleEntry(date: Date()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SimpleEntry>) -> Void) {
        completion(Timeline(entries: [SimpleEntry(date: Date())], policy: .never))
    }
}

struct SimpleEntry: TimelineEntry {
    let date: Date
}

struct FonosWidgetEntryView: View {
    var body: some View {
        ZStack {
            Color(red: 0.102, green: 0.098, blue: 0.090)
            Image(systemName: "mic.fill")
                .font(.system(size: 32))
                .foregroundColor(Color(red: 0.984, green: 0.749, blue: 0.141))
        }
        .containerBackground(for: .widget) {
            Color(red: 0.102, green: 0.098, blue: 0.090)
        }
    }
}

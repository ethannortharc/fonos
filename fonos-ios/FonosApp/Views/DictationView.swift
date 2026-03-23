import SwiftUI

/// Main dictation screen — placeholder scaffold.
/// Business logic will be implemented by wp-executor.
struct DictationView: View {
    var body: some View {
        ZStack {
            Color(hex: "#1a1917")
                .ignoresSafeArea()

            VStack(spacing: 32) {
                Text("Fonos")
                    .font(.largeTitle.bold())
                    .foregroundColor(Color(hex: "#fafaf9"))

                // Placeholder mic button
                Circle()
                    .fill(Color(hex: "#fbbf24"))
                    .frame(width: 96, height: 96)
                    .overlay {
                        Image(systemName: "mic.fill")
                            .font(.system(size: 40))
                            .foregroundColor(Color(hex: "#1a1917"))
                    }

                Text("Tap to dictate")
                    .font(.subheadline)
                    .foregroundColor(Color(hex: "#fafaf9").opacity(0.6))
            }
        }
    }
}

#Preview {
    DictationView()
}

import SwiftUI

extension Color {
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let red, green, blue: Double
        switch hex.count {
        case 6:
            (red, green, blue) = (Double((int >> 16) & 0xFF) / 255,
                                  Double((int >> 8) & 0xFF) / 255,
                                  Double(int & 0xFF) / 255)
        default:
            (red, green, blue) = (1, 1, 1)
        }
        self.init(red: red, green: green, blue: blue)
    }
}

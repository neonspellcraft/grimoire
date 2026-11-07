import SwiftUI

/// Grimoire's palette: near-black violet chrome, a dark LCD with spectral green text, arcane violet accents.
enum Theme {
    static let chromeTop = Color(red: 0.16, green: 0.13, blue: 0.22)
    static let chromeBottom = Color(red: 0.07, green: 0.06, blue: 0.10)
    static let lcd = Color(red: 0.02, green: 0.03, blue: 0.035)
    static let lcdText = Color(red: 0.45, green: 1.0, blue: 0.68)
    static let lcdGhost = Color(red: 0.45, green: 1.0, blue: 0.68).opacity(0.07)
    static let lcdDim = Color(red: 0.45, green: 1.0, blue: 0.68).opacity(0.45)
    static let accent = Color(red: 0.70, green: 0.50, blue: 1.0)
    static let text = Color(white: 0.85)
    static let textDim = Color(white: 0.5)
    static let bevelLight = Color.white.opacity(0.14)
    static let bevelDark = Color.black.opacity(0.7)

    static func lcdFont(_ size: CGFloat) -> Font {
        .system(size: size, weight: .bold, design: .monospaced)
    }

    static func labelFont(_ size: CGFloat = 9) -> Font {
        .system(size: size, weight: .heavy, design: .monospaced)
    }
}

extension View {
    /// Raised (outset) or sunken (inset) 1px bevel, like late-90s chrome.
    func bevel(inset: Bool = false, cornerRadius: CGFloat = 3) -> some View {
        overlay(
            RoundedRectangle(cornerRadius: cornerRadius)
                .strokeBorder(
                    LinearGradient(
                        colors: inset ? [Theme.bevelDark, Theme.bevelLight] : [Theme.bevelLight, Theme.bevelDark],
                        startPoint: .top, endPoint: .bottom),
                    lineWidth: 1)
        )
    }

    /// Sunken LCD panel background.
    func lcdPanel(cornerRadius: CGFloat = 3) -> some View {
        background(RoundedRectangle(cornerRadius: cornerRadius).fill(Theme.lcd))
            .bevel(inset: true, cornerRadius: cornerRadius)
    }
}

/// Window body: gradient chrome with a bevelled edge. Also acts as a drag handle wherever nothing else is.
struct WindowChrome: View {
    var onDragEnded: (() -> Void)?

    var body: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(LinearGradient(colors: [Theme.chromeTop, Theme.chromeBottom], startPoint: .top, endPoint: .bottom))
            .overlay(WindowDragArea(onDragEnded: onDragEnded).clipShape(RoundedRectangle(cornerRadius: 8)))
            .bevel(cornerRadius: 8)
    }
}

import GrimoireCore
import SwiftUI

/// Segmented LCD-style bars, green at the bottom fading to violet at the top, with violet peak caps.
final class SpectrumVisualization: Visualization {
    private let analyzer = SpectrumAnalyzer(fftSize: 2048, bandCount: 20)
    private var dynamics = BarDynamics(count: 20)
    private var segmentColors: [CGColor] = []
    private let peakColor = Theme.accent.cgColor
    private let ghostColor = Theme.lcdGhost.cgColor

    private let barGap: CGFloat = 2
    private let segmentHeight: CGFloat = 2
    private let segmentGap: CGFloat = 1

    func update(window: UnsafeBufferPointer<Float>?, sampleRate: Double, dt: Double) {
        let targets = window.map { analyzer.bands(from: $0, sampleRate: sampleRate) }
        dynamics.step(targets: targets, dt: dt)
    }

    var isIdle: Bool { dynamics.isSettled }

    func draw(in context: CGContext, size: CGSize) {
        let count = dynamics.levels.count
        let barWidth = ((size.width - barGap * CGFloat(count - 1)) / CGFloat(count)).rounded(.down)
        let pitch = segmentHeight + segmentGap
        let segments = Int((size.height + segmentGap) / pitch)
        if segmentColors.count != segments { segmentColors = Self.gradient(segments) }

        // Rects are grouped by color (each segment row, the peaks, the unlit ghosts) so there are ~a dozen fills.
        var rows = [[CGRect]](repeating: [], count: segments)
        var peaks: [CGRect] = []
        var ghosts: [CGRect] = []
        for bar in 0..<count {
            let x = CGFloat(bar) * (barWidth + barGap)
            let lit = Int((CGFloat(dynamics.levels[bar]) * CGFloat(segments)).rounded())
            let peak = Int((CGFloat(dynamics.peaks[bar]) * CGFloat(segments)).rounded())
            for segment in 0..<segments {
                let rect = CGRect(
                    x: x, y: size.height - CGFloat(segment + 1) * pitch + segmentGap,
                    width: barWidth, height: segmentHeight)
                if segment < lit {
                    rows[segment].append(rect)
                } else if peak > 0, segment == peak - 1 {
                    peaks.append(rect)
                } else {
                    ghosts.append(rect)
                }
            }
        }
        context.setFillColor(ghostColor)
        context.fill(ghosts)
        for (segment, row) in rows.enumerated() where !row.isEmpty {
            context.setFillColor(segmentColors[segment])
            context.fill(row)
        }
        context.setFillColor(peakColor)
        context.fill(peaks)
    }

    /// Theme.lcdText (green) → Theme.accent (violet), bottom to top.
    private static func gradient(_ count: Int) -> [CGColor] {
        let bottom: (CGFloat, CGFloat, CGFloat) = (0.45, 1.0, 0.68)
        let top: (CGFloat, CGFloat, CGFloat) = (0.70, 0.50, 1.0)
        return (0..<count).map { index in
            let t = count > 1 ? CGFloat(index) / CGFloat(count - 1) : 0
            return CGColor(
                srgbRed: bottom.0 + (top.0 - bottom.0) * t,
                green: bottom.1 + (top.1 - bottom.1) * t,
                blue: bottom.2 + (top.2 - bottom.2) * t,
                alpha: 1)
        }
    }
}

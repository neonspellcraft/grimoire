import SwiftUI

/// Oscilloscope: the waveform as a glowing line, triggered on a rising zero crossing so steady tones hold still.
final class ScopeVisualization: Visualization {
    private let pointCount = 96
    private let span = 768
    private let gain: Float = 2.5
    private var points: [Float]
    private let lineColor = Theme.lcdText.cgColor
    private let glowColor = Theme.lcdText.opacity(0.22).cgColor
    private let axisColor = Theme.lcdGhost.cgColor

    init() {
        points = [Float](repeating: 0, count: pointCount)
    }

    func update(window: UnsafeBufferPointer<Float>?, sampleRate: Double, dt: Double) {
        guard let window, window.count >= span * 2 else {
            // Let the line flatten out smoothly when audio stops.
            let decay = Float(pow(0.002, dt))
            for i in points.indices {
                points[i] *= decay
                if abs(points[i]) < 0.001 { points[i] = 0 }
            }
            return
        }
        var start = window.count - span
        for i in stride(from: window.count - span, to: window.count - span * 2, by: -1)
        where window[i - 1] < 0 && window[i] >= 0 {
            start = i
            break
        }
        let step = Double(span) / Double(pointCount)
        for i in 0..<pointCount {
            points[i] = window[start + Int(Double(i) * step)]
        }
    }

    var isIdle: Bool { points.allSatisfy { $0 == 0 } }

    func draw(in context: CGContext, size: CGSize) {
        let middle = size.height / 2
        context.setFillColor(axisColor)
        context.fill(CGRect(x: 0, y: middle, width: size.width, height: 1))

        let path = CGMutablePath()
        for (i, value) in points.enumerated() {
            let point = CGPoint(
                x: size.width * CGFloat(i) / CGFloat(pointCount - 1),
                y: middle - CGFloat(max(-1, min(1, value * gain))) * (middle - 1))
            i == 0 ? path.move(to: point) : path.addLine(to: point)
        }
        // Glow is a wide translucent stroke under the line; a real shadow blur costs several times more CPU.
        context.setLineJoin(.round)
        context.addPath(path)
        context.setStrokeColor(glowColor)
        context.setLineWidth(3.5)
        context.strokePath()
        context.addPath(path)
        context.setStrokeColor(lineColor)
        context.setLineWidth(1.2)
        context.strokePath()
    }
}

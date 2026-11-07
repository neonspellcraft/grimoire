import Foundation

/// Classic analyzer motion: bars jump up to their target instantly and fall at a fixed rate;
/// peak caps hold for a moment, then fall.
public struct BarDynamics: Sendable {
    public private(set) var levels: [Float]
    public private(set) var peaks: [Float]
    /// Full bar heights per second.
    public var fallRate: Float = 1.6
    public var peakHold: Double = 0.35
    public var peakFallRate: Float = 0.9
    private var peakAge: [Double]

    public init(count: Int) {
        levels = [Float](repeating: 0, count: count)
        peaks = [Float](repeating: 0, count: count)
        peakAge = [Double](repeating: 0, count: count)
    }

    /// Advances by `dt` seconds. `targets` nil means silence (everything falls).
    public mutating func step(targets: [Float]?, dt: Double) {
        for i in levels.indices {
            let target = targets.flatMap { i < $0.count ? $0[i] : nil } ?? 0
            levels[i] = max(target, max(0, levels[i] - fallRate * Float(dt)))
            if levels[i] >= peaks[i] {
                peaks[i] = levels[i]
                peakAge[i] = 0
            } else {
                peakAge[i] += dt
                if peakAge[i] > peakHold {
                    peaks[i] = max(levels[i], peaks[i] - peakFallRate * Float(dt))
                }
            }
        }
    }

    /// True once every bar and peak is at zero, so animation can stop.
    public var isSettled: Bool {
        levels.allSatisfy { $0 == 0 } && peaks.allSatisfy { $0 == 0 }
    }
}

import Foundation

/// One value per band: low (0–120 Hz), mid (120–800 Hz), high (800 Hz–20 kHz).
public struct BandLevels: Sendable, Equatable {
    public var low: Float
    public var mid: Float
    public var high: Float

    public init(low: Float, mid: Float, high: Float) {
        self.low = low
        self.mid = mid
        self.high = high
    }

    public static let zero = BandLevels(low: 0, mid: 0, high: 0)

    public subscript(index: Int) -> Float {
        get { index == 0 ? low : index == 1 ? mid : high }
        set {
            switch index {
            case 0: low = newValue
            case 1: mid = newValue
            default: high = newValue
            }
        }
    }

    public var loudest: Float { max(low, mid, high) }
}

/// Measures the energy in the three bands, as 0...1 levels mapped from `floorDB...ceilingDB`.
public final class MultibandAnalyzer {
    /// Upper edge of each band in Hz. The low band starts just above DC.
    public static let crossovers: [Double] = [120, 800, 20_000]
    public var floorDB: Float = -66
    public var ceilingDB: Float = -10

    private let spectrum: PowerSpectrum

    public init(fftSize: Int = 2048) {
        spectrum = PowerSpectrum(size: fftSize)
    }

    /// Uses the last `fftSize` samples of `samples`.
    public func levels(from samples: UnsafeBufferPointer<Float>, sampleRate: Double) -> BandLevels {
        spectrum.compute(samples)
        let power = spectrum.power
        let binWidth = spectrum.binWidth(sampleRate: sampleRate)
        let bins = power.count
        var edges = [1]
        for crossover in Self.crossovers {
            edges.append(min(max(Int((min(crossover, sampleRate / 2) / binWidth).rounded()), edges.last!), bins))
        }

        var result = BandLevels.zero
        for band in 0..<3 {
            var sum: Float = 0
            for bin in edges[band]..<edges[band + 1] { sum += power[bin] }
            let amplitude = sum.squareRoot() * spectrum.amplitudeScale
            guard amplitude > 0 else { continue }
            let db = 20 * log10(amplitude)
            result[band] = min(max((db - floorDB) / (ceilingDB - floorDB), 0), 1)
        }
        return result
    }
}

/// Turns raw band levels into something good to animate with:
/// - `envelope`: fast-attack / slower-release follower, auto-ranged between each band's recent floor and peak,
///   so quiet passages, loud passages and dense mixes all swing across the full 0...1.
/// - `hits`: jumps to 1 on a transient (a kick, a snare, a hi-hat burst) and fades out.
public struct BandFollower: Sendable {
    public private(set) var envelope = BandLevels.zero
    public private(set) var hits = BandLevels.zero
    public var attack: Double = 0.015
    public var release: Double = 0.2
    public var hitFade: Double = 0.25

    /// Recent loudest and quietest level per band. Both drift slowly toward the signal so the range adapts.
    private var peak = BandLevels(low: 0.5, mid: 0.5, high: 0.5)
    private var floor = BandLevels.zero
    /// The range never shrinks below this, so steady sounds still read as "on" instead of collapsing to 0.
    private let minimumRange: Float = 0.2
    private var average = BandLevels.zero

    public init() {}

    /// `input` nil means silence.
    public mutating func step(_ input: BandLevels?, dt: Double) {
        guard dt > 0 else { return }
        for band in 0..<3 {
            let raw = input?[band] ?? 0
            peak[band] = max(raw, peak[band] - Float(dt * 0.05))
            floor[band] = min(raw, floor[band] + Float(dt * 0.08))
            floor[band] = min(floor[band], max(peak[band] - minimumRange, 0))
            let target = min(max((raw - floor[band]) / max(peak[band] - floor[band], minimumRange), 0), 1)
            let timeConstant = target > envelope[band] ? attack : release
            envelope[band] += (target - envelope[band]) * Float(1 - exp(-dt / timeConstant))

            let rise = raw - average[band]
            average[band] += rise * Float(1 - exp(-dt / 0.4))
            if rise > 0.08 + 0.15 * average[band], hits[band] < 0.5 {
                hits[band] = 1
            } else {
                hits[band] = max(0, hits[band] - Float(dt / hitFade))
            }
        }
    }
}

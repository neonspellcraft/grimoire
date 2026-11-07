import Foundation

/// Turns a window of mono samples into `bandCount` log-spaced spectrum levels in 0...1.
///
/// Uses `PowerSpectrum`, then takes each band's peak amplitude in dB, mapped from `floorDB...ceilingDB`.
public final class SpectrumAnalyzer {
    public var fftSize: Int { spectrum.size }
    public let bandCount: Int
    public var minFrequency: Double = 50
    public var maxFrequency: Double = 16_000
    public var floorDB: Float = -72
    public var ceilingDB: Float = -6

    private let spectrum: PowerSpectrum
    private var binRanges: [ClosedRange<Int>] = []
    private var binRangesSampleRate: Double = 0

    public init(fftSize: Int = 2048, bandCount: Int = 20) {
        spectrum = PowerSpectrum(size: fftSize)
        self.bandCount = bandCount
    }

    /// Uses the last `fftSize` samples of `samples`.
    public func bands(from samples: UnsafeBufferPointer<Float>, sampleRate: Double) -> [Float] {
        spectrum.compute(samples)
        if sampleRate != binRangesSampleRate { computeBinRanges(sampleRate: sampleRate) }
        let power = spectrum.power
        let amplitudeScale = spectrum.amplitudeScale
        return binRanges.map { range in
            var peak: Float = 0
            for bin in range { peak = max(peak, power[bin]) }
            let amplitude = peak.squareRoot() * amplitudeScale
            guard amplitude > 0 else { return 0 }
            let db = 20 * log10(amplitude)
            return min(max((db - floorDB) / (ceilingDB - floorDB), 0), 1)
        }
    }

    /// Index of the band that contains `frequency`, or nil if it's outside the analyzed range.
    public func bandIndex(for frequency: Double, sampleRate: Double) -> Int? {
        let top = min(maxFrequency, sampleRate / 2)
        guard frequency >= minFrequency, frequency < top else { return nil }
        let position = log(frequency / minFrequency) / log(top / minFrequency)
        return min(Int(position * Double(bandCount)), bandCount - 1)
    }

    private func computeBinRanges(sampleRate: Double) {
        let binWidth = spectrum.binWidth(sampleRate: sampleRate)
        let top = min(maxFrequency, sampleRate / 2)
        let lastBin = fftSize / 2 - 1
        binRanges = (0..<bandCount).map { band in
            let low = minFrequency * pow(top / minFrequency, Double(band) / Double(bandCount))
            let high = minFrequency * pow(top / minFrequency, Double(band + 1) / Double(bandCount))
            let first = min(max(1, Int((low / binWidth).rounded())), lastBin)
            let last = min(max(first, Int((high / binWidth).rounded()) - 1), lastBin)
            return first...last
        }
        binRangesSampleRate = sampleRate
    }
}

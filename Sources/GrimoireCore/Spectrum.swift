import Accelerate
import Foundation

/// Turns a window of mono samples into `bandCount` log-spaced spectrum levels in 0...1.
///
/// Hann window → real FFT (vDSP) → per-band peak amplitude in dB, mapped from `floorDB...ceilingDB`.
/// Buffers are allocated once, so calling `bands` every frame doesn't allocate beyond the returned array.
public final class SpectrumAnalyzer {
    public let fftSize: Int
    public let bandCount: Int
    public var minFrequency: Double = 50
    public var maxFrequency: Double = 16_000
    public var floorDB: Float = -72
    public var ceilingDB: Float = -6

    private let log2n: vDSP_Length
    private let setup: FFTSetup
    private let window: [Float]
    private var windowed: [Float]
    private var real: [Float]
    private var imaginary: [Float]
    private var magnitudes: [Float]
    private var binRanges: [ClosedRange<Int>] = []
    private var binRangesSampleRate: Double = 0

    public init(fftSize: Int = 2048, bandCount: Int = 20) {
        precondition(fftSize >= 64 && fftSize & (fftSize - 1) == 0, "fftSize must be a power of two")
        self.fftSize = fftSize
        self.bandCount = bandCount
        log2n = vDSP_Length(log2(Double(fftSize)))
        setup = vDSP_create_fftsetup(log2n, FFTRadix(kFFTRadix2))!
        window = vDSP.window(ofType: Float.self, usingSequence: .hanningDenormalized, count: fftSize, isHalfWindow: false)
        windowed = [Float](repeating: 0, count: fftSize)
        real = [Float](repeating: 0, count: fftSize / 2)
        imaginary = [Float](repeating: 0, count: fftSize / 2)
        magnitudes = [Float](repeating: 0, count: fftSize / 2)
    }

    deinit {
        vDSP_destroy_fftsetup(setup)
    }

    /// Uses the last `fftSize` samples of `samples`.
    public func bands(from samples: UnsafeBufferPointer<Float>, sampleRate: Double) -> [Float] {
        precondition(samples.count >= fftSize)
        let input = UnsafeBufferPointer(rebasing: samples[(samples.count - fftSize)...])
        vDSP.multiply(input, window, result: &windowed)

        let half = fftSize / 2
        windowed.withUnsafeBufferPointer { windowedPointer in
            real.withUnsafeMutableBufferPointer { realPointer in
                imaginary.withUnsafeMutableBufferPointer { imaginaryPointer in
                    var split = DSPSplitComplex(realp: realPointer.baseAddress!, imagp: imaginaryPointer.baseAddress!)
                    windowedPointer.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: half) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(half))
                    }
                    vDSP_fft_zrip(setup, &split, 1, log2n, FFTDirection(FFT_FORWARD))
                    magnitudes.withUnsafeMutableBufferPointer {
                        vDSP_zvmags(&split, 1, $0.baseAddress!, 1, vDSP_Length(half))
                    }
                }
            }
        }

        if sampleRate != binRangesSampleRate { computeBinRanges(sampleRate: sampleRate) }
        // zrip output is 2× the DFT and the Hann window halves a sine's peak, so a full-scale sine reads |X| = N/2.
        let amplitudeScale = 2 / Float(fftSize)
        return binRanges.map { range in
            var peak: Float = 0
            for bin in range { peak = max(peak, magnitudes[bin]) }
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
        let binWidth = sampleRate / Double(fftSize)
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

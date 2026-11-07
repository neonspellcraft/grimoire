import Accelerate
import Foundation

/// Hann-windowed real FFT (vDSP) producing squared magnitudes. Shared by the spectrum and multiband analyzers.
/// Buffers are allocated once, so `compute` can run every frame without allocating.
public final class PowerSpectrum {
    public let size: Int
    /// Squared magnitudes of bins `0..<size/2` from the last `compute` call.
    public private(set) var power: [Float]
    /// Multiply `sqrt(power[bin])` by this to get a sine's amplitude (full scale = 1).
    /// vDSP's zrip output is 2× the DFT and the Hann window halves a sine's peak, so a full-scale sine reads N/2.
    public var amplitudeScale: Float { 2 / Float(size) }

    private let log2n: vDSP_Length
    private let setup: FFTSetup
    private let window: [Float]
    private var windowed: [Float]
    private var real: [Float]
    private var imaginary: [Float]

    public init(size: Int = 2048) {
        precondition(size >= 64 && size & (size - 1) == 0, "FFT size must be a power of two")
        self.size = size
        log2n = vDSP_Length(log2(Double(size)))
        setup = vDSP_create_fftsetup(log2n, FFTRadix(kFFTRadix2))!
        window = vDSP.window(ofType: Float.self, usingSequence: .hanningDenormalized, count: size, isHalfWindow: false)
        windowed = [Float](repeating: 0, count: size)
        real = [Float](repeating: 0, count: size / 2)
        imaginary = [Float](repeating: 0, count: size / 2)
        power = [Float](repeating: 0, count: size / 2)
    }

    deinit {
        vDSP_destroy_fftsetup(setup)
    }

    public func binWidth(sampleRate: Double) -> Double {
        sampleRate / Double(size)
    }

    /// Analyzes the last `size` samples of `samples`.
    public func compute(_ samples: UnsafeBufferPointer<Float>) {
        precondition(samples.count >= size)
        let input = UnsafeBufferPointer(rebasing: samples[(samples.count - size)...])
        vDSP.multiply(input, window, result: &windowed)

        let half = size / 2
        windowed.withUnsafeBufferPointer { windowedPointer in
            real.withUnsafeMutableBufferPointer { realPointer in
                imaginary.withUnsafeMutableBufferPointer { imaginaryPointer in
                    var split = DSPSplitComplex(realp: realPointer.baseAddress!, imagp: imaginaryPointer.baseAddress!)
                    windowedPointer.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: half) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(half))
                    }
                    vDSP_fft_zrip(setup, &split, 1, log2n, FFTDirection(FFT_FORWARD))
                    power.withUnsafeMutableBufferPointer {
                        vDSP_zvmags(&split, 1, $0.baseAddress!, 1, vDSP_Length(half))
                    }
                }
            }
        }
    }
}

import Foundation

/// Pulls a fixed-size window of recent samples from an `AudioTap` once per display frame.
///
/// The tap fills its buffer in ~100 ms chunks, so the reader keeps its own cursor that advances in real time and
/// trails the newest sample by about one chunk. That gives smooth motion at the display's frame rate.
/// Each visualizer owns its own reader; they can share one tap.
@MainActor
final class TapReader {
    let tap: AudioTap
    private(set) var samples: [Float]
    private(set) var sampleRate: Double = 44_100
    private var cursor: Double = 0
    private var lastWritten = 0

    init(tap: AudioTap, windowSize: Int = 2048) {
        self.tap = tap
        samples = [Float](repeating: 0, count: windowSize)
    }

    /// Advances by `dt` seconds and refreshes `samples`. Returns false when there's no audio to show.
    func read(dt: Double, playing: Bool) -> Bool {
        let position = tap.position
        sampleRate = position.sampleRate
        defer { lastWritten = position.written }
        guard playing, position.written >= samples.count else { return false }

        if position.written < lastWritten || cursor > Double(position.written + position.chunk) {
            cursor = Double(position.written - position.chunk)   // tap was reset (new track)
        }
        cursor += dt * position.sampleRate
        let newest = Double(position.written)
        cursor = min(max(cursor, newest - 2 * Double(position.chunk)), newest)
        return samples.withUnsafeMutableBufferPointer {
            tap.copyWindow(endingAt: Int(cursor), into: $0)
        }
    }
}

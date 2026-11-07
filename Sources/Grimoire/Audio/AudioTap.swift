import AVFoundation

/// Captures what a node is playing into a mono ring buffer for visualizers.
///
/// macOS delivers tap buffers in ~100 ms chunks regardless of the requested size, so analysis isn't done here.
/// The tap only stores samples; visualizers pull a window at their own frame rate (see `VisualizerRenderer`).
final class AudioTap: @unchecked Sendable {
    struct Position {
        /// Total samples written since the tap was installed (an absolute index into the stream).
        let written: Int
        let sampleRate: Double
        /// Size of the most recent chunk, roughly how far behind real time the newest sample can be.
        let chunk: Int
    }

    static let capacity = 16_384

    private let lock = NSLock()
    private var ring = [Float](repeating: 0, count: capacity)
    private var written = 0
    private var sampleRate: Double = 44_100
    private var chunk = 4_096
    private weak var node: AVAudioNode?

    func install(on node: AVAudioNode) {
        remove()
        let format = node.outputFormat(forBus: 0)
        lock.withLock {
            written = 0
            sampleRate = format.sampleRate > 0 ? format.sampleRate : 44_100
        }
        node.installTap(onBus: 0, bufferSize: 2048, format: nil) { [weak self] buffer, _ in
            self?.append(buffer)
        }
        self.node = node
    }

    func remove() {
        node?.removeTap(onBus: 0)
        node = nil
    }

    var position: Position {
        lock.withLock { Position(written: written, sampleRate: sampleRate, chunk: chunk) }
    }

    /// Copies the `output.count` samples that end at absolute index `end`. False if they aren't buffered.
    func copyWindow(endingAt end: Int, into output: UnsafeMutableBufferPointer<Float>) -> Bool {
        lock.withLock {
            let count = output.count
            let start = end - count
            guard start >= 0, end <= written, start >= written - Self.capacity else { return false }
            for i in 0..<count { output[i] = ring[(start + i) % Self.capacity] }
            return true
        }
    }

    private func append(_ buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData else { return }
        let frames = Int(buffer.frameLength)
        let channelCount = Int(buffer.format.channelCount)
        guard frames > 0, channelCount > 0 else { return }
        let scale = 1 / Float(channelCount)
        lock.withLock {
            let start = written
            for frame in 0..<frames {
                var sum: Float = 0
                for channel in 0..<channelCount { sum += channels[channel][frame] }
                ring[(start + frame) % Self.capacity] = sum * scale
            }
            written = start + frames
            chunk = frames
        }
    }
}

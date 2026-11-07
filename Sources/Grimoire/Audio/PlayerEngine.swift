import AVFoundation
import GrimoireCore

/// Plays one audio file at a time through AVAudioEngine.
///
/// Graph: `AVAudioPlayerNode → mainMixerNode → output`. The engine (rather than AVAudioPlayer) is used so a
/// visualizer tap and an EQ can be inserted later without rewiring playback.
@MainActor
final class PlayerEngine {
    struct FileInfo {
        let duration: TimeInterval
        let sampleRate: Double
        let channels: Int
        let bitrate: Int?
        let format: String
    }

    /// Called when the current file finished playing on its own (not after stop or seek).
    var onFinished: (() -> Void)?

    /// Taps the player node (before the volume stage), so visualizers don't shrink when the volume goes down.
    let tap = AudioTap()

    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private var file: AVAudioFile?
    private var startFrame: AVAudioFramePosition = 0
    private var lastKnownTime: TimeInterval = 0
    private var isScheduled = false
    private var wantsPlayback = false
    /// Bumped whenever scheduled audio is discarded, so stale completion callbacks are ignored.
    private var generation = 0

    init() {
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: nil)
        NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.recoverFromConfigurationChange() }
        }
    }

    var volume: Float {
        get { engine.mainMixerNode.outputVolume }
        set { engine.mainMixerNode.outputVolume = newValue }
    }

    var currentTime: TimeInterval {
        guard let file else { return 0 }
        guard player.isPlaying,
              let nodeTime = player.lastRenderTime,
              let playerTime = player.playerTime(forNodeTime: nodeTime)
        else { return lastKnownTime }
        lastKnownTime = SeekMath.time(
            startFrame: startFrame, renderedFrames: playerTime.sampleTime,
            sampleRate: file.processingFormat.sampleRate, length: file.length)
        return lastKnownTime
    }

    func load(_ url: URL) throws -> FileInfo {
        stop()
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        if player.outputFormat(forBus: 0) != format {
            tap.remove()
            engine.stop()
            engine.disconnectNodeOutput(player)
            engine.connect(player, to: engine.mainMixerNode, format: format)
        }
        tap.install(on: player)
        self.file = file

        let duration = Double(file.length) / format.sampleRate
        let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize
        let bitrate = size.flatMap { duration > 0 ? Int(Double($0) * 8 / duration / 1000) : nil }
        return FileInfo(
            duration: duration, sampleRate: format.sampleRate, channels: Int(format.channelCount),
            bitrate: bitrate, format: url.pathExtension.uppercased())
    }

    func play() throws {
        guard file != nil else { return }
        if !engine.isRunning { try engine.start() }
        if !isScheduled { schedule(from: startFrame) }
        player.play()
        wantsPlayback = true
    }

    func pause() {
        _ = currentTime
        player.pause()
        wantsPlayback = false
    }

    /// Stops and rewinds to the start of the file.
    func stop() {
        generation += 1
        player.stop()
        isScheduled = false
        wantsPlayback = false
        startFrame = 0
        lastKnownTime = 0
    }

    func seek(to time: TimeInterval) {
        guard let file else { return }
        let wasPlaying = player.isPlaying
        let frame = SeekMath.frame(for: time, sampleRate: file.processingFormat.sampleRate, length: file.length)
        if !engine.isRunning { try? engine.start() }
        schedule(from: frame)
        lastKnownTime = Double(frame) / file.processingFormat.sampleRate
        if wasPlaying { player.play() }
    }

    private func schedule(from frame: AVAudioFramePosition) {
        guard let file else { return }
        generation += 1
        player.stop()
        let scheduledGeneration = generation
        startFrame = frame

        let remaining = file.length - frame
        guard remaining > 0 else {
            isScheduled = false
            DispatchQueue.main.async { [weak self] in
                MainActor.assumeIsolated {
                    guard let self, self.generation == scheduledGeneration else { return }
                    self.finish()
                }
            }
            return
        }
        player.scheduleSegment(
            file, startingFrame: frame, frameCount: AVAudioFrameCount(remaining), at: nil,
            completionCallbackType: .dataPlayedBack
        ) { [weak self] _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.generation == scheduledGeneration else { return }
                    self.finish()
                }
            }
        }
        isScheduled = true
    }

    private func finish() {
        isScheduled = false
        wantsPlayback = false
        startFrame = 0
        lastKnownTime = 0
        onFinished?()
    }

    /// The engine stops itself when the output device changes (e.g. headphones unplugged); resume where we were.
    private func recoverFromConfigurationChange() {
        guard let file else { return }
        let resume = wantsPlayback
        let frame = SeekMath.frame(for: lastKnownTime, sampleRate: file.processingFormat.sampleRate, length: file.length)
        schedule(from: frame)
        if resume {
            try? engine.start()
            player.play()
        }
    }
}

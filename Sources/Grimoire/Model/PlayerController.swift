import AppKit
import GrimoireCore
import Observation
import UniformTypeIdentifiers

/// App-wide playback state: owns the playlist and the audio engine, and is what every view talks to.
@MainActor @Observable
final class PlayerController {
    enum State { case stopped, playing, paused }

    private enum Keys {
        static let volume = "volume"
        static let shuffle = "shuffle"
        static let repeatMode = "repeatMode"
        static let visualization = "visualization"
    }

    private(set) var playlist = Playlist()
    private(set) var state: State = .stopped
    private(set) var currentTime: TimeInterval = 0
    private(set) var duration: TimeInterval = 0
    private(set) var fileInfo: PlayerEngine.FileInfo?
    /// Transient status (errors) shown in the marquee instead of the track title.
    private(set) var message: String?
    var showRemaining = false
    var volume: Double {
        didSet {
            engine.volume = Float(volume)
            UserDefaults.standard.set(volume, forKey: Keys.volume)
        }
    }
    var visualization: VisualizationKind {
        didSet { UserDefaults.standard.set(visualization.rawValue, forKey: Keys.visualization) }
    }

    @ObservationIgnored weak var nowPlaying: NowPlaying?
    @ObservationIgnored private let engine = PlayerEngine()
    @ObservationIgnored private var ticker: Timer?

    init() {
        let defaults = UserDefaults.standard
        volume = defaults.object(forKey: Keys.volume) as? Double ?? 0.8
        visualization = defaults.string(forKey: Keys.visualization).flatMap(VisualizationKind.init) ?? .spectrum
        playlist.shuffle = defaults.bool(forKey: Keys.shuffle)
        playlist.repeatMode = defaults.string(forKey: Keys.repeatMode).flatMap(RepeatMode.init) ?? .off
        engine.volume = Float(volume)
        engine.onFinished = { [weak self] in self?.trackFinished() }
    }

    var currentTrack: Track? { playlist.current }

    var audioTap: AudioTap { engine.tap }

    var marqueeText: String {
        if let message { return message }
        guard let track = playlist.current, let index = playlist.currentIndex else {
            return "GRIMOIRE  ✦  drop music here or press ⌘O"
        }
        let length = track.duration.map { " (\(TimeFormat.clock($0)))" } ?? ""
        return "\(index + 1). \(track.displayTitle)\(length)"
    }

    // MARK: Transport

    func togglePlayPause() {
        state == .playing ? pause() : play()
    }

    func play() {
        switch state {
        case .playing:
            return
        case .paused:
            do {
                try engine.play()
                state = .playing
                startTicker()
            } catch {
                fail(error)
            }
            nowPlaying?.update()
        case .stopped:
            if let track = playlist.current ?? playlist.advance(automatic: false) { start(track) }
        }
    }

    func pause() {
        guard state == .playing else { return }
        engine.pause()
        currentTime = engine.currentTime
        state = .paused
        stopTicker()
        nowPlaying?.update()
    }

    func stop() {
        engine.stop()
        state = .stopped
        currentTime = 0
        stopTicker()
        nowPlaying?.update()
    }

    func next() {
        if let track = playlist.advance(automatic: false) { start(track) }
    }

    /// Like most players: restarts the current track if we're a few seconds in, otherwise goes back one.
    func previous() {
        if state != .stopped, currentTime > 3 {
            seek(to: 0)
        } else if let track = playlist.retreat() {
            start(track)
        }
    }

    func playTrack(_ id: Track.ID) {
        playlist.select(id)
        if let track = playlist.current { start(track) }
    }

    func seek(to time: TimeInterval) {
        guard fileInfo != nil else { return }
        engine.seek(to: time)
        currentTime = engine.currentTime
        nowPlaying?.update()
    }

    func seek(by delta: TimeInterval) {
        guard state != .stopped else { return }
        seek(to: min(max(0, currentTime + delta), duration))
    }

    var shuffle: Bool { playlist.shuffle }
    var repeatMode: RepeatMode { playlist.repeatMode }

    func toggleShuffle() {
        playlist.shuffle.toggle()
        UserDefaults.standard.set(playlist.shuffle, forKey: Keys.shuffle)
    }

    func cycleRepeat() {
        playlist.repeatMode = playlist.repeatMode.next
        UserDefaults.standard.set(playlist.repeatMode.rawValue, forKey: Keys.repeatMode)
    }

    private func start(_ track: Track) {
        do {
            let info = try engine.load(track.url)
            fileInfo = info
            duration = info.duration
            currentTime = 0
            message = nil
            playlist.update(track.id) { $0.duration = info.duration }
            try engine.play()
            state = .playing
            startTicker()
        } catch {
            fail(error, track: track)
        }
        nowPlaying?.update()
    }

    private func trackFinished() {
        if let track = playlist.advance(automatic: true) {
            start(track)
        } else {
            stop()
        }
    }

    private func fail(_ error: Error, track: Track? = nil) {
        engine.stop()
        state = .stopped
        fileInfo = nil
        duration = 0
        currentTime = 0
        stopTicker()
        message = "Can't play \(track?.url.lastPathComponent ?? "file"): \(error.localizedDescription)"
    }

    private func startTicker() {
        guard ticker == nil else { return }
        let timer = Timer(timeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.currentTime = self.engine.currentTime
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        ticker = timer
    }

    private func stopTicker() {
        ticker?.invalidate()
        ticker = nil
    }

    // MARK: Playlist

    /// Adds files, folders (recursively) and m3u playlists. `playFirst` starts the first added track.
    func add(_ urls: [URL], at index: Int? = nil, playFirst: Bool = false) {
        var audio: [URL] = []
        for url in urls {
            if AudioFiles.isPlaylist(url), let text = try? String(contentsOf: url, encoding: .utf8) {
                audio += M3U.parse(text, relativeTo: url.deletingLastPathComponent()).filter(AudioFiles.isSupported)
            } else {
                audio += AudioFiles.expand([url])
            }
        }
        let tracks = audio.map { Track(url: $0) }
        guard let first = tracks.first else { return }
        playlist.insert(tracks, at: index ?? playlist.tracks.count)
        loadMetadata(for: tracks)
        if playFirst { playTrack(first.id) }
    }

    func remove(_ ids: Set<Track.ID>) {
        let removingCurrent = playlist.currentID.map(ids.contains) ?? false
        if removingCurrent {
            stop()
            fileInfo = nil
            duration = 0
        }
        playlist.remove(ids: ids)
    }

    func clear() {
        stop()
        fileInfo = nil
        duration = 0
        message = nil
        playlist.removeAll()
    }

    func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        playlist.move(fromOffsets: source, toOffset: destination)
    }

    private func loadMetadata(for tracks: [Track]) {
        Task {
            for track in tracks {
                let meta = await TrackLoader.metadata(for: track.url)
                playlist.update(track.id) {
                    $0.title = meta.title ?? $0.title
                    $0.artist = meta.artist ?? $0.artist
                    if $0.duration == nil { $0.duration = meta.duration }
                }
                if track.id == playlist.currentID { nowPlaying?.update() }
            }
        }
    }

    // MARK: Files & panels

    func openFiles() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = true
        panel.allowedContentTypes = [.audio, .m3uPlaylist]
        guard panel.runModal() == .OK else { return }
        add(panel.urls, playFirst: true)
    }

    func openFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK else { return }
        add(panel.urls, playFirst: true)
    }

    func loadPlaylist() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.m3uPlaylist]
        guard panel.runModal() == .OK else { return }
        clear()
        add(panel.urls, playFirst: true)
    }

    func savePlaylist() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.m3uPlaylist]
        panel.nameFieldStringValue = "Playlist.m3u"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try M3U.serialize(playlist.tracks).write(to: url, atomically: true, encoding: .utf8)
        } catch {
            message = "Couldn't save playlist: \(error.localizedDescription)"
        }
    }

    // MARK: Session

    private static var sessionURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Grimoire/last-session.m3u")
    }

    func saveSession() {
        let url = Self.sessionURL
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? M3U.serialize(playlist.tracks).write(to: url, atomically: true, encoding: .utf8)
    }

    func restoreSession() {
        guard FileManager.default.fileExists(atPath: Self.sessionURL.path) else { return }
        add([Self.sessionURL])
    }
}

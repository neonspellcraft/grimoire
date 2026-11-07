import MediaPlayer

/// Media keys, AirPods controls and the Control Center "Now Playing" panel.
@MainActor
final class NowPlaying {
    private weak var controller: PlayerController?

    init(controller: PlayerController) {
        self.controller = controller
        let center = MPRemoteCommandCenter.shared()
        handle(center.playCommand) { $0.play() }
        handle(center.pauseCommand) { $0.pause() }
        handle(center.togglePlayPauseCommand) { $0.togglePlayPause() }
        handle(center.stopCommand) { $0.stop() }
        handle(center.nextTrackCommand) { $0.next() }
        handle(center.previousTrackCommand) { $0.previous() }
        center.changePlaybackPositionCommand.addTarget { [weak controller] event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            let position = event.positionTime
            Task { @MainActor in controller?.seek(to: position) }
            return .success
        }
    }

    private func handle(_ command: MPRemoteCommand, _ action: @escaping @MainActor (PlayerController) -> Void) {
        command.addTarget { [weak controller] _ in
            Task { @MainActor in if let controller { action(controller) } }
            return .success
        }
    }

    func update() {
        guard let controller else { return }
        let center = MPNowPlayingInfoCenter.default()
        guard let track = controller.currentTrack, controller.state != .stopped else {
            center.nowPlayingInfo = nil
            center.playbackState = .stopped
            return
        }
        center.nowPlayingInfo = [
            MPMediaItemPropertyTitle: track.title ?? track.url.deletingPathExtension().lastPathComponent,
            MPMediaItemPropertyArtist: track.artist ?? "",
            MPMediaItemPropertyPlaybackDuration: controller.duration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: controller.currentTime,
            MPNowPlayingInfoPropertyPlaybackRate: controller.state == .playing ? 1.0 : 0.0,
        ]
        center.playbackState = controller.state == .playing ? .playing : .paused
    }
}

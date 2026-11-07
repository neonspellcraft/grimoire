import AppKit
import Observation
import SwiftUI

/// Borderless windows can't become key by default, which would break keyboard focus and the playlist selection.
private final class BorderlessWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

/// Creates the main and playlist windows, keeps the playlist docked under the main window,
/// and owns the free-floating visualizer window.
@MainActor @Observable
final class WindowManager {
    private enum Keys {
        static let mainFrame = "GrimoireMain"
        static let playlistVisible = "playlistVisible"
        static let playlistSize = "playlistSize"
        static let visualizerFrame = "GrimoireVisualizer"
        static let visualizerVisible = "visualizerVisible"
    }

    private(set) var isPlaylistVisible = false
    private(set) var isVisualizerVisible = false

    @ObservationIgnored private var main: NSWindow!
    @ObservationIgnored private var playlist: NSWindow!
    /// Created the first time it's shown.
    @ObservationIgnored private var visualizer: NSWindow?
    @ObservationIgnored private weak var player: PlayerController?
    /// Distance at which a dragged playlist snaps back under the main window.
    @ObservationIgnored private let snapDistance: CGFloat = 20

    func setUp(player: PlayerController) {
        self.player = player
        // The main window is fixed-size, so let SwiftUI size it to its content.
        main = makeWindow(
            MainWindowView(player: player, windows: self), styleMask: [.borderless, .miniaturizable],
            sizingOptions: [.minSize, .maxSize, .intrinsicContentSize])
        let mainSize = main.contentView!.fittingSize
        if !main.setFrameUsingName(Keys.mainFrame) { main.center() }
        main.setContentSize(mainSize)
        main.setFrameAutosaveName(Keys.mainFrame)

        playlist = makeWindow(PlaylistView(player: player, windows: self), styleMask: [.borderless, .resizable], sizingOptions: [])
        playlist.minSize = NSSize(width: 320, height: 160)
        let savedSize = UserDefaults.standard.string(forKey: Keys.playlistSize).map(NSSizeFromString)
            .flatMap { $0.width >= playlist.minSize.width && $0.height >= playlist.minSize.height ? $0 : nil }
        playlist.setContentSize(savedSize ?? NSSize(width: mainSize.width, height: 280))

        main.makeKeyAndOrderFront(nil)
        let showPlaylist = UserDefaults.standard.object(forKey: Keys.playlistVisible) as? Bool ?? true
        if showPlaylist { setPlaylistVisible(true) }
        if UserDefaults.standard.bool(forKey: Keys.visualizerVisible) { setVisualizerVisible(true) }
    }

    func togglePlaylist() {
        setPlaylistVisible(!isPlaylistVisible)
    }

    func toggleVisualizer() {
        setVisualizerVisible(!isVisualizerVisible)
    }

    func minimize() {
        main.miniaturize(nil)
    }

    func quit() {
        NSApp.terminate(nil)
    }

    func saveState() {
        UserDefaults.standard.set(isPlaylistVisible, forKey: Keys.playlistVisible)
        UserDefaults.standard.set(NSStringFromSize(playlist.frame.size), forKey: Keys.playlistSize)
        UserDefaults.standard.set(isVisualizerVisible, forKey: Keys.visualizerVisible)
    }

    /// Called after the playlist is dragged: snap and dock it if it ended up near the main window's bottom edge,
    /// otherwise let it float on its own.
    func snapPlaylist() {
        let mainFrame = main.frame
        var frame = playlist.frame
        let overlapsHorizontally = frame.maxX > mainFrame.minX && frame.minX < mainFrame.maxX
        guard overlapsHorizontally, abs(frame.maxY - mainFrame.minY) < snapDistance else {
            main.removeChildWindow(playlist)
            return
        }
        frame.origin.y = mainFrame.minY - frame.height
        if abs(frame.minX - mainFrame.minX) < snapDistance { frame.origin.x = mainFrame.minX }
        playlist.setFrame(frame, display: true)
        main.addChildWindow(playlist, ordered: .above)
    }

    private func setPlaylistVisible(_ visible: Bool) {
        isPlaylistVisible = visible
        if visible {
            // Re-showing always docks it under the main window; the child relationship makes it follow drags.
            let size = playlist.frame.size
            playlist.setFrame(
                NSRect(x: main.frame.minX, y: main.frame.minY - size.height, width: size.width, height: size.height),
                display: false)
            main.addChildWindow(playlist, ordered: .above)
            playlist.orderFront(nil)
        } else {
            main.removeChildWindow(playlist)
            playlist.orderOut(nil)
        }
    }

    private func setVisualizerVisible(_ visible: Bool) {
        if visible {
            guard let window = visualizer ?? makeVisualizerWindow() else { return }
            window.makeKeyAndOrderFront(nil)
        } else {
            visualizer?.orderOut(nil)
        }
        isVisualizerVisible = visible
    }

    /// A regular resizable window (so it can go full screen), dressed down to just the Metal view.
    private func makeVisualizerWindow() -> NSWindow? {
        guard let player, let content = SceneHostView(player: player) else { return nil }
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 640, height: 400),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        window.title = "Grimoire Visualizer"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.backgroundColor = .black
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 240, height: 150)
        window.collectionBehavior = [.fullScreenPrimary]
        window.contentView = content
        if !window.setFrameUsingName(Keys.visualizerFrame) { window.center() }
        window.setFrameAutosaveName(Keys.visualizerFrame)
        NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.isVisualizerVisible = false }
        }
        visualizer = window
        return window
    }

    private func makeWindow<Content: View>(
        _ content: Content, styleMask: NSWindow.StyleMask, sizingOptions: NSHostingSizingOptions
    ) -> NSWindow {
        let window = BorderlessWindow(
            contentRect: NSRect(x: 0, y: 0, width: 520, height: 280),
            styleMask: styleMask, backing: .buffered, defer: false)
        let hosting = NSHostingView(rootView: content)
        hosting.sizingOptions = sizingOptions
        window.contentView = hosting
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.isReleasedWhenClosed = false
        window.collectionBehavior = [.fullScreenNone]
        return window
    }
}

import AppKit

@main
@MainActor
enum GrimoireMain {
    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        withExtendedLifetime(delegate) { app.run() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let player = PlayerController()
    let windows = WindowManager()
    private var nowPlaying: NowPlaying?
    private var keyMonitor: Any?

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = makeMainMenu()
        player.restoreSession()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let nowPlaying = NowPlaying(controller: player)
        player.nowPlaying = nowPlaying
        self.nowPlaying = nowPlaying
        windows.setUp(player: player)
        installKeyboardShortcuts()
        NSApp.activate()
    }

    /// Finder "Open With", dropping on the Dock icon, `open -a Grimoire file.flac`.
    func application(_ application: NSApplication, open urls: [URL]) {
        player.add(urls, playFirst: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func applicationWillTerminate(_ notification: Notification) {
        player.saveSession()
        windows.saveState()
    }

    // MARK: Keyboard

    /// Space and the arrow keys have no modifier, so they're handled here rather than as menu shortcuts.
    private func installKeyboardShortcuts() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.numericPad, .function])
            guard modifiers.isEmpty, NSApp.modalWindow == nil else { return event }
            switch event.keyCode {
            case 49: player.togglePlayPause()   // space
            case 123: player.seek(by: -5)       // left arrow
            case 124: player.seek(by: 5)        // right arrow
            default: return event
            }
            return nil
        }
    }

    // MARK: Menu

    private func makeMainMenu() -> NSMenu {
        let mainMenu = NSMenu()

        let appMenu = NSMenu()
        appMenu.addItem(NSMenuItem(title: "About Grimoire", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: ""))
        appMenu.addItem(.separator())
        appMenu.addItem(NSMenuItem(title: "Hide Grimoire", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h"))
        appMenu.addItem(NSMenuItem(title: "Quit Grimoire", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        addSubmenu(appMenu, titled: "Grimoire", to: mainMenu)

        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(item("Open Files…", #selector(openFiles), "o"))
        fileMenu.addItem(item("Open Folder…", #selector(openFolder), "o", [.command, .shift]))
        fileMenu.addItem(.separator())
        fileMenu.addItem(item("Open Playlist…", #selector(loadPlaylist), "o", [.command, .option]))
        fileMenu.addItem(item("Save Playlist…", #selector(savePlaylist), "s"))
        addSubmenu(fileMenu, titled: "File", to: mainMenu)

        let playbackMenu = NSMenu(title: "Playback")
        playbackMenu.addItem(item("Play / Pause", #selector(togglePlayPause), "p"))
        playbackMenu.addItem(item("Stop", #selector(stop), "."))
        playbackMenu.addItem(item("Next", #selector(next), arrowKey(NSRightArrowFunctionKey)))
        playbackMenu.addItem(item("Previous", #selector(previous), arrowKey(NSLeftArrowFunctionKey)))
        playbackMenu.addItem(.separator())
        playbackMenu.addItem(item("Shuffle", #selector(toggleShuffle), ""))
        playbackMenu.addItem(item("Cycle Repeat", #selector(cycleRepeat), ""))
        addSubmenu(playbackMenu, titled: "Playback", to: mainMenu)

        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(item("Toggle Playlist", #selector(togglePlaylist), "l"))
        windowMenu.addItem(item("Toggle Visualizer Window", #selector(toggleVisualizer), "v", [.command, .shift]))
        let sceneMenu = NSMenu(title: "Visualizer Scene")
        for kind in WindowSceneKind.allCases {
            let sceneItem = item(kind.title, #selector(selectScene(_:)), "")
            sceneItem.representedObject = kind.rawValue
            sceneMenu.addItem(sceneItem)
        }
        addSubmenu(sceneMenu, titled: "Visualizer Scene", to: windowMenu)
        windowMenu.addItem(item("Minimize", #selector(minimize), "m"))
        windowMenu.addItem(.separator())
        let visualizationMenu = NSMenu(title: "Visualization")
        for kind in VisualizationKind.allCases {
            let kindItem = item(kind.title, #selector(selectVisualization(_:)), "")
            kindItem.representedObject = kind.rawValue
            visualizationMenu.addItem(kindItem)
        }
        addSubmenu(visualizationMenu, titled: "Visualization", to: windowMenu)
        addSubmenu(windowMenu, titled: "Window", to: mainMenu)

        return mainMenu
    }

    private func addSubmenu(_ submenu: NSMenu, titled title: String, to menu: NSMenu) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.submenu = submenu
        menu.addItem(item)
    }

    private func item(_ title: String, _ action: Selector, _ key: String, _ modifiers: NSEvent.ModifierFlags = .command) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        item.target = self
        return item
    }

    private func arrowKey(_ key: Int) -> String {
        String(Character(UnicodeScalar(UInt16(key))!))
    }

    @objc private func openFiles() { player.openFiles() }
    @objc private func openFolder() { player.openFolder() }
    @objc private func loadPlaylist() { player.loadPlaylist() }
    @objc private func savePlaylist() { player.savePlaylist() }
    @objc private func togglePlayPause() { player.togglePlayPause() }
    @objc private func stop() { player.stop() }
    @objc private func next() { player.next() }
    @objc private func previous() { player.previous() }
    @objc private func toggleShuffle() { player.toggleShuffle() }
    @objc private func cycleRepeat() { player.cycleRepeat() }
    @objc private func togglePlaylist() { windows.togglePlaylist() }
    @objc private func toggleVisualizer() { windows.toggleVisualizer() }
    @objc private func minimize() { windows.minimize() }

    @objc private func selectScene(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let kind = WindowSceneKind(rawValue: raw) else { return }
        windows.selectScene(kind)
    }

    @objc private func selectVisualization(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String, let kind = VisualizationKind(rawValue: raw) else { return }
        player.visualization = kind
    }
}

extension AppDelegate: NSMenuItemValidation {
    /// Checks the active visualization and scene in the menu; everything else stays enabled.
    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if menuItem.action == #selector(selectVisualization(_:)) {
            menuItem.state = (menuItem.representedObject as? String) == player.visualization.rawValue ? .on : .off
        }
        if menuItem.action == #selector(selectScene(_:)) {
            menuItem.state = (menuItem.representedObject as? String) == SceneHostView.savedKind.rawValue ? .on : .off
        }
        return true
    }
}

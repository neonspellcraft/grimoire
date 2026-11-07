import AppKit
import GrimoireCore
import MetalKit

/// Metal view that fills the visualizer window. Each frame it reads audio from the tap, splits it into three
/// bands, and hands that to the current `WindowScene`.
///
/// Drag anywhere to move the window, double-click for full screen, right-click for scene options.
/// Rendering pauses while the window is hidden or fully covered.
final class SceneHostView: MTKView, MTKViewDelegate {
    private enum Keys {
        static let scene = "windowScene"
        static let meters = "showBandMeters"
    }

    private weak var player: PlayerController?
    private let reader: TapReader
    private let analyzer = MultibandAnalyzer()
    private var follower = BandFollower()
    private let commandQueue: MTLCommandQueue
    private var scene: WindowScene?
    private var kind: WindowSceneKind
    private var showMeters: Bool
    private var lastTimestamp: CFTimeInterval?
    private var occlusionObserver: NSObjectProtocol?

    init?(player: PlayerController) {
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else { return nil }
        self.player = player
        reader = TapReader(tap: player.audioTap)
        commandQueue = queue
        kind = UserDefaults.standard.string(forKey: Keys.scene).flatMap(WindowSceneKind.init) ?? .threeBody
        showMeters = UserDefaults.standard.bool(forKey: Keys.meters)
        super.init(frame: .zero, device: device)
        colorPixelFormat = .bgra8Unorm
        framebufferOnly = true
        preferredFramesPerSecond = 60
        clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        delegate = self
        loadScene()
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError() }

    private func loadScene() {
        do {
            scene = try kind.make(device: device!, colorPixelFormat: colorPixelFormat)
        } catch {
            NSLog("Grimoire: couldn't build the \(kind.title) scene: \(error)")
            scene = nil
        }
    }

    // MARK: Rendering

    func draw(in view: MTKView) {
        let now = CACurrentMediaTime()
        let dt = lastTimestamp.map { min(max(now - $0, 0), 0.1) } ?? 1.0 / 60
        lastTimestamp = now

        let playing = player?.state == .playing
        let raw: BandLevels? = reader.read(dt: dt, playing: playing)
            ? reader.samples.withUnsafeBufferPointer { analyzer.levels(from: $0, sampleRate: reader.sampleRate) }
            : nil
        follower.step(raw, dt: dt)

        guard let scene else { return }
        scene.update(audio: SceneAudio(levels: follower.envelope, hits: follower.hits), dt: dt)
        guard let pass = currentRenderPassDescriptor, let drawable = currentDrawable,
              let commands = commandQueue.makeCommandBuffer()
        else { return }
        scene.encode(into: commands, target: pass, drawableSize: drawableSize, showMeters: showMeters)
        commands.present(drawable)
        commands.commit()
    }

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let occlusionObserver { NotificationCenter.default.removeObserver(occlusionObserver) }
        occlusionObserver = nil
        guard let window else { return }
        occlusionObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, let window = self.window else { return }
                self.isPaused = !window.occlusionState.contains(.visible)
                self.lastTimestamp = nil
            }
        }
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 {
            window?.toggleFullScreen(nil)
        } else {
            window?.performDrag(with: event)
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        for option in WindowSceneKind.allCases {
            let item = NSMenuItem(title: option.title, action: #selector(chooseScene(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = option.rawValue
            item.state = option == kind ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let reseed = NSMenuItem(title: kind.reseedTitle, action: #selector(reseedScene), keyEquivalent: "")
        reseed.target = self
        menu.addItem(reseed)
        let meters = NSMenuItem(title: "Show Band Meters", action: #selector(toggleMeters), keyEquivalent: "")
        meters.target = self
        meters.state = showMeters ? .on : .off
        menu.addItem(meters)
        menu.addItem(.separator())
        let fullScreen = NSMenuItem(title: "Toggle Full Screen", action: #selector(NSWindow.toggleFullScreen(_:)), keyEquivalent: "")
        fullScreen.target = window
        menu.addItem(fullScreen)
        return menu
    }

    @objc private func chooseScene(_ item: NSMenuItem) {
        guard let raw = item.representedObject as? String, let option = WindowSceneKind(rawValue: raw), option != kind else { return }
        kind = option
        UserDefaults.standard.set(option.rawValue, forKey: Keys.scene)
        loadScene()
    }

    @objc private func reseedScene() {
        scene?.reseed()
    }

    @objc private func toggleMeters() {
        showMeters.toggle()
        UserDefaults.standard.set(showMeters, forKey: Keys.meters)
    }
}

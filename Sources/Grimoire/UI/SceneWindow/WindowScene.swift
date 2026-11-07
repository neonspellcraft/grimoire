import GrimoireCore
import Metal

/// What every window scene gets each frame: the three bands, smoothed and auto-gained, plus transient hits.
struct SceneAudio {
    var levels: BandLevels
    var hits: BandLevels
}

/// A full-window, GPU-rendered visualizer. To add one: write a class conforming to this,
/// then add a case to `WindowSceneKind`.
@MainActor
protocol WindowScene: AnyObject {
    func update(audio: SceneAudio, dt: Double)
    /// Encodes this frame's rendering, finishing in `target` (the window's drawable).
    func encode(into commands: MTLCommandBuffer, target: MTLRenderPassDescriptor, drawableSize: CGSize, showMeters: Bool)
    /// Starts over from a fresh, slightly randomized state.
    func reseed()
}

enum WindowSceneKind: String, CaseIterable {
    case threeBody
    case viewscreen
    case spellrun

    var title: String {
        switch self {
        case .threeBody: "Three-Body"
        case .viewscreen: "Viewscreen"
        case .spellrun: "Spellrun"
        }
    }

    /// Menu title for `reseed()`.
    var reseedTitle: String {
        switch self {
        case .threeBody: "New Orbit"
        case .viewscreen: "New Course"
        case .spellrun: "New World"
        }
    }

    @MainActor
    func make(device: MTLDevice, colorPixelFormat: MTLPixelFormat) throws -> WindowScene {
        switch self {
        case .threeBody: try ThreeBodyScene(device: device, colorPixelFormat: colorPixelFormat)
        case .viewscreen: try ViewscreenScene(device: device, colorPixelFormat: colorPixelFormat)
        case .spellrun: try SpellrunScene(device: device, colorPixelFormat: colorPixelFormat)
        }
    }
}

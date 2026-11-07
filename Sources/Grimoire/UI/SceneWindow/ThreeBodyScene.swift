import GrimoireCore
import Metal
import simd

/// The three-body problem as a music-driven light show.
///
/// Each body belongs to one band: low, mid and high. Its mass follows that band, bass also pumps the strength
/// of gravity, and the mids set how fast simulated time runs. The figure-eight orbit drifts into chaos as the
/// music pushes the masses around. The shader turns the bodies' potential field into flowing rings with feedback trails.
final class ThreeBodyScene: WindowScene {
    private struct Uniforms {
        var view: SIMD4<Float>
        var body0: SIMD4<Float>
        var body1: SIMD4<Float>
        var body2: SIMD4<Float>
        var bands: SIMD4<Float>
        var hits: SIMD4<Float>
        var feedback: SIMD4<Float>
    }

    private let device: MTLDevice
    private let evolvePipeline: MTLRenderPipelineState
    private let presentPipeline: MTLRenderPipelineState
    /// Ping-pong feedback buffers at half the drawable's resolution.
    private var textures: [MTLTexture] = []
    private var current = 0

    private var system = ThreeBodyScene.freshSystem()
    /// Energy of the unmodulated figure-eight; the thermostat target scales from this.
    private let baselineEnergy = ThreeBodyScene.freshSystem().totalEnergy
    private var audio = SceneAudio(levels: .zero, hits: .zero)
    private var flow: Double = 0
    private var hue: Double = 0
    private var cameraScale: Double = 0.7
    private var frameScale: Double = 1

    init(device: MTLDevice, colorPixelFormat: MTLPixelFormat) throws {
        self.device = device
        let library = try device.makeLibrary(source: ThreeBodyShaders.source, options: nil)
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "fullscreen")
        descriptor.fragmentFunction = library.makeFunction(name: "evolve")
        descriptor.colorAttachments[0].pixelFormat = .rgba16Float
        evolvePipeline = try device.makeRenderPipelineState(descriptor: descriptor)
        descriptor.fragmentFunction = library.makeFunction(name: "present")
        descriptor.colorAttachments[0].pixelFormat = colorPixelFormat
        presentPipeline = try device.makeRenderPipelineState(descriptor: descriptor)
    }

    func update(audio: SceneAudio, dt: Double) {
        self.audio = audio
        let low = Double(audio.levels.low), mid = Double(audio.levels.mid), high = Double(audio.levels.high)

        let masses = [1 + 0.8 * low, 1 + 0.8 * mid, 1 + 0.8 * high]
        system.gravity = 1 + 0.9 * low + 0.5 * Double(audio.hits.low)
        system.setMasses(masses)
        // Energy scales with G·m² at a fixed size, so this holds the orbit's size while the music pumps it.
        let meanMass = masses.reduce(0, +) / 3
        system.targetEnergy = baselineEnergy * system.gravity * meanMass * meanMass
        system.advance(by: dt * (0.45 + 0.9 * mid + 0.3 * high))
        if system.extent > system.wallRadius * 1.8 || !system.extent.isFinite { reseed() }

        flow += dt * (0.5 + 1.8 * mid + 1.2 * low)
        hue += dt * (0.015 + 0.12 * mid + 0.2 * Double(audio.hits.mid))
        // Keep the whole system in view, easing the zoom so it doesn't jitter.
        let targetScale = 0.8 / max(system.extent, 0.3)
        cameraScale += (targetScale - cameraScale) * min(1, dt * 1.2)
        frameScale = dt * 60
    }

    func reseed() {
        var next = Self.freshSystem()
        next.rotate(by: .random(in: 0..<(2 * .pi)))
        next.perturb(0.04)
        system = next
    }

    /// The figure-eight, softened and in a gentle bowl so all three bodies keep dancing instead of decaying into
    /// a tight pair and a loner (the usual fate of a chaotic three-body system).
    private static func freshSystem() -> ThreeBodySystem {
        var system = ThreeBodySystem.figureEight()
        system.softening = 0.15
        system.trapStrength = 1
        system.maxKineticEnergy = 30
        return system
    }

    func encode(into commands: MTLCommandBuffer, target: MTLRenderPassDescriptor, drawableSize: CGSize, showMeters: Bool) {
        let width = max(Int(drawableSize.width / 2), 16)
        let height = max(Int(drawableSize.height / 2), 16)
        if textures.first?.width != width || textures.first?.height != height {
            makeTextures(width: width, height: height, commands: commands)
        }
        let source = textures[current]
        let destination = textures[1 - current]

        var uniforms = makeUniforms(
            resolution: SIMD2(Float(width), Float(height)),
            aspect: Float(drawableSize.width / max(drawableSize.height, 1)),
            showMeters: showMeters)

        let evolvePass = MTLRenderPassDescriptor()
        evolvePass.colorAttachments[0].texture = destination
        evolvePass.colorAttachments[0].loadAction = .dontCare
        evolvePass.colorAttachments[0].storeAction = .store
        if let encoder = commands.makeRenderCommandEncoder(descriptor: evolvePass) {
            encoder.setRenderPipelineState(evolvePipeline)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
            encoder.setFragmentTexture(source, index: 0)
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            encoder.endEncoding()
        }

        uniforms.view.x = Float(drawableSize.width)
        uniforms.view.y = Float(drawableSize.height)
        if let encoder = commands.makeRenderCommandEncoder(descriptor: target) {
            encoder.setRenderPipelineState(presentPipeline)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
            encoder.setFragmentTexture(destination, index: 0)
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            encoder.endEncoding()
        }
        current = 1 - current
    }

    private func makeUniforms(resolution: SIMD2<Float>, aspect: Float, showMeters: Bool) -> Uniforms {
        let levels = audio.levels, hits = audio.hits
        func body(_ index: Int) -> SIMD4<Float> {
            let b = system.bodies[index]
            return SIMD4(Float(b.position.x * cameraScale), Float(b.position.y * cameraScale), Float(b.mass), levels[index])
        }
        // Per-frame feedback amounts, normalized to 60 fps so the look doesn't depend on frame rate.
        let zoom = 1 - (0.0012 + 0.004 * Double(levels.low) + 0.02 * Double(hits.low)) * frameScale
        let rotation = (0.001 + 0.006 * Double(levels.mid)) * frameScale
        let decay = pow(0.90 - 0.06 * Double(levels.high), frameScale)
        return Uniforms(
            view: SIMD4(resolution.x, resolution.y, Float(flow), aspect),
            body0: body(0), body1: body(1), body2: body(2),
            bands: SIMD4(levels.low, levels.mid, levels.high, levels.loudest),
            hits: SIMD4(hits.low, hits.mid, hits.high, Float(hue)),
            feedback: SIMD4(Float(zoom), Float(rotation), Float(decay), showMeters ? 1 : 0))
    }

    private func makeTextures(width: Int, height: Int, commands: MTLCommandBuffer) {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .rgba16Float, width: width, height: height, mipmapped: false)
        descriptor.usage = [.renderTarget, .shaderRead]
        descriptor.storageMode = .private
        textures = (0..<2).compactMap { _ in device.makeTexture(descriptor: descriptor) }
        // New textures hold garbage; clear them so the feedback starts from black.
        for texture in textures {
            let clear = MTLRenderPassDescriptor()
            clear.colorAttachments[0].texture = texture
            clear.colorAttachments[0].loadAction = .clear
            clear.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
            clear.colorAttachments[0].storeAction = .store
            commands.makeRenderCommandEncoder(descriptor: clear)?.endEncoding()
        }
        current = 0
    }
}

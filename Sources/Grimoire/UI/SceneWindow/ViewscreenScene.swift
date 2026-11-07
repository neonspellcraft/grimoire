import GrimoireCore
import Metal
import simd

/// Colors for the Viewscreen scene. Change these to restyle it.
struct ViewscreenPalette {
    /// Deep navy at the center of the view, fading to near-black at the edges.
    var space = SIMD3<Float>(0.012, 0.018, 0.045)
    var spaceEdge = SIMD3<Float>(0.0, 0.002, 0.010)
    /// White-blue starlight.
    var star = SIMD3<Float>(0.78, 0.87, 1.0)
    var nebulaTeal = SIMD3<Float>(0.04, 0.55, 0.58)
    var nebulaMagenta = SIMD3<Float>(0.72, 0.10, 0.52)
    var nebulaViolet = SIMD3<Float>(0.38, 0.18, 0.85)
    /// HUD lines and readouts on the glass.
    var hud = SIMD3<Float>(0.55, 0.80, 1.0)
    /// Rim light on planets.
    var atmosphere = SIMD3<Float>(0.40, 0.70, 1.0)
    /// Surface color pairs for passing bodies; one pair is picked at random each time.
    var planets: [(SIMD3<Float>, SIMD3<Float>)] = [
        (SIMD3(0.55, 0.32, 0.42), SIMD3(0.86, 0.68, 0.55)),   // dusty rose and sand
        (SIMD3(0.10, 0.28, 0.48), SIMD3(0.45, 0.72, 0.78)),   // ocean blues
        (SIMD3(0.30, 0.18, 0.50), SIMD3(0.78, 0.55, 0.85)),   // violet giant
    ]
    var moons: [(SIMD3<Float>, SIMD3<Float>)] = [
        (SIMD3(0.30, 0.30, 0.33), SIMD3(0.70, 0.70, 0.72)),
        (SIMD3(0.36, 0.30, 0.26), SIMD3(0.72, 0.64, 0.56)),
    ]
    var asteroids: [(SIMD3<Float>, SIMD3<Float>)] = [
        (SIMD3(0.22, 0.20, 0.19), SIMD3(0.50, 0.46, 0.42)),
    ]

    static let standard = ViewscreenPalette()
}

/// The view from a starship's bridge, with the music as the engine.
///
/// - LOW is the throttle: a laggy, smoothed speed drives a layered 3D starfield, and stars stretch into warp
///   streaks as it rises. It never drops below a slow drift.
/// - MID is the environment: the nebula's opacity and saturation follow it, and its hue drifts.
/// - HIGH is sparkle: near stars twinkle, shooting stars appear, and strong hits glint across the glass.
///
/// Every 30–90 s a planet, moon or asteroid field drifts past on one side. All state is allocated up front;
/// each frame rewrites star data into one of three reused GPU buffers.
final class ViewscreenScene: WindowScene {
    private struct Uniforms {
        var view: SIMD4<Float>
        var camera: SIMD4<Float>
        var motion: SIMD4<Float>
        var bands: SIMD4<Float>
        var hits: SIMD4<Float>
        var nebula: SIMD4<Float>
        var body: SIMD4<Float>
        var bodyLook: SIMD4<Float>
        var bodyColorA: SIMD4<Float>
        var bodyColorB: SIMD4<Float>
    }

    private struct GPUPalette {
        var space, spaceEdge, star, nebulaA, nebulaB, nebulaC, hud, atmosphere: SIMD4<Float>
    }

    private struct Star {
        var position: SIMD4<Float>   // xyz world, layer speed multiplier
        var look: SIMD4<Float>       // size px, twinkle, tint, brightness
    }

    /// Star depth layers, drawn far to near. Near layers move faster than their depth alone would make them,
    /// which exaggerates the parallax.
    private struct Layer {
        let count: Int
        let near: Float
        let far: Float
        let speed: Float
        let size: Float
        let brightness: Float
    }

    private static let layers = [
        Layer(count: 900, near: 40, far: 80, speed: 0.25, size: 0.9, brightness: 0.55),
        Layer(count: 600, near: 20, far: 40, speed: 0.45, size: 1.2, brightness: 0.70),
        Layer(count: 400, near: 8, far: 20, speed: 0.70, size: 1.6, brightness: 0.85),
        Layer(count: 220, near: 1.5, far: 8, speed: 1.00, size: 2.2, brightness: 1.00),
    ]
    /// Stars in these first layers are drawn behind a passing body; the rest in front of it.
    private static let backLayerCount = 2

    private enum BodyKind: Float {
        case planet = 1, moon = 2, asteroids = 3
    }

    private struct PassingBody {
        var kind: BodyKind
        var position: SIMD3<Float>
        var radius: Float
        var speed: Float
        var colors: (SIMD3<Float>, SIMD3<Float>)
        var hasRing: Bool
        var seed: Float
        var lightAngle: Float
        /// Asteroid field only: rock offsets from `position` (xyz) and radius (w), sorted far to near.
        var rocks: [SIMD4<Float>]
    }

    private struct Meteor {
        var head = SIMD2<Float>.zero
        var velocity = SIMD2<Float>.zero
        var age: Float = 0
        var life: Float = 0
        var length: Float = 0
        var isActive: Bool { age < life }
    }

    let palette: ViewscreenPalette
    private let starPipeline: MTLRenderPipelineState
    private let nebulaPipeline: MTLRenderPipelineState
    private let bodyPipeline: MTLRenderPipelineState
    private let overlayPipeline: MTLRenderPipelineState
    private let starBuffers: [MTLBuffer]
    private var bufferIndex = 0

    private var stars: [Star] = []
    private var starLayer: [Int] = []
    private var meteors = [Meteor](repeating: Meteor(), count: 3)
    private var rockScratch = [SIMD4<Float>](repeating: .zero, count: 16)
    private var meteorScratch = [SIMD4<Float>](repeating: .zero, count: 6)
    private var body: PassingBody?
    /// The first body shows up fairly soon so it's discoverable; after that every 30–90 s.
    private var nextBodyIn: Double = 15

    private var audio = SceneAudio(levels: .zero, hits: .zero)
    private var time: Double = 0
    private var throttle: Double = 0
    private var sustainedLow: Double = 0
    private var heading = Double.random(in: 0..<360)
    private var glint: Double = 0
    private var nebulaDrift = SIMD2<Double>(.random(in: 0..<50), .random(in: 0..<50))
    private var hue: Double = 0
    private var roll: Double = 0
    private var twinkleBudget: Double = 0
    private var lastHighHit: Float = 0
    private var aspect: Float = 1.6

    init(device: MTLDevice, colorPixelFormat: MTLPixelFormat, palette: ViewscreenPalette = .standard) throws {
        self.palette = palette
        let library = try device.makeLibrary(source: ViewscreenShaders.source, options: nil)

        func pipeline(vertex: String, fragment: String, blend: (MTLBlendFactor, MTLBlendFactor)?) throws -> MTLRenderPipelineState {
            let descriptor = MTLRenderPipelineDescriptor()
            descriptor.vertexFunction = library.makeFunction(name: vertex)
            descriptor.fragmentFunction = library.makeFunction(name: fragment)
            let attachment = descriptor.colorAttachments[0]!
            attachment.pixelFormat = colorPixelFormat
            if let (source, destination) = blend {
                attachment.isBlendingEnabled = true
                attachment.sourceRGBBlendFactor = source
                attachment.sourceAlphaBlendFactor = source
                attachment.destinationRGBBlendFactor = destination
                attachment.destinationAlphaBlendFactor = destination
            }
            return try device.makeRenderPipelineState(descriptor: descriptor)
        }
        nebulaPipeline = try pipeline(vertex: "fullscreen", fragment: "nebulaFragment", blend: nil)
        starPipeline = try pipeline(vertex: "starVertex", fragment: "starFragment", blend: (.one, .one))
        bodyPipeline = try pipeline(vertex: "fullscreen", fragment: "bodyFragment", blend: (.one, .oneMinusSourceAlpha))
        overlayPipeline = try pipeline(vertex: "fullscreen", fragment: "overlayFragment", blend: (.one, .oneMinusSourceAlpha))

        let starCount = Self.layers.reduce(0) { $0 + $1.count }
        let length = starCount * MemoryLayout<Star>.stride
        // Three buffers so the CPU never writes one the GPU is still reading (MTKView keeps ≤ 3 frames in flight).
        starBuffers = try (0..<3).map { _ in
            guard let buffer = device.makeBuffer(length: length, options: .storageModeShared) else {
                throw ViewscreenError.bufferAllocationFailed
            }
            return buffer
        }
        seedStars()
    }

    // MARK: Simulation

    func update(audio: SceneAudio, dt: Double) {
        self.audio = audio
        time += dt
        let low = Double(audio.levels.low), mid = Double(audio.levels.mid), high = Double(audio.levels.high)

        // Throttle: rises over ~0.35 s and falls over ~1.6 s, so kicks feel like thrust surges.
        let target = min(0.05 + 0.8 * low + 0.45 * Double(audio.hits.low), 1.2)
        throttle += (target - throttle) * (1 - exp(-dt / (target > throttle ? 0.35 : 1.6)))
        sustainedLow += (low - sustainedLow) * (1 - exp(-dt / 3))
        let speed = Float(1.2 + throttle * 48)

        // Ship drift: slow sway and roll, banking a little on sustained bass.
        roll = 0.012 * sin(time * 0.11) + 0.007 * sin(time * 0.23 + 1)
            + 0.05 * sustainedLow * sin(time * 0.05 + 0.6)
        heading = (heading + dt * (0.6 + 40 * roll)).truncatingRemainder(dividingBy: 360)
        if heading < 0 { heading += 360 }

        nebulaDrift += SIMD2(0.006, 0.003) * dt * (1 + 2 * mid) + SIMD2(0, 0.0004) * Double(speed) * dt
        hue += dt * (0.01 + 0.05 * mid)

        updateStars(speed: speed, dt: Float(dt))
        updateSparkle(high: high, dt: dt)
        updateBody(speed: speed, dt: Float(dt))
    }

    func reseed() {
        heading = .random(in: 0..<360)
        seedStars()
        body = nil
        nextBodyIn = 3
    }

    private func seedStars() {
        stars.removeAll(keepingCapacity: true)
        starLayer.removeAll(keepingCapacity: true)
        for (index, layer) in Self.layers.enumerated() {
            for _ in 0..<layer.count {
                var star = Star(position: .zero, look: .zero)
                respawn(&star, in: layer, anywhere: true)
                stars.append(star)
                starLayer.append(index)
            }
        }
    }

    private func respawn(_ star: inout Star, in layer: Layer, anywhere: Bool) {
        let z = anywhere ? Float.random(in: layer.near...layer.far) : layer.far - Float.random(in: 0...(0.08 * (layer.far - layer.near)))
        let coverX = aspect + 0.25
        star.position = SIMD4(
            Float.random(in: -coverX...coverX) * z, Float.random(in: -1.25...1.25) * z, z, layer.speed)
        star.look = SIMD4(layer.size * Float.random(in: 0.7...1.3), 0, Float.random(in: 0...0.35), 0)
    }

    private func updateStars(speed: Float, dt: Float) {
        let coverX = aspect + 0.3
        for i in stars.indices {
            let layer = Self.layers[starLayer[i]]
            var star = stars[i]
            star.position.z -= speed * layer.speed * dt
            let z = star.position.z
            if z < layer.near || abs(star.position.x / z) > coverX || abs(star.position.y / z) > 1.3 {
                respawn(&star, in: layer, anywhere: false)
            }
            // Fade in at the far edge of the layer so respawns don't pop.
            let fadeIn = simd_smoothstep(layer.far, layer.far - 0.2 * (layer.far - layer.near), star.position.z)
            star.look.w = layer.brightness * fadeIn
            star.look.y = max(0, star.look.y - dt / 0.3)
            stars[i] = star
        }
    }

    private func updateSparkle(high: Double, dt: Double) {
        let highHit = audio.hits.high
        let struck = highHit > 0.9 && lastHighHit <= 0.9
        lastHighHit = highHit

        // Twinkles on the near layers: a burst on each hit, plus a trickle that follows the highs.
        let nearStart = Self.layers.prefix(2).reduce(0) { $0 + $1.count }
        twinkleBudget += high * 20 * dt + (struck ? 8 : 0)
        while twinkleBudget >= 1 {
            stars[Int.random(in: nearStart..<stars.count)].look.y = 1
            twinkleBudget -= 1
        }

        if struck, glint < 0.2, Double.random(in: 0..<1) < 0.4 { glint = 1 }
        glint = max(0, glint - dt / 1.2)

        let spawnChance = (0.02 + 0.35 * high) * dt + (struck ? 0.3 : 0)
        if Double.random(in: 0..<1) < spawnChance, let slot = meteors.firstIndex(where: { !$0.isActive }) {
            let direction: Float = Bool.random() ? 1 : -1
            let angle = Float.random(in: 0.35...0.7)
            meteors[slot] = Meteor(
                head: SIMD2(Float.random(in: -aspect...aspect), Float.random(in: 0.1...0.9)),
                velocity: SIMD2(direction * cos(angle), -sin(angle)) * Float.random(in: 1.6...2.6),
                age: 0, life: Float.random(in: 0.55...1.0), length: Float.random(in: 0.25...0.45))
        }
        for i in meteors.indices where meteors[i].isActive {
            meteors[i].age += Float(dt)
            meteors[i].head += meteors[i].velocity * Float(dt)
        }
    }

    private func updateBody(speed: Float, dt: Float) {
        guard var current = body else {
            nextBodyIn -= Double(dt)
            if nextBodyIn <= 0 { body = makeBody() }
            return
        }
        current.position.z -= (current.speed + speed * 0.05) * dt
        let z = current.position.z
        let reach = current.kind == .asteroids ? current.radius + 5 : current.radius * (current.hasRing ? 2.4 : 1.1)
        if z < 1 || abs(current.position.x / z) - reach / z > aspect + 0.1 {
            body = nil
            nextBodyIn = .random(in: 30...90)
        } else {
            body = current
        }
    }

    private func makeBody() -> PassingBody {
        let roll = Double.random(in: 0..<1)
        let kind: BodyKind = roll < 0.45 ? .planet : roll < 0.75 ? .moon : .asteroids
        let side: Float = Bool.random() ? 1 : -1
        let radius: Float
        let colors: (SIMD3<Float>, SIMD3<Float>)
        switch kind {
        case .planet:
            radius = .random(in: 7...11)
            colors = palette.planets.randomElement()!
        case .moon:
            radius = .random(in: 3.5...5.5)
            colors = palette.moons.randomElement()!
        case .asteroids:
            radius = 1
            colors = palette.asteroids.randomElement()!
        }
        let offset: Float = kind == .asteroids ? .random(in: 7...10) : radius * 1.3 + .random(in: 1...4)
        let rocks: [SIMD4<Float>] = kind != .asteroids ? [] : (0..<16).map { _ in
            SIMD4(.random(in: -5...5), .random(in: -3...3), .random(in: -6...6), .random(in: 0.25...0.9))
        }.sorted { $0.z > $1.z }
        return PassingBody(
            kind: kind,
            position: SIMD3(side * offset, .random(in: -0.35...0.35) * max(radius, 4), 70),
            radius: radius, speed: .random(in: 1.8...2.8), colors: colors,
            hasRing: kind == .planet && Bool.random(), seed: .random(in: 0..<100),
            lightAngle: side > 0 ? .random(in: 2.4...3.4) : .random(in: -0.5...0.5),
            rocks: rocks)
    }

    // MARK: Rendering

    func encode(into commands: MTLCommandBuffer, target: MTLRenderPassDescriptor, drawableSize: CGSize, showMeters: Bool) {
        aspect = Float(drawableSize.width / max(drawableSize.height, 1))
        let buffer = starBuffers[bufferIndex]
        bufferIndex = (bufferIndex + 1) % starBuffers.count
        stars.withUnsafeBytes { buffer.contents().copyMemory(from: $0.baseAddress!, byteCount: $0.count) }

        var uniforms = makeUniforms(drawableSize: drawableSize, showMeters: showMeters)
        var gpuPalette = makeGPUPalette()
        fillRocks()
        fillMeteors()

        guard let encoder = commands.makeRenderCommandEncoder(descriptor: target) else { return }
        let uniformsLength = MemoryLayout<Uniforms>.stride
        let paletteLength = MemoryLayout<GPUPalette>.stride

        encoder.setRenderPipelineState(nebulaPipeline)
        encoder.setFragmentBytes(&uniforms, length: uniformsLength, index: 0)
        encoder.setFragmentBytes(&gpuPalette, length: paletteLength, index: 1)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)

        let backCount = Self.layers.prefix(Self.backLayerCount).reduce(0) { $0 + $1.count }
        encoder.setRenderPipelineState(starPipeline)
        encoder.setVertexBuffer(buffer, offset: 0, index: 0)
        encoder.setVertexBytes(&uniforms, length: uniformsLength, index: 1)
        encoder.setVertexBytes(&gpuPalette, length: paletteLength, index: 2)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: backCount)

        if body != nil {
            encoder.setRenderPipelineState(bodyPipeline)
            encoder.setFragmentBytes(&uniforms, length: uniformsLength, index: 0)
            encoder.setFragmentBytes(&gpuPalette, length: paletteLength, index: 1)
            rockScratch.withUnsafeBytes { encoder.setFragmentBytes($0.baseAddress!, length: $0.count, index: 2) }
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        }

        encoder.setRenderPipelineState(starPipeline)
        encoder.setVertexBufferOffset(backCount * MemoryLayout<Star>.stride, index: 0)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: stars.count - backCount)

        encoder.setRenderPipelineState(overlayPipeline)
        encoder.setFragmentBytes(&uniforms, length: uniformsLength, index: 0)
        encoder.setFragmentBytes(&gpuPalette, length: paletteLength, index: 1)
        meteorScratch.withUnsafeBytes { encoder.setFragmentBytes($0.baseAddress!, length: $0.count, index: 2) }
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding()
    }

    private func makeUniforms(drawableSize: CGSize, showMeters: Bool) -> Uniforms {
        let levels = audio.levels, hits = audio.hits
        let swayX = Float(0.010 * sin(time * 0.07) + 0.004 * sin(time * 0.17 + 2))
        let swayY = Float(0.008 * sin(time * 0.09 + 2) + 0.003 * sin(time * 0.21))
        let streak = Float(1.2 + throttle * 48) * 0.06

        var bodyData = SIMD4<Float>.zero
        var bodyLook = SIMD4<Float>.zero
        var colorA = SIMD4<Float>.zero, colorB = SIMD4<Float>.zero
        if let body {
            let z = body.position.z
            bodyData = SIMD4(body.position.x / z, body.position.y / z, body.radius / z, body.kind.rawValue)
            bodyLook = SIMD4(body.hasRing ? 1 : 0, body.seed, body.lightAngle, 0)
            colorA = SIMD4(body.colors.0, 1)
            colorB = SIMD4(body.colors.1, 1)
        }
        return Uniforms(
            view: SIMD4(Float(drawableSize.width), Float(drawableSize.height), aspect, Float(time)),
            camera: SIMD4(Float(roll), swayX, swayY, streak),
            motion: SIMD4(Float(throttle), Float(heading), Float(glint), 0),
            bands: SIMD4(levels.low, levels.mid, levels.high, Float(sustainedLow)),
            hits: SIMD4(hits.low, hits.mid, hits.high, showMeters ? 1 : 0),
            nebula: SIMD4(Float(nebulaDrift.x), Float(nebulaDrift.y), Float(hue), 0),
            body: bodyData, bodyLook: bodyLook, bodyColorA: colorA, bodyColorB: colorB)
    }

    private func makeGPUPalette() -> GPUPalette {
        GPUPalette(
            space: SIMD4(palette.space, 1), spaceEdge: SIMD4(palette.spaceEdge, 1), star: SIMD4(palette.star, 1),
            nebulaA: SIMD4(palette.nebulaTeal, 1), nebulaB: SIMD4(palette.nebulaMagenta, 1),
            nebulaC: SIMD4(palette.nebulaViolet, 1), hud: SIMD4(palette.hud, 1),
            atmosphere: SIMD4(palette.atmosphere, 1))
    }

    /// Projects the asteroid field's rocks into screen space (xy, radius, seed); unused slots get radius 0.
    private func fillRocks() {
        for i in rockScratch.indices { rockScratch[i] = .zero }
        guard let body, body.kind == .asteroids else { return }
        for (i, rock) in body.rocks.enumerated() where i < rockScratch.count {
            let z = body.position.z + rock.z
            guard z > 0.5 else { continue }
            rockScratch[i] = SIMD4((body.position.x + rock.x) / z, (body.position.y + rock.y) / z, rock.w / z, Float(i) * 7.31)
        }
    }

    /// Each meteor is two float4s: head (xy, brightness) and tail (xy).
    private func fillMeteors() {
        for (i, meteor) in meteors.enumerated() {
            guard meteor.isActive else {
                meteorScratch[i * 2] = .zero
                meteorScratch[i * 2 + 1] = .zero
                continue
            }
            let progress = meteor.age / meteor.life
            let brightness = min(progress * 6, 1) * (1 - progress)
            let tail = meteor.head - simd_normalize(meteor.velocity) * meteor.length * min(progress * 4, 1)
            meteorScratch[i * 2] = SIMD4(lowHalf: meteor.head, highHalf: SIMD2(brightness, 0))
            meteorScratch[i * 2 + 1] = SIMD4(lowHalf: tail, highHalf: .zero)
        }
    }
}

enum ViewscreenError: Error {
    case bufferAllocationFailed
}

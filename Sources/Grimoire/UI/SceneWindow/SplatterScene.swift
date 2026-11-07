import GrimoireCore
import Metal
import simd

/// Colors for the Splatter scene. Each splat picks from a small group of neighboring swatches, and the group
/// drifts over time, so the painting goes through color phases instead of being uniformly rainbow.
struct SplatterPalette {
    var swatches: [SIMD3<Float>] = [
        SIMD3(1.00, 0.18, 0.53),   // hot pink
        SIMD3(1.00, 0.48, 0.00),   // laser orange
        SIMD3(1.00, 0.95, 0.00),   // highlighter yellow
        SIMD3(0.49, 1.00, 0.00),   // acid green
        SIMD3(0.00, 0.94, 1.00),   // electric cyan
        SIMD3(0.64, 0.23, 1.00),   // UV purple
    ]
    /// How many neighboring swatches are in play at once.
    var groupSize = 3

    static let standard = SplatterPalette()
}

/// Neon paint thrown at a black canvas by the music.
///
/// Every splat is shaped by a piece of theoretical physics (see `Theory`): superstring harmonics, quantum orbitals,
/// Einstein rings, inflation fields, Kerr black holes, Lorenz attractors, Feynman diagrams, double-slit fringes
/// and Brownian fractals. All parameters are fresh continuous random values, so no two splats are ever the same.
///
/// - LOW: kicks throw big cosmic splats (Einstein ring, inflation, Kerr, superstring, Brownian), sized by the hit.
/// - MID: quantum and particle splats on mid hits (orbitals, Feynman diagrams); drips run longer and faster with
///   the mids; the color group drifts with them.
/// - HIGH: Lorenz attractors, double-slit fringes and brush flicks, plus a fine speckle mist.
///
/// Paint accumulates on a persistent canvas. It glistens while wet, dries, and slowly fades so the painting keeps
/// evolving. A drop triggers a pour, and the old paint dries off faster to make room.
final class SplatterScene: WindowScene {
    private struct Uniforms {
        var view, dry, bands: SIMD4<Float>
    }

    private struct Stamp {
        var shape: SIMD4<Float>   // center xy, radius, rotation
        var color: SIMD4<Float>   // rgb, seed
        var style: SIMD4<Float>   // kind (0 blob, 1 round), stretch
    }

    private struct Drip {
        var position = SIMD2<Float>.zero
        var speed: Float = 0
        var radius: Float = 0
        var remaining: Float = 0
        var color = SIMD3<Float>.zero
        var isActive: Bool { remaining > 0 }
    }

    private static let maxStampsPerFrame = 3072

    let palette: SplatterPalette
    private let device: MTLDevice
    private let dryPipeline: MTLRenderPipelineState
    private let stampPipeline: MTLRenderPipelineState
    private let presentPipeline: MTLRenderPipelineState
    private let stampBuffers: [MTLBuffer]
    private var bufferIndex = 0
    /// Ping-pong canvases (float, mipmapped for the bloom).
    private var canvases: [MTLTexture] = []
    private var current = 0
    private var needsClear = true

    private var stamps: [Stamp] = []
    private var drips = [Drip](repeating: Drip(), count: 256)
    private var dripCursor = 0
    private var audio = SceneAudio(levels: .zero, hits: .zero)
    private var time: Double = 0
    private var aspect: Float = 1.6
    private var lastHits = BandLevels.zero
    private var lastSplat = SIMD2<Float>.zero
    private var lastSplatRadius: Float = 0.1
    private var colorBase = Double.random(in: 0..<6)
    private var mistBudget: Double = 0
    private var ambientBudget: Double = 0
    private var fastLevel: Double = 0
    private var slowLevel: Double = 0
    private var lastPour: Double = -100
    /// Seconds left of fast drying after a pour.
    private var wipe: Double = 0

    init(device: MTLDevice, colorPixelFormat: MTLPixelFormat, palette: SplatterPalette = .standard) throws {
        precondition(!palette.swatches.isEmpty, "SplatterPalette needs at least one swatch")
        self.device = device
        self.palette = palette
        let library = try device.makeLibrary(source: SplatterShaders.source, options: nil)

        let dry = MTLRenderPipelineDescriptor()
        dry.vertexFunction = library.makeFunction(name: "fullscreen")
        dry.fragmentFunction = library.makeFunction(name: "dryFragment")
        dry.colorAttachments[0].pixelFormat = .rgba16Float
        dryPipeline = try device.makeRenderPipelineState(descriptor: dry)

        // Color goes "over" the old paint; wetness (alpha) keeps whichever is wetter.
        let stamp = MTLRenderPipelineDescriptor()
        stamp.vertexFunction = library.makeFunction(name: "stampVertex")
        stamp.fragmentFunction = library.makeFunction(name: "stampFragment")
        let attachment = stamp.colorAttachments[0]!
        attachment.pixelFormat = .rgba16Float
        attachment.isBlendingEnabled = true
        attachment.sourceRGBBlendFactor = .sourceAlpha
        attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
        attachment.rgbBlendOperation = .add
        attachment.sourceAlphaBlendFactor = .one
        attachment.destinationAlphaBlendFactor = .one
        attachment.alphaBlendOperation = .max
        stampPipeline = try device.makeRenderPipelineState(descriptor: stamp)

        let present = MTLRenderPipelineDescriptor()
        present.vertexFunction = library.makeFunction(name: "fullscreen")
        present.fragmentFunction = library.makeFunction(name: "presentFragment")
        present.colorAttachments[0].pixelFormat = colorPixelFormat
        presentPipeline = try device.makeRenderPipelineState(descriptor: present)

        let length = Self.maxStampsPerFrame * MemoryLayout<Stamp>.stride
        stampBuffers = try (0..<3).map { _ in
            guard let buffer = device.makeBuffer(length: length, options: .storageModeShared) else {
                throw SplatterError.bufferAllocationFailed
            }
            return buffer
        }
        stamps.reserveCapacity(Self.maxStampsPerFrame)
    }

    // MARK: Painting

    func update(audio: SceneAudio, dt: Double) {
        self.audio = audio
        time += dt
        let levels = audio.levels, hits = audio.hits
        let low = Double(levels.low), mid = Double(levels.mid), high = Double(levels.high)
        func struck(_ band: Int) -> Bool { hits[band] > 0.9 && lastHits[band] <= 0.9 }
        let kick = struck(0), midHit = struck(1), highHit = struck(2)
        lastHits = hits

        colorBase += dt * (0.02 + 0.12 * mid)

        let loudness = Double(levels.loudest)
        fastLevel += (loudness - fastLevel) * (1 - exp(-dt / 2))
        slowLevel += (loudness - slowLevel) * (1 - exp(-dt / 8))
        if fastLevel > 0.6, slowLevel < 0.4, time - lastPour > 12 {
            lastPour = time
            pour(mid: mid)
        }

        // Heavy, cosmic theories on the kicks; quantum and particle physics on the mids; chaos and waves on the highs.
        if kick {
            paint(pick([.einsteinRing, .inflation, .kerrBlackHole, .superstring, .brownianFractal]), at: randomSpot(),
                  radius: Float(0.05 + 0.09 * low + 0.03 * Double(hits.low)), mid: mid, energy: low)
        }
        if midHit {
            paint(pick([.quantumOrbital, .feynmanDiagram, .superstring, .brownianFractal, .inflation]), at: randomSpot(),
                  radius: Float(0.035 + 0.045 * mid), mid: mid, energy: mid)
        }
        if highHit {
            paint(pick([.lorenzAttractor, .doubleSlit, .brushFlick]), at: randomSpot(),
                  radius: Float(0.05 + 0.06 * high), mid: mid, energy: high)
        }

        // Even without hits, sustained mids keep a light patter of small splats going.
        ambientBudget += dt * 0.3 * mid
        if ambientBudget >= 1 {
            ambientBudget -= 1
            paint(pick([.superstring, .quantumOrbital, .inflation, .brownianFractal]), at: randomSpot(),
                  radius: Float(0.02 + 0.025 * mid), mid: mid, energy: 0.3)
        }
        mistBudget += dt * 50 * high * high
        while mistBudget >= 1 {
            mistBudget -= 1
            let angle = Float.random(in: 0..<(2 * .pi)), distance = lastSplatRadius * Float.random(in: 1.2...4)
            addRound(at: lastSplat + SIMD2(cos(angle), sin(angle)) * distance,
                     radius: Float.random(in: 0.0012...0.003), color: pickColor())
        }

        updateDrips(dt: Float(dt), mid: Float(mid))
        wipe = max(0, wipe - dt)
    }

    func reseed() {
        needsClear = true
        for i in drips.indices { drips[i].remaining = 0 }
    }

    private func randomSpot() -> SIMD2<Float> {
        SIMD2(Float.random(in: -0.85...0.85) * aspect, Float.random(in: -0.8...0.85))
    }

    private func pickColor() -> SIMD3<Float> {
        let count = palette.swatches.count
        let offset = Int(Double.random(in: 0..<Double(min(palette.groupSize, count))))
        return palette.swatches[(Int(colorBase) + offset) % count]
    }

    /// The physics each splat is shaped by. Every splat draws fresh continuous random parameters (phases, spectra,
    /// quantum numbers, chaotic initial conditions, rotation, color blend), so no two are ever the same.
    enum Theory: CaseIterable {
        case superstring, quantumOrbital, einsteinRing, inflation, kerrBlackHole
        case lorenzAttractor, feynmanDiagram, doubleSlit, brownianFractal, brushFlick
    }

    private var lastTheory: Theory?

    /// Picks one of `options`, never the same theory twice in a row.
    private func pick(_ options: [Theory]) -> Theory {
        let fresh = options.filter { $0 != lastTheory }
        let theory = (fresh.isEmpty ? options : fresh).randomElement()!
        lastTheory = theory
        return theory
    }

    private func paint(_ theory: Theory, at center: SIMD2<Float>, radius r: Float, mid: Double, energy: Double) {
        let color = uniqueColor()
        let spin = Float.random(in: 0..<(2 * .pi))
        let seed = Float.random(in: 0..<1000)
        switch theory {
        case .superstring:
            // A closed string vibrating in 6...16 overtones; a steep spectrum is smooth, a shallow one jagged.
            let tilt = Float.random(in: 0.9...2.2), modes = Float(Int.random(in: 6...16))
            blob(0, at: center, radius: r, spin: spin, color: color, seed: seed, params: SIMD2(tilt, modes))
            harmonicSpray(at: center, radius: r, color: color, modes: Int(modes), phase: seed, count: 10 + Int(16 * energy))
            drips(from: center, radius: r, color: color, mid: mid)
        case .quantumOrbital:
            let m = Float(Int.random(in: 1...4)), sharpness = Float.random(in: 0.6...3)
            blob(2, at: center, radius: r, spin: spin, color: color, seed: seed, params: SIMD2(sharpness, m))
            probabilityCloud(at: center, radius: r, color: uniqueColor(), m: Int(m), phase: seed + spin, count: 50 + Int(60 * energy))
            drips(from: center, radius: r * 0.6, color: color, mid: mid * 0.6)
        case .einsteinRing:
            blob(3, at: center, radius: r, spin: spin, color: color, seed: seed, params: SIMD2(.random(in: 0.7...1.4), 0))
            lensedArcs(at: center, radius: r, color: color, axis: seed)
            drips(from: center, radius: r * 0.3, color: color, mid: mid)
        case .inflation:
            blob(4, at: center, radius: r * 1.2, spin: spin, color: color, seed: seed,
                 params: SIMD2(.random(in: 0.05...0.25), .random(in: 1.4...3.2)))
            powerLawSpeckles(at: center, radius: r, color: color, count: 40 + Int(50 * energy))
            drips(from: center, radius: r * 0.8, color: color, mid: mid)
        case .kerrBlackHole:
            let arms = Float(Int.random(in: 2...5)), twist = Float.random(in: 1.2...3.5) * (Bool.random() ? 1 : -1)
            blob(5, at: center, radius: r * 1.15, spin: spin, color: color, seed: seed, params: SIMD2(arms, twist))
            spiralStream(at: center, radius: r, color: color, arms: Int(arms), twist: twist, phase: seed, count: 30 + Int(30 * energy))
        case .lorenzAttractor:
            // Line-based theories need more room than a blob to read clearly.
            lorenz(at: center, radius: r * 2.1, color: color, spin: spin)
        case .feynmanDiagram:
            feynman(at: center, radius: r * 1.8, color: color, lineColor: uniqueColor(), spin: spin, mid: mid)
        case .doubleSlit:
            interference(at: center, radius: r * 1.5, color: color, spin: spin, count: 90 + Int(70 * energy))
        case .brownianFractal:
            blob(0, at: center, radius: r * 0.8, spin: spin, color: color, seed: seed, params: SIMD2(1.6, 9))
            brownianBranches(at: center, radius: r, color: color)
            drips(from: center, radius: r * 0.8, color: color, mid: mid)
        case .brushFlick:
            flick(at: center, color: color, energy: energy)
        }
        lastSplat = center
        lastSplatRadius = r
    }

    /// The group's swatch, blended a random amount toward a neighbor and brightened or dimmed a little.
    private func uniqueColor() -> SIMD3<Float> {
        let blend = SIMD3<Float>(repeating: .random(in: 0...0.45))
        return simd_mix(pickColor(), pickColor(), blend) * Float.random(in: 0.88...1.1)
    }

    // MARK: Theory painters

    private func blob(_ kind: Float, at center: SIMD2<Float>, radius: Float, spin: Float, color: SIMD3<Float>,
                      seed: Float, params: SIMD2<Float>) {
        addStamp(Stamp(shape: SIMD4(center, radius, spin), color: SIMD4(color, seed), style: SIMD4(kind, 1, params.x, params.y)))
    }

    private func droplet(at position: SIMD2<Float>, radius: Float, color: SIMD3<Float>, angle: Float = 0, stretch: Float = 1) {
        addStamp(Stamp(shape: SIMD4(position, max(radius, 0.0012), angle), color: SIMD4(color, 0), style: SIMD4(1, stretch, 0, 0)))
    }

    private func polar(_ angle: Float, _ distance: Float) -> SIMD2<Float> {
        SIMD2(cos(angle), sin(angle)) * distance
    }

    /// Droplets flung outward, most often along the string's strongest overtone.
    private func harmonicSpray(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, modes: Int, phase: Float, count: Int) {
        let n = Float(Int.random(in: 2...max(2, modes / 2)))
        var placed = 0
        while placed < count {
            let angle = Float.random(in: 0..<(2 * .pi))
            guard Float.random(in: 0...1) < 0.35 + 0.65 * (0.5 + 0.5 * sin(n * angle + phase)) else { continue }
            let distance = r * Float.random(in: 1.1...2.8)
            let size = r * Float.random(in: 0.03...0.12) * (2.8 - distance / r) / 1.7
            droplet(at: center + polar(angle, distance), radius: size, color: color, angle: angle, stretch: .random(in: 1.3...3.2))
            placed += 1
        }
    }

    /// |ψ|² for a hydrogen-like state in the plane: r^2l e^(-2r/n) cos²(mθ), sampled by rejection.
    private func probabilityCloud(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, m: Int, phase: Float, count: Int) {
        let n = Float(m + 1), l = Float(m)
        let peak = pow(l * n, 2 * l) * exp(-2 * l)   // the radial density's maximum, at ρ = l·n
        var placed = 0, attempts = 0
        while placed < count, attempts < count * 40 {
            attempts += 1
            let rho = Float.random(in: 0...(4 * n)), theta = Float.random(in: 0..<(2 * .pi))
            let density = pow(rho, 2 * l) * exp(-2 * rho / n) * pow(cos(Float(m) * (theta - phase)), 2)
            guard Float.random(in: 0...max(peak, 1e-6)) < density else { continue }
            droplet(at: center + polar(theta, r * 0.55 * rho / n), radius: .random(in: 0.0013...0.0032), color: color)
            placed += 1
        }
    }

    /// Secondary lensed images: short bright arcs at wider radii along the lens axis.
    private func lensedArcs(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, axis: Float) {
        for image in 0..<Int.random(in: 2...4) {
            let base = axis + (image.isMultiple(of: 2) ? 0 : .pi) + .random(in: -0.4...0.4)
            let distance = r * Float.random(in: 1.35...2.1), span = Float.random(in: 0.25...0.7)
            let steps = Int(distance * span / 0.004) + 4
            for i in 0..<steps {
                let angle = base - span / 2 + span * Float(i) / Float(steps - 1)
                let taper = sin(.pi * Float(i) / Float(steps - 1))
                droplet(at: center + polar(angle, distance), radius: r * 0.05 * taper + 0.0012, color: color)
            }
        }
    }

    /// Primordial-style speckles: Gaussian scatter with power-law sizes (lots of small, a few large).
    private func powerLawSpeckles(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, count: Int) {
        for _ in 0..<count {
            let u = Float.random(in: 0.02...1), v = Float.random(in: 0..<(2 * .pi))
            let distance = r * 1.4 * sqrt(-2 * log(Float.random(in: 0.0001...1)))
            let size = min(0.0012 * pow(u, -0.65), r * 0.12)
            droplet(at: center + polar(v, distance), radius: size, color: color)
        }
    }

    /// Matter flung outward along logarithmic spirals, stretched along the flow.
    private func spiralStream(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, arms: Int, twist: Float, phase: Float, count: Int) {
        for _ in 0..<count {
            let arm = Float(Int.random(in: 0..<arms))
            let distance = r * Float.random(in: 1.0...2.6)
            let angle = (arm * 2 * .pi - phase) / Float(arms) + twist * log(distance / r) + .random(in: -0.08...0.08)
            let tangent = angle + .pi / 2 * (twist > 0 ? 1 : -1)
            droplet(at: center + polar(angle, distance), radius: r * Float.random(in: 0.02...0.06) * (2.8 - distance / r),
                    color: color, angle: tangent, stretch: .random(in: 1.5...3.5))
        }
    }

    /// A Lorenz-attractor orbit from a random start with randomized ρ and β, projected onto a random plane.
    private func lorenz(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, spin: Float) {
        let sigma: Float = 10, rho = Float.random(in: 24...32), beta = Float.random(in: 2.4...2.9)
        var p = SIMD3<Float>(.random(in: -5...5), .random(in: -5...5), .random(in: 10...30))
        let dt: Float = 0.008
        func derivative(_ p: SIMD3<Float>) -> SIMD3<Float> {
            SIMD3(sigma * (p.y - p.x), p.x * (rho - p.z) - p.y, p.x * p.y - beta * p.z)
        }
        for _ in 0..<250 { p += derivative(p) * dt }   // settle onto the attractor
        let plane = Int.random(in: 0...1)
        let scale = r * 1.3 / 25
        let c = cos(spin), sn = sin(spin)
        for step in 0..<640 {
            let velocity = derivative(p)
            p += velocity * dt
            guard step % 4 == 0 else { continue }
            let flat = plane == 0 ? SIMD2(p.x, p.z - rho + 3) : SIMD2(p.y, p.z - rho + 3)
            let rotated = SIMD2(c * flat.x - sn * flat.y, sn * flat.x + c * flat.y) * scale
            let speed = min(simd_length(velocity) / 160, 1)
            droplet(at: center + rotated, radius: r * (0.018 + 0.04 * (1 - speed)), color: color)
        }
    }

    /// Two vertices joined by a wavy photon propagator, four fermion legs, and sometimes a vacuum-polarization loop.
    private func feynman(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, lineColor: SIMD3<Float>, spin: Float, mid: Double) {
        let half = polar(spin, r * Float.random(in: 0.7...1.0))
        let a = center - half, b = center + half
        let normal = SIMD2(-sin(spin), cos(spin))
        let waves = Float(Int.random(in: 3...6)), amplitude = r * Float.random(in: 0.1...0.2)
        for i in 0...48 {
            let t = Float(i) / 48
            droplet(at: a + (b - a) * t + normal * sin(t * waves * 2 * .pi) * amplitude, radius: r * 0.035, color: lineColor)
        }
        for (vertex, outward) in [(a, spin + .pi), (b, spin)] {
            blob(0, at: vertex, radius: r * 0.26, spin: .random(in: 0..<(2 * .pi)), color: color,
                 seed: .random(in: 0..<1000), params: SIMD2(1.8, 8))
            for side: Float in [-1, 1] {
                let angle = outward + side * Float.random(in: 0.45...0.9)
                let length = r * Float.random(in: 0.9...1.5)
                for i in 1...20 {
                    droplet(at: vertex + polar(angle, length * Float(i) / 20), radius: r * 0.03, color: color)
                }
            }
            drips(from: vertex, radius: r * 0.25, color: color, mid: mid)
        }
        if Bool.random() {
            let loopCenter = center + normal * r * 0.55, loopRadius = r * 0.25
            for i in 0..<28 {
                droplet(at: loopCenter + polar(Float(i) / 28 * 2 * .pi, loopRadius), radius: r * 0.028, color: lineColor)
            }
        }
    }

    /// Droplets landing in double-slit fringes: cos² fringes under a sinc² envelope, sampled by rejection.
    private func interference(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>, spin: Float, count: Int) {
        let spacing = r * Float.random(in: 0.22...0.45), envelope = r * Float.random(in: 1.0...1.6)
        let c = cos(spin), sn = sin(spin)
        var placed = 0, attempts = 0
        while placed < count, attempts < count * 30 {
            attempts += 1
            let x = Float.random(in: -2.6...2.6) * r
            let u = .pi * x / envelope
            let sinc = abs(u) < 1e-4 ? 1 : sin(u) / u
            guard Float.random(in: 0...1) < pow(cos(.pi * x / spacing), 2) * sinc * sinc else { continue }
            let y = Float.random(in: -0.6...0.6) * r
            droplet(at: center + SIMD2(c * x - sn * y, sn * x + c * y), radius: .random(in: 0.0014...0.0034), color: color)
            placed += 1
        }
    }

    /// Branching random walks growing outward from the rim, like diffusion-limited aggregation.
    private func brownianBranches(at center: SIMD2<Float>, radius r: Float, color: SIMD3<Float>) {
        func grow(from start: SIMD2<Float>, heading: Float, size: Float, depth: Int) {
            var position = start, direction = heading, width = size
            for _ in 0..<Int.random(in: 14...30) {
                direction += Float.random(in: -0.45...0.45)
                position += polar(direction, r * 0.055)
                width *= 0.95
                droplet(at: position, radius: width, color: color)
                if depth < 2, Float.random(in: 0...1) < 0.07 {
                    grow(from: position, heading: direction + Float.random(in: -0.9...0.9), size: width * 0.8, depth: depth + 1)
                }
            }
        }
        for _ in 0..<Int.random(in: 5...9) {
            let angle = Float.random(in: 0..<(2 * .pi))
            grow(from: center + polar(angle, r * 0.7), heading: angle, size: r * 0.07, depth: 0)
        }
    }

    /// A flick of the brush: a curved line of droplets that shrink along the arc.
    private func flick(at start: SIMD2<Float>, color: SIMD3<Float>, energy: Double) {
        let heading = Float.random(in: 0..<(2 * .pi)), bend = Float.random(in: -1.2...1.2)
        let length = Float(0.35 + 0.5 * energy)
        let count = 16 + Int(24 * energy)
        for i in 0..<count {
            let t = Float(i) / Float(count - 1)
            let angle = heading + bend * t
            let jitter = SIMD2<Float>(.random(in: -1...1), .random(in: -1...1)) * 0.008
            droplet(at: start + polar(angle, length * t) + jitter, radius: Float.random(in: 0.002...0.006) * (1.2 - t),
                    color: color, angle: angle, stretch: .random(in: 1...2.2))
        }
    }

    private func drips(from center: SIMD2<Float>, radius: Float, color: SIMD3<Float>, mid: Double) {
        for _ in 0..<Int(Double.random(in: 0...1) + mid * 3.5) {
            let x = center.x + radius * Float.random(in: -0.7...0.7)
            startDrip(at: SIMD2(x, center.y - radius * Float.random(in: 0.5...0.85)),
                      radius: radius * Float.random(in: 0.06...0.14), color: color, mid: Float(mid))
        }
    }

    /// A drop: a cluster of four different theories at once, and the old paint dries off faster to make room.
    private func pour(mid: Double) {
        wipe = 1.2
        let center = SIMD2<Float>(.random(in: -0.4...0.4) * aspect, .random(in: -0.2...0.3))
        for theory in Theory.allCases.filter({ $0 != .brushFlick }).shuffled().prefix(4) {
            let offset = SIMD2<Float>(.random(in: -0.3...0.3), .random(in: -0.22...0.22))
            paint(theory, at: center + offset, radius: .random(in: 0.1...0.18), mid: max(mid, 0.8), energy: 1)
        }
    }

    private func startDrip(at position: SIMD2<Float>, radius: Float, color: SIMD3<Float>, mid: Float) {
        drips[dripCursor] = Drip(position: position, speed: Float.random(in: 0.05...0.12) * (0.6 + 1.4 * mid),
                                 radius: max(radius, 0.002), remaining: Float.random(in: 0.08...0.45) * (0.5 + mid),
                                 color: color)
        dripCursor = (dripCursor + 1) % drips.count
    }

    /// Drips run down, slowing and thinning. Each frame they stamp along their path, so the trail is unbroken.
    private func updateDrips(dt: Float, mid: Float) {
        for i in drips.indices where drips[i].isActive {
            var drip = drips[i]
            let travel = min(drip.speed * dt, drip.remaining)
            let steps = max(1, Int(ceil(travel / (drip.radius * 0.6))))
            for step in 1...steps {
                let y = drip.position.y - travel * Float(step) / Float(steps)
                addRound(at: SIMD2(drip.position.x, y), radius: drip.radius, color: drip.color)
            }
            drip.position.y -= travel
            drip.remaining -= travel
            drip.speed *= 1 - 0.35 * dt
            drip.radius *= 1 - 0.25 * dt
            // A bead forms at the end of the run.
            if drip.remaining <= 0 { addRound(at: drip.position, radius: drip.radius * 1.5, color: drip.color) }
            drips[i] = drip
        }
    }

    private func addRound(at position: SIMD2<Float>, radius: Float, color: SIMD3<Float>) {
        addStamp(Stamp(shape: SIMD4(position, radius, 0), color: SIMD4(color, 0), style: SIMD4(1, 1, 0, 0)))
    }

    private func addStamp(_ stamp: Stamp) {
        guard stamps.count < Self.maxStampsPerFrame else { return }
        stamps.append(stamp)
    }

    // MARK: Rendering

    func encode(into commands: MTLCommandBuffer, target: MTLRenderPassDescriptor, drawableSize: CGSize, showMeters: Bool) {
        aspect = Float(drawableSize.width / max(drawableSize.height, 1))
        let width = max(Int(drawableSize.width), 16), height = max(Int(drawableSize.height), 16)
        if canvases.first?.width != width || canvases.first?.height != height { makeCanvases(width: width, height: height) }

        let source = canvases[current], destination = canvases[1 - current]
        // Paint dries in a few tenths of a second and loses half its brightness in about 4 s, keeping plenty of black
        // canvas showing; a pour fades it much faster.
        let colorDecay = Float(wipe > 0 ? 0.95 : 0.997)
        var uniforms = Uniforms(
            view: SIMD4(Float(width), Float(height), aspect, Float(time)),
            dry: SIMD4(colorDecay, 0.965, Float(0.85 + 0.7 * Double(audio.levels.low)), showMeters ? 1 : 0),
            bands: SIMD4(audio.levels.low, audio.levels.mid, audio.levels.high, 0))

        let paintPass = MTLRenderPassDescriptor()
        paintPass.colorAttachments[0].texture = destination
        paintPass.colorAttachments[0].loadAction = needsClear ? .clear : .dontCare
        paintPass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        paintPass.colorAttachments[0].storeAction = .store
        if let encoder = commands.makeRenderCommandEncoder(descriptor: paintPass) {
            if !needsClear {
                encoder.setRenderPipelineState(dryPipeline)
                encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
                encoder.setFragmentTexture(source, index: 0)
                encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            }
            if !stamps.isEmpty {
                let buffer = stampBuffers[bufferIndex]
                bufferIndex = (bufferIndex + 1) % stampBuffers.count
                stamps.withUnsafeBytes { buffer.contents().copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
                encoder.setRenderPipelineState(stampPipeline)
                encoder.setVertexBuffer(buffer, offset: 0, index: 0)
                encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
                encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: stamps.count)
            }
            encoder.endEncoding()
        }
        stamps.removeAll(keepingCapacity: true)
        needsClear = false

        if let blit = commands.makeBlitCommandEncoder() {
            blit.generateMipmaps(for: destination)
            blit.endEncoding()
        }

        if let encoder = commands.makeRenderCommandEncoder(descriptor: target) {
            encoder.setRenderPipelineState(presentPipeline)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
            encoder.setFragmentTexture(destination, index: 0)
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            encoder.endEncoding()
        }
        current = 1 - current
    }

    private func makeCanvases(width: Int, height: Int) {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba16Float, width: width, height: height, mipmapped: true)
        descriptor.usage = [.renderTarget, .shaderRead]
        descriptor.storageMode = .private
        canvases = (0..<2).compactMap { _ in device.makeTexture(descriptor: descriptor) }
        current = 0
        needsClear = true
    }
}

enum SplatterError: Error {
    case bufferAllocationFailed
}

import GrimoireCore
import Metal

/// Spellrun: a generated pixel platformer that the music plays.
///
/// The game and its renderer live in GrimoireCore (`SpellrunGame`, `SpellrunRenderer`). This scene turns the
/// band envelopes and hits into game input, renders a low-resolution frame on the CPU, and has Metal scale it up
/// by a whole number so every game pixel stays perfectly sharp.
///
/// - LOW: kicks make the apprentice hop when the ground ahead is safe; big kicks mid-air become ground pounds;
///   beat lanterns and rune blocks glow with the bass.
/// - MID: run speed and how lively the generated terrain is (gaps, hills, platforms, blocks).
/// - HIGH: spell bolts, motes dropping out of the air, twinkling stars and the staff gem's glow.
final class SpellrunScene: WindowScene {
    /// About how many game pixels tall the view is; the exact size follows from a whole-number scale.
    private static let targetHeight = 160.0

    private let device: MTLDevice
    private let pipeline: MTLRenderPipelineState
    private var game = SpellrunGame()
    private let renderer = SpellrunRenderer()
    private var canvas = PixelCanvas(width: 256, height: 160)
    /// Three textures in rotation so the CPU never overwrites one the GPU is still drawing from.
    private var textures: [MTLTexture] = []
    private var textureIndex = 0
    private var scale = 5

    private var audio = SceneAudio(levels: .zero, hits: .zero)
    private var lastLowHit: Float = 0
    private var lastHighHit: Float = 0
    private var fastLevel: Double = 0
    private var slowLevel: Double = 0

    init(device: MTLDevice, colorPixelFormat: MTLPixelFormat) throws {
        self.device = device
        let library = try device.makeLibrary(source: Self.shaderSource, options: nil)
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "fullscreen")
        descriptor.fragmentFunction = library.makeFunction(name: "present")
        descriptor.colorAttachments[0].pixelFormat = colorPixelFormat
        pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
    }

    func update(audio: SceneAudio, dt: Double) {
        self.audio = audio
        let kick = audio.hits.low > 0.9 && lastLowHit <= 0.9
        let sparkle = audio.hits.high > 0.9 && lastHighHit <= 0.9
        lastLowHit = audio.hits.low
        lastHighHit = audio.hits.high

        let loudness = Double(audio.levels.loudest)
        fastLevel += (loudness - fastLevel) * (1 - exp(-dt / 2))
        slowLevel += (loudness - slowLevel) * (1 - exp(-dt / 8))
        let drop = fastLevel > 0.6 && slowLevel < 0.4

        let input = SpellrunInput(
            low: audio.levels.low, mid: audio.levels.mid, high: audio.levels.high,
            kick: kick, bigKick: kick && audio.levels.low > 0.75, sparkle: sparkle, drop: drop)
        game.step(dt: dt, input: input, viewWidth: Double(canvas.width), viewHeight: Double(canvas.height))
    }

    func reseed() {
        game = SpellrunGame()
    }

    func encode(into commands: MTLCommandBuffer, target: MTLRenderPassDescriptor, drawableSize: CGSize, showMeters: Bool) {
        resizeIfNeeded(drawableSize)
        renderer.render(game, into: canvas, levels: audio.levels, showMeters: showMeters)

        let texture = textures[textureIndex]
        textureIndex = (textureIndex + 1) % textures.count
        canvas.pixels.withUnsafeBytes {
            texture.replace(region: MTLRegionMake2D(0, 0, canvas.width, canvas.height), mipmapLevel: 0,
                            withBytes: $0.baseAddress!, bytesPerRow: canvas.width * 4)
        }

        guard let encoder = commands.makeRenderCommandEncoder(descriptor: target) else { return }
        var uniforms = SIMD4<Float>(Float(scale), Float(canvas.width), Float(canvas.height), 0)
        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentTexture(texture, index: 0)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<SIMD4<Float>>.stride, index: 0)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding()
    }

    /// Picks a whole-number scale near 160 game pixels tall, so each game pixel is an exact square of screen pixels.
    private func resizeIfNeeded(_ drawableSize: CGSize) {
        let newScale = max(2, Int((Double(drawableSize.height) / Self.targetHeight).rounded()))
        let width = max(64, Int((Double(drawableSize.width) / Double(newScale)).rounded(.up)))
        let height = max(64, Int((Double(drawableSize.height) / Double(newScale)).rounded(.up)))
        guard textures.isEmpty || width != canvas.width || height != canvas.height || newScale != scale else { return }
        scale = newScale
        canvas = PixelCanvas(width: width, height: height)
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: false)
        descriptor.usage = .shaderRead
        textures = (0..<3).compactMap { _ in device.makeTexture(descriptor: descriptor) }
        textureIndex = 0
    }

    private static let shaderSource = """
    #include <metal_stdlib>
    using namespace metal;

    struct VertexOut { float4 position [[position]]; };

    vertex VertexOut fullscreen(uint id [[vertex_id]]) {
        float2 corner = float2((id << 1) & 2, id & 2);
        VertexOut out;
        out.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
        return out;
    }

    // Each game pixel covers an exact scale×scale block of screen pixels, plus a whisper of scanline.
    fragment float4 present(VertexOut in [[stage_in]],
                            texture2d<float, access::read> image [[texture(0)]],
                            constant float4& u [[buffer(0)]]) {
        float scale = u.x;
        uint2 texel = min(uint2(in.position.xy / scale), uint2(u.y - 1.0, u.z - 1.0));
        float3 color = image.read(texel).rgb;
        if (scale >= 3.0 && fmod(in.position.y, scale) >= scale - max(1.0, floor(scale / 4.0))) {
            color *= 0.9;
        }
        return float4(color, 1.0);
    }
    """
}

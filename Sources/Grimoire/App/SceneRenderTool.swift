import AVFoundation
import GrimoireCore
import ImageIO
import Metal
import UniformTypeIdentifiers

/// Developer tool: plays a track through the real band analysis and a real visualizer scene, rendering offscreen
/// to PNG frames. No window is needed, so scenes can be checked without taking over the screen.
///
///     Grimoire --render-scene splatter --track song.aiff --out frames/ [--seconds 40] [--every 5] [--size 1280x800]
enum SceneRenderTool {
    static func runIfRequested() -> Bool {
        let arguments = CommandLine.arguments
        guard let flag = arguments.firstIndex(of: "--render-scene"), flag + 1 < arguments.count else { return false }
        func value(_ name: String) -> String? {
            arguments.firstIndex(of: name).flatMap { $0 + 1 < arguments.count ? arguments[$0 + 1] : nil }
        }
        do {
            guard let kind = WindowSceneKind(rawValue: arguments[flag + 1]) else {
                throw ToolError("Unknown scene '\(arguments[flag + 1])'. Scenes: \(WindowSceneKind.allCases.map(\.rawValue).joined(separator: ", "))")
            }
            guard let track = value("--track"), let output = value("--out") else { throw ToolError("Needs --track and --out") }
            let size = (value("--size") ?? "1280x800").split(separator: "x").compactMap { Int($0) }
            try MainActor.assumeIsolated {
                try render(kind: kind, track: URL(fileURLWithPath: track), output: URL(fileURLWithPath: output),
                           seconds: Double(value("--seconds") ?? "") ?? 40, every: Double(value("--every") ?? "") ?? 5,
                           width: size.first ?? 1280, height: size.count > 1 ? size[1] : 800)
            }
        } catch {
            FileHandle.standardError.write(Data("render-scene: \(error)\n".utf8))
            exit(1)
        }
        return true
    }

    @MainActor
    private static func render(kind: WindowSceneKind, track: URL, output: URL, seconds: Double, every: Double, width: Int, height: Int) throws {
        guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else { throw ToolError("No Metal device") }
        let file = try AVAudioFile(forReading: track)
        let format = file.processingFormat
        let frameCount = AVAudioFrameCount(min(Double(file.length), seconds * format.sampleRate))
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameCount) else { throw ToolError("Couldn't allocate audio") }
        try file.read(into: buffer, frameCount: frameCount)
        var mono = [Float](repeating: 0, count: Int(buffer.frameLength))
        for channel in 0..<Int(format.channelCount) {
            let data = buffer.floatChannelData![channel]
            for i in mono.indices { mono[i] += data[i] / Float(format.channelCount) }
        }

        let scene = try kind.make(device: device, colorPixelFormat: .bgra8Unorm)
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
        descriptor.usage = [.renderTarget, .shaderRead]
        descriptor.storageMode = .managed
        guard let target = device.makeTexture(descriptor: descriptor) else { throw ToolError("Couldn't make the render target") }
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

        let analyzer = MultibandAnalyzer()
        var follower = BandFollower()
        let dt = 1.0 / 60
        let hop = Int(format.sampleRate * dt)
        let framesPerShot = max(1, Int(every / dt))
        var frame = 0, end = 2048, saved = 0
        while end <= mono.count {
            let levels = mono.withUnsafeBufferPointer {
                analyzer.levels(from: UnsafeBufferPointer(rebasing: $0[(end - 2048)..<end]), sampleRate: format.sampleRate)
            }
            follower.step(levels, dt: dt)
            scene.update(audio: SceneAudio(levels: follower.envelope, hits: follower.hits), dt: dt)

            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = target
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].storeAction = .store
            guard let commands = queue.makeCommandBuffer() else { break }
            scene.encode(into: commands, target: pass, drawableSize: CGSize(width: width, height: height), showMeters: true)
            let shoot = frame > 0 && frame % framesPerShot == 0
            if shoot, let blit = commands.makeBlitCommandEncoder() {
                blit.synchronize(resource: target)
                blit.endEncoding()
            }
            commands.commit()
            if shoot {
                commands.waitUntilCompleted()
                try savePNG(target, to: output.appendingPathComponent(String(format: "%@_%03ds.png", kind.rawValue, Int(Double(frame) * dt))))
                saved += 1
            }
            end += hop
            frame += 1
        }
        print("Rendered \(frame) frames of \(kind.title); saved \(saved) PNGs to \(output.path)")
    }

    private static func savePNG(_ texture: MTLTexture, to url: URL) throws {
        let width = texture.width, height = texture.height
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        texture.getBytes(&bytes, bytesPerRow: width * 4, from: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0)
        let info = CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
        guard let provider = CGDataProvider(data: Data(bytes) as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: info, provider: provider,
                                  decode: nil, shouldInterpolate: false, intent: .defaultIntent),
              let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { throw ToolError("Couldn't encode \(url.lastPathComponent)") }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw ToolError("Couldn't write \(url.path)") }
    }

    private struct ToolError: Error, CustomStringConvertible {
        let description: String
        init(_ description: String) { self.description = description }
    }
}

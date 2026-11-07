import AVFoundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import GrimoireCore

final class LevelGeneratorTests: XCTestCase {
    /// Every gap and step must be clearable: gaps ≤ 3 wide, steps up ≤ 2, wide gaps never land higher.
    func testGeneratedTerrainIsAlwaysPassable() {
        for seed: UInt64 in [1, 42, 9_001] {
            for energy in [0.0, 0.5, 1.0] {
                var generator = LevelGenerator(seed: seed)
                generator.energy = energy
                let columns = (0..<3_000).map { _ in generator.next() }
                var lastGround = columns[0].ground
                var gap = 0
                for column in columns {
                    if column.ground == 0 {
                        gap += 1
                        XCTAssertLessThanOrEqual(gap, LevelGenerator.maxGap, "seed \(seed) energy \(energy)")
                        continue
                    }
                    let rise = column.ground - lastGround
                    XCTAssertLessThanOrEqual(rise, gap >= LevelGenerator.maxGap ? 0 : LevelGenerator.maxStepUp)
                    XCTAssertGreaterThanOrEqual(column.ground, LevelGenerator.minHeight)
                    XCTAssertLessThanOrEqual(column.ground, LevelGenerator.maxHeight)
                    lastGround = column.ground
                    gap = 0
                }
            }
        }
    }

    func testSameSeedSameLevel() {
        var a = LevelGenerator(seed: 7), b = LevelGenerator(seed: 7)
        for _ in 0..<500 {
            let x = a.next(), y = b.next()
            XCTAssertEqual(x.ground, y.ground)
            XCTAssertEqual(x.block, y.block)
        }
    }

    func testBusyMusicMakesBusierTerrain() {
        func features(_ energy: Double) -> Int {
            var generator = LevelGenerator(seed: 3)
            generator.energy = energy
            return (0..<2_000).map { _ in generator.next() }.filter { $0.ground == 0 || $0.platform >= 0 || $0.block >= 0 }.count
        }
        XCTAssertGreaterThan(features(1), features(0))
    }
}

final class SpellrunGameTests: XCTestCase {
    /// Two minutes of music-like input: the apprentice keeps moving forward and never falls into a pit.
    func testApprenticeSurvivesAndMakesProgress() {
        for seed: UInt64 in [1, 2, 3, 4, 5] {
            let game = SpellrunGame(seed: seed)
            var frame = 0
            while frame < 60 * 120 {
                let t = Double(frame) / 60
                let beat = frame % 30 == 0
                let input = SpellrunInput(
                    low: Float(0.5 + 0.5 * sin(t * 2)), mid: Float(0.5 + 0.5 * sin(t * 0.3)), high: Float(0.5 + 0.5 * sin(t * 1.7)),
                    kick: beat, bigKick: beat && frame % 120 == 0, sparkle: frame % 45 == 0, drop: frame == 60 * 40)
                game.step(dt: 1.0 / 60, input: input, viewWidth: 256, viewHeight: 160)
                frame += 1
            }
            XCTAssertEqual(game.respawns, 0, "seed \(seed): the apprentice fell into a pit")
            XCTAssertGreaterThan(game.hero.x, 95 * 100, "seed \(seed): should cover ground at run speed")
            XCTAssertTrue(game.hero.y.isFinite)
            XCTAssertGreaterThan(game.motesCollected, 0)
        }
    }

    /// Renders a frame after a few seconds of play. Set SPELLRUN_PREVIEW=/path/to/file.png to save it.
    func testRendersAFrame() throws {
        let game = SpellrunGame(seed: 11)
        for frame in 0..<(60 * 6) {
            let input = SpellrunInput(low: 0.7, mid: 0.6, high: 0.5, kick: frame % 28 == 0, sparkle: frame % 40 == 0)
            game.step(dt: 1.0 / 60, input: input, viewWidth: 256, viewHeight: 160)
        }
        let canvas = PixelCanvas(width: 256, height: 160)
        SpellrunRenderer().render(game, into: canvas, levels: BandLevels(low: 0.7, mid: 0.6, high: 0.5), showMeters: true)
        XCTAssertGreaterThan(Set(canvas.pixels).count, 20, "a real frame has lots of colors")

        guard let path = ProcessInfo.processInfo.environment["SPELLRUN_PREVIEW"] else { return }
        try Self.savePNG(canvas, to: URL(fileURLWithPath: path))
    }

    /// Saves the canvas scaled up 4× with nearest-neighbor, so pixels stay crisp.
    static func savePNG(_ small: PixelCanvas, to url: URL) throws {
        let scale = 4
        let canvas = PixelCanvas(width: small.width * scale, height: small.height * scale)
        for y in 0..<canvas.height { for x in 0..<canvas.width { canvas.set(x, y, small.pixel(x / scale, y / scale)) } }
        var pixels = canvas.pixels
        let data = Data(bytes: &pixels, count: pixels.count * 4) as CFData
        let image = CGImage(
            width: canvas.width, height: canvas.height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: canvas.width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: CGDataProvider(data: data)!, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
        let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
    }
}

/// Plays a real track through the same analysis the app uses and checks the music actually drives the game.
/// Set SPELLRUN_TRACK=/path/to/song (and optionally SPELLRUN_FRAMES=/dir) to run it.
final class SpellrunMusicTests: XCTestCase {
    func testRealTrackDrivesTheGame() throws {
        guard let path = ProcessInfo.processInfo.environment["SPELLRUN_TRACK"] else {
            throw XCTSkip("Set SPELLRUN_TRACK to an audio file to run this")
        }
        let samples = try MonoDecoder.decode(URL(fileURLWithPath: path), seconds: 60)
        let analyzer = MultibandAnalyzer()
        var follower = BandFollower()
        let game = SpellrunGame(seed: 5)
        let hop = Int(samples.rate / 60)
        var lastLow: Float = 0, lastHigh: Float = 0
        var kicks = 0
        var frame = 0
        var end = 2048
        while end <= samples.data.count {
            let levels = samples.data.withUnsafeBufferPointer {
                analyzer.levels(from: UnsafeBufferPointer(rebasing: $0[(end - 2048)..<end]), sampleRate: samples.rate)
            }
            follower.step(levels, dt: 1.0 / 60)
            let kick = follower.hits.low > 0.9 && lastLow <= 0.9
            let sparkle = follower.hits.high > 0.9 && lastHigh <= 0.9
            lastLow = follower.hits.low
            lastHigh = follower.hits.high
            if kick { kicks += 1 }
            let e = follower.envelope
            game.step(dt: 1.0 / 60, input: SpellrunInput(low: e.low, mid: e.mid, high: e.high, kick: kick,
                                                         bigKick: kick && e.low > 0.75, sparkle: sparkle),
                      viewWidth: 256, viewHeight: 160)
            if let directory = ProcessInfo.processInfo.environment["SPELLRUN_FRAMES"], frame > 0, frame % 600 == 0 {
                let canvas = PixelCanvas(width: 256, height: 160)
                SpellrunRenderer().render(game, into: canvas, levels: e, showMeters: true)
                try SpellrunGameTests.savePNG(canvas, to: URL(fileURLWithPath: "\(directory)/music_\(frame / 60)s.png"))
            }
            end += hop
            frame += 1
        }
        let s = game.stats
        print("Spellrun on real music: kicks \(kicks), hops \(s.beatHops), pounds \(s.pounds), spells \(s.spells), " +
              "stomps \(s.stomps), blocks \(s.blocksShattered), boosts \(s.boosts), motes \(game.motesCollected), respawns \(game.respawns)")
        XCTAssertGreaterThan(kicks, 20, "the bass should register kicks")
        XCTAssertGreaterThan(s.beatHops + s.pounds, 5, "kicks should make the apprentice hop or pound")
        XCTAssertGreaterThan(s.spells, 3, "highs should cast spells")
        XCTAssertEqual(game.respawns, 0)
    }
}

enum MonoDecoder {
    static func decode(_ url: URL, seconds: Double) throws -> (data: [Float], rate: Double) {
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        let frames = AVAudioFrameCount(min(Double(file.length), seconds * format.sampleRate))
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)!
        try file.read(into: buffer, frameCount: frames)
        let channels = Int(format.channelCount)
        var mono = [Float](repeating: 0, count: Int(buffer.frameLength))
        for c in 0..<channels {
            let data = buffer.floatChannelData![c]
            for i in mono.indices { mono[i] += data[i] / Float(channels) }
        }
        return (mono, format.sampleRate)
    }
}

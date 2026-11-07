import XCTest
@testable import GrimoireCore
import simd

final class PlaylistTests: XCTestCase {
    private func makePlaylist(_ count: Int) -> Playlist {
        Playlist(tracks: (0..<count).map { Track(url: URL(fileURLWithPath: "/music/\($0).mp3")) })
    }

    func testAdvancesInOrderAndStopsAtEnd() {
        var playlist = makePlaylist(3)
        XCTAssertEqual(playlist.advance(automatic: true)?.id, playlist.tracks[0].id)
        XCTAssertEqual(playlist.advance(automatic: true)?.id, playlist.tracks[1].id)
        XCTAssertEqual(playlist.advance(automatic: true)?.id, playlist.tracks[2].id)
        XCTAssertNil(playlist.advance(automatic: true))
    }

    func testRepeatAllWraps() {
        var playlist = makePlaylist(2)
        playlist.repeatMode = .all
        playlist.select(playlist.tracks[1].id)
        XCTAssertEqual(playlist.advance(automatic: true)?.id, playlist.tracks[0].id)
        XCTAssertEqual(playlist.retreat()?.id, playlist.tracks[1].id)
    }

    func testRepeatOneReplaysOnlyWhenAutomatic() {
        var playlist = makePlaylist(3)
        playlist.repeatMode = .one
        playlist.select(playlist.tracks[0].id)
        XCTAssertEqual(playlist.advance(automatic: true)?.id, playlist.tracks[0].id)
        XCTAssertEqual(playlist.advance(automatic: false)?.id, playlist.tracks[1].id)
    }

    func testRetreatAtStartStaysWithoutRepeat() {
        var playlist = makePlaylist(3)
        playlist.select(playlist.tracks[0].id)
        XCTAssertEqual(playlist.retreat()?.id, playlist.tracks[0].id)
    }

    func testShuffleVisitsEveryTrackOnce() {
        var playlist = makePlaylist(20)
        playlist.shuffle = true
        var seen: [Track.ID] = []
        while let track = playlist.advance(automatic: true) { seen.append(track.id) }
        XCTAssertEqual(seen.count, 20)
        XCTAssertEqual(Set(seen), Set(playlist.tracks.map(\.id)))
    }

    func testRemovingCurrentClearsIt() {
        var playlist = makePlaylist(3)
        playlist.select(playlist.tracks[1].id)
        playlist.remove(ids: [playlist.tracks[1].id])
        XCTAssertNil(playlist.current)
        XCTAssertEqual(playlist.tracks.count, 2)
    }

    func testMoveKeepsCurrentTrack() {
        var playlist = makePlaylist(4)
        let ids = playlist.tracks.map(\.id)
        playlist.select(ids[0])
        playlist.move(fromOffsets: [0], toOffset: 3)
        XCTAssertEqual(playlist.tracks.map(\.id), [ids[1], ids[2], ids[0], ids[3]])
        XCTAssertEqual(playlist.currentIndex, 2)
    }
}

final class M3UTests: XCTestCase {
    func testRoundTrip() {
        let tracks = [
            Track(url: URL(fileURLWithPath: "/music/a song.flac"), title: "Song", artist: "Band", duration: 61.4),
            Track(url: URL(fileURLWithPath: "/music/b.wav")),
        ]
        let text = M3U.serialize(tracks)
        XCTAssertTrue(text.contains("#EXTINF:61,Band - Song"))
        let urls = M3U.parse(text, relativeTo: URL(fileURLWithPath: "/tmp/", isDirectory: true))
        XCTAssertEqual(urls.map(\.path), ["/music/a song.flac", "/music/b.wav"])
    }

    func testRelativePathsResolveAgainstPlaylistFolder() {
        let urls = M3U.parse("#EXTM3U\nsub/x.mp3\n\n../y.aiff\n", relativeTo: URL(fileURLWithPath: "/music/lists/", isDirectory: true))
        XCTAssertEqual(urls.map(\.path), ["/music/lists/sub/x.mp3", "/music/y.aiff"])
    }
}

final class TimeMathTests: XCTestCase {
    func testSeekFrameClamps() {
        XCTAssertEqual(SeekMath.frame(for: 1.5, sampleRate: 44_100, length: 441_000), 66_150)
        XCTAssertEqual(SeekMath.frame(for: -3, sampleRate: 44_100, length: 441_000), 0)
        XCTAssertEqual(SeekMath.frame(for: 999, sampleRate: 44_100, length: 441_000), 441_000)
    }

    func testElapsedTimeAfterSeek() {
        let time = SeekMath.time(startFrame: 88_200, renderedFrames: 44_100, sampleRate: 44_100, length: 441_000)
        XCTAssertEqual(time, 3, accuracy: 0.0001)
    }

    func testFormatting() {
        XCTAssertEqual(TimeFormat.clock(185), "3:05")
        XCTAssertEqual(TimeFormat.clock(3723), "1:02:03")
        XCTAssertEqual(TimeFormat.lcd(3723), "62:03")
        XCTAssertEqual(TimeFormat.lcd(.nan), "00:00")
    }
}

final class FLACTagsTests: XCTestCase {
    func testParsesVorbisComment() {
        func le(_ value: Int) -> Data { Data((0..<4).map { UInt8((value >> ($0 * 8)) & 0xFF) }) }
        var block = le(3) + Data("abc".utf8) + le(2)
        for entry in ["title=Moonrise", "ARTIST=The Coven"] {
            block += le(entry.utf8.count) + Data(entry.utf8)
        }
        let tags = FLACTags.parseVorbisComment(block)
        XCTAssertEqual(tags?["TITLE"], "Moonrise")
        XCTAssertEqual(tags?["ARTIST"], "The Coven")
    }

    func testReadsTagsFromFile() throws {
        let path = ProcessInfo.processInfo.environment["GRIMOIRE_TEST_FLAC"]
        guard let path else { throw XCTSkip("Set GRIMOIRE_TEST_FLAC to a tagged .flac to run this") }
        XCTAssertNotNil(FLACTags.read(from: URL(fileURLWithPath: path))?["TITLE"])
    }
}

final class SpectrumTests: XCTestCase {
    private func sine(_ frequency: Double, sampleRate: Double, count: Int = 2048, amplitude: Float = 0.5) -> [Float] {
        (0..<count).map { amplitude * Float(sin(2 * .pi * frequency * Double($0) / sampleRate)) }
    }

    private func loudestBand(_ bands: [Float]) -> Int {
        bands.indices.max { bands[$0] < bands[$1] }!
    }

    func testSinePeaksInItsBand() {
        let analyzer = SpectrumAnalyzer()
        let bands = sine(1000, sampleRate: 44_100).withUnsafeBufferPointer { analyzer.bands(from: $0, sampleRate: 44_100) }
        XCTAssertEqual(loudestBand(bands), analyzer.bandIndex(for: 1000, sampleRate: 44_100))
        XCTAssertGreaterThan(bands[loudestBand(bands)], 0.8)
    }

    func testSilenceIsZero() {
        let analyzer = SpectrumAnalyzer()
        let silence = [Float](repeating: 0, count: 2048)
        let bands = silence.withUnsafeBufferPointer { analyzer.bands(from: $0, sampleRate: 44_100) }
        XCTAssertEqual(bands, [Float](repeating: 0, count: analyzer.bandCount))
    }

    func testSameToneSameBandAcrossSampleRates() {
        let analyzer = SpectrumAnalyzer()
        for rate in [48_000.0, 96_000.0] {
            let bands = sine(3000, sampleRate: rate).withUnsafeBufferPointer { analyzer.bands(from: $0, sampleRate: rate) }
            XCTAssertEqual(loudestBand(bands), analyzer.bandIndex(for: 3000, sampleRate: 44_100), "at \(rate) Hz")
        }
    }

    func testLowAndHighTonesLandAtOppositeEnds() {
        let analyzer = SpectrumAnalyzer()
        let low = sine(80, sampleRate: 44_100).withUnsafeBufferPointer { analyzer.bands(from: $0, sampleRate: 44_100) }
        let high = sine(12_000, sampleRate: 44_100).withUnsafeBufferPointer { analyzer.bands(from: $0, sampleRate: 44_100) }
        XCTAssertLessThan(loudestBand(low), 4)
        XCTAssertGreaterThan(loudestBand(high), analyzer.bandCount - 4)
    }
}

final class BarDynamicsTests: XCTestCase {
    func testRisesInstantlyAndFallsAtRate() {
        var bars = BarDynamics(count: 1)
        bars.step(targets: [0.9], dt: 0.016)
        XCTAssertEqual(bars.levels[0], 0.9)
        bars.step(targets: nil, dt: 0.25)
        XCTAssertEqual(bars.levels[0], 0.9 - 1.6 * 0.25, accuracy: 0.0001)
    }

    func testPeakHoldsThenFalls() {
        var bars = BarDynamics(count: 1)
        bars.step(targets: [1], dt: 0.016)
        bars.step(targets: nil, dt: 0.3)
        XCTAssertEqual(bars.peaks[0], 1)
        bars.step(targets: nil, dt: 0.1)
        XCTAssertLessThan(bars.peaks[0], 1)
    }

    func testSettlesToZero() {
        var bars = BarDynamics(count: 3)
        bars.step(targets: [1, 0.5, 0.2], dt: 0.016)
        XCTAssertFalse(bars.isSettled)
        for _ in 0..<200 { bars.step(targets: nil, dt: 0.016) }
        XCTAssertTrue(bars.isSettled)
    }
}

final class MultibandTests: XCTestCase {
    private func levels(forSine frequency: Double, sampleRate: Double = 44_100) -> BandLevels {
        let samples = (0..<2048).map { 0.5 * Float(sin(2 * .pi * frequency * Double($0) / sampleRate)) }
        return samples.withUnsafeBufferPointer { MultibandAnalyzer().levels(from: $0, sampleRate: sampleRate) }
    }

    func testEachToneLandsInItsBand() {
        let bass = levels(forSine: 60)
        XCTAssertGreaterThan(bass.low, bass.mid)
        XCTAssertGreaterThan(bass.low, bass.high)

        let mid = levels(forSine: 400)
        XCTAssertGreaterThan(mid.mid, mid.low)
        XCTAssertGreaterThan(mid.mid, mid.high)

        let high = levels(forSine: 5000, sampleRate: 96_000)
        XCTAssertGreaterThan(high.high, high.low)
        XCTAssertGreaterThan(high.high, high.mid)
    }

    func testSilenceIsZero() {
        let silence = [Float](repeating: 0, count: 2048)
        let result = silence.withUnsafeBufferPointer { MultibandAnalyzer().levels(from: $0, sampleRate: 44_100) }
        XCTAssertEqual(result, .zero)
    }

    func testFollowerAutoGainsAndReleases() {
        var follower = BandFollower()
        for _ in 0..<60 { follower.step(BandLevels(low: 0.6, mid: 0.6, high: 0.6), dt: 1.0 / 60) }
        XCTAssertGreaterThan(follower.envelope.low, 0.95, "a steady level should fill the range after auto-gain")
        for _ in 0..<120 { follower.step(nil, dt: 1.0 / 60) }
        XCTAssertLessThan(follower.envelope.low, 0.01)
    }

    func testFollowerDetectsHits() {
        var follower = BandFollower()
        for _ in 0..<120 { follower.step(BandLevels(low: 0.2, mid: 0.2, high: 0.2), dt: 1.0 / 60) }
        XCTAssertEqual(follower.hits.low, 0)
        follower.step(BandLevels(low: 0.9, mid: 0.2, high: 0.2), dt: 1.0 / 60)
        XCTAssertEqual(follower.hits.low, 1)
        XCTAssertEqual(follower.hits.mid, 0)
    }
}

final class ThreeBodyTests: XCTestCase {
    func testFigureEightConservesEnergyAndMomentum() {
        var system = ThreeBodySystem.figureEight()
        let start = system.totalEnergy
        for _ in 0..<(60 * 20) { system.advance(by: 1.0 / 60) }
        XCTAssertEqual(system.totalEnergy, start, accuracy: abs(start) * 1e-3)
        XCTAssertLessThan(simd_length(system.momentum), 1e-9)
    }

    func testFigureEightReturnsAfterOnePeriod() {
        var system = ThreeBodySystem.figureEight()
        system.softening = 0
        let start = system.bodies.map(\.position)
        system.advance(by: 6.32591398, maxStep: 0.0005)
        for (body, origin) in zip(system.bodies, start) {
            XCTAssertLessThan(simd_length(body.position - origin), 0.01)
        }
    }

    func testStaysBoundedUnderMusicLikeModulation() {
        var system = ThreeBodySystem.figureEight()
        for frame in 0..<(60 * 120) {
            let t = Double(frame) / 60
            system.gravity = 1 + 0.9 * abs(sin(t * 7.3))
            system.setMasses([1 + 0.8 * abs(sin(t * 2.1)), 1 + 0.8 * abs(sin(t * 3.7)), 1 + 0.8 * abs(sin(t * 5.9))])
            system.advance(by: (0.45 + abs(sin(t))) / 60)
        }
        XCTAssertTrue(system.bodies.allSatisfy { $0.position.x.isFinite && $0.position.y.isFinite })
        XCTAssertLessThan(system.extent, system.wallRadius * 2)
    }
}

final class ThreeBodyThermostatTests: XCTestCase {
    /// Mirrors ThreeBodyScene: softened, gently trapped, energy target scaled with gravity and mass.
    func testOrbitKeepsItsSizeUnderMusicLikeModulation() {
        var system = ThreeBodySystem.figureEight()
        system.softening = 0.15
        system.trapStrength = 1
        system.maxKineticEnergy = 30
        let baseline = system.totalEnergy
        var extents: [Double] = []
        for frame in 0..<(60 * 90) {
            let t = Double(frame) / 60
            let low = abs(sin(t * 2.3)), mid = abs(sin(t * 1.3)), high = abs(sin(t * 3.1))
            let masses = [1 + 0.8 * low, 1 + 0.8 * mid, 1 + 0.8 * high]
            system.gravity = 1 + 0.9 * low
            system.setMasses(masses)
            let meanMass = masses.reduce(0, +) / 3
            system.targetEnergy = baseline * system.gravity * meanMass * meanMass
            system.advance(by: (0.45 + 0.9 * mid) / 60)
            if frame > 600 { extents.append(system.extent) }
        }
        extents.sort()
        XCTAssertGreaterThan(extents[extents.count / 10], 0.4, "the orbit should not collapse into a knot")
        XCTAssertLessThan(extents[extents.count * 9 / 10], system.wallRadius, "no body should be flung to the wall")
    }
}

final class BandFollowerRangeTests: XCTestCase {
    func testDenseLoudSignalStillSwings() {
        var follower = BandFollower()
        var lows: [Float] = []
        for frame in 0..<(60 * 20) {
            let pulse: Float = frame % 30 < 5 ? 0.95 : 0.8   // loud bed with kicks on top
            follower.step(BandLevels(low: pulse, mid: 0.8, high: 0.8), dt: 1.0 / 60)
            if frame > 60 * 10 { lows.append(follower.envelope.low) }
        }
        XCTAssertGreaterThan(lows.max()! - lows.min()!, 0.4, "kicks should still move the envelope a lot")
    }
}

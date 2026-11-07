import XCTest
@testable import GrimoireCore

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

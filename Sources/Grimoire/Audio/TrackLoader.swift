import AVFoundation
import GrimoireCore

/// Reads tags and duration without blocking the main thread.
enum TrackLoader {
    struct Metadata: Sendable {
        var title: String?
        var artist: String?
        var duration: TimeInterval?
    }

    static func metadata(for url: URL) async -> Metadata {
        let asset = AVURLAsset(url: url)
        var result = Metadata()
        if let items = try? await asset.load(.commonMetadata) {
            result.title = await string(.commonIdentifierTitle, in: items)
            result.artist = await string(.commonIdentifierArtist, in: items)
        }
        if let duration = try? await asset.load(.duration), duration.isNumeric {
            result.duration = duration.seconds
        }
        if url.pathExtension.lowercased() == "flac", let tags = FLACTags.read(from: url) {
            result.title = tags["TITLE"] ?? result.title
            result.artist = tags["ARTIST"] ?? result.artist
        }
        return result
    }

    private static func string(_ identifier: AVMetadataIdentifier, in items: [AVMetadataItem]) async -> String? {
        guard let item = AVMetadataItem.metadataItems(from: items, filteredByIdentifier: identifier).first else { return nil }
        return try? await item.load(.stringValue)
    }
}

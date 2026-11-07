import Foundation

public struct Track: Identifiable, Hashable, Sendable {
    public let id: UUID
    public let url: URL
    public var title: String?
    public var artist: String?
    public var duration: TimeInterval?

    public init(url: URL, title: String? = nil, artist: String? = nil, duration: TimeInterval? = nil) {
        self.id = UUID()
        self.url = url
        self.title = title
        self.artist = artist
        self.duration = duration
    }

    /// "Artist - Title", falling back to the file name when tags are missing.
    public var displayTitle: String {
        let name = title.nonEmpty ?? url.deletingPathExtension().lastPathComponent
        if let artist = artist.nonEmpty { return "\(artist) - \(name)" }
        return name
    }
}

extension Optional where Wrapped == String {
    var nonEmpty: String? {
        guard let trimmed = self?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else { return nil }
        return trimmed
    }
}

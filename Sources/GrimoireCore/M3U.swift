import Foundation

public enum M3U {
    /// Returns the file URLs listed in an m3u/m3u8 playlist. Relative paths resolve against `base` (the playlist's folder).
    public static func parse(_ text: String, relativeTo base: URL) -> [URL] {
        text.components(separatedBy: .newlines).compactMap { raw in
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty, !line.hasPrefix("#") else { return nil }
            if line.hasPrefix("file://") { return URL(string: line) }
            if line.hasPrefix("/") { return URL(fileURLWithPath: line) }
            return URL(fileURLWithPath: line, relativeTo: base).standardizedFileURL
        }
    }

    public static func serialize(_ tracks: [Track]) -> String {
        var lines = ["#EXTM3U"]
        for track in tracks {
            let seconds = track.duration.map { Int($0.rounded()) } ?? -1
            lines.append("#EXTINF:\(seconds),\(track.displayTitle)")
            lines.append(track.url.path)
        }
        return lines.joined(separator: "\n") + "\n"
    }
}

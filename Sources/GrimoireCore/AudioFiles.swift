import Foundation

public enum AudioFiles {
    public static let extensions: Set<String> = ["mp3", "wav", "wave", "aif", "aiff", "aifc", "flac"]
    public static let playlistExtensions: Set<String> = ["m3u", "m3u8"]

    public static func isSupported(_ url: URL) -> Bool {
        extensions.contains(url.pathExtension.lowercased())
    }

    public static func isPlaylist(_ url: URL) -> Bool {
        playlistExtensions.contains(url.pathExtension.lowercased())
    }

    /// Expands folders recursively into supported audio files (sorted Finder-style); plain files pass through if supported.
    public static func expand(_ urls: [URL]) -> [URL] {
        var result: [URL] = []
        for url in urls {
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else { continue }
            if isDirectory.boolValue {
                let enumerator = FileManager.default.enumerator(at: url, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles])
                var found: [URL] = []
                while let file = enumerator?.nextObject() as? URL {
                    if isSupported(file) { found.append(file) }
                }
                result += found.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
            } else if isSupported(url) {
                result.append(url)
            }
        }
        return result
    }
}

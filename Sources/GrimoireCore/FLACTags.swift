import Foundation

/// Reads Vorbis comments (TITLE, ARTIST, …) from a FLAC file's metadata blocks.
/// AVFoundation plays FLAC but doesn't surface these tags, so we parse them directly.
public enum FLACTags {
    /// Upper-cased tag names mapped to their first value, or nil if the file isn't FLAC or has no comments.
    public static func read(from url: URL) -> [String: String]? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        guard (try? handle.read(upToCount: 4)) == Data("fLaC".utf8) else { return nil }

        while let header = try? handle.read(upToCount: 4), header.count == 4 {
            let isLast = header[0] & 0x80 != 0
            let type = header[0] & 0x7F
            let length = Int(header[1]) << 16 | Int(header[2]) << 8 | Int(header[3])
            if type == 4 {
                guard let block = try? handle.read(upToCount: length), block.count == length else { return nil }
                return parseVorbisComment(block)
            }
            guard !isLast, let offset = try? handle.offset() else { return nil }
            try? handle.seek(toOffset: offset + UInt64(length))
        }
        return nil
    }

    /// Vorbis comment block: little-endian vendor length + vendor, count, then `length + "KEY=value"` entries.
    static func parseVorbisComment(_ data: Data) -> [String: String]? {
        var reader = Reader(data: data)
        guard let vendorLength = reader.uint32(), reader.skip(Int(vendorLength)), let count = reader.uint32() else { return nil }
        var tags: [String: String] = [:]
        for _ in 0..<count {
            guard let length = reader.uint32(), let bytes = reader.bytes(Int(length)) else { break }
            guard let entry = String(data: bytes, encoding: .utf8), let equals = entry.firstIndex(of: "=") else { continue }
            let key = entry[..<equals].uppercased()
            if tags[key] == nil { tags[key] = String(entry[entry.index(after: equals)...]) }
        }
        return tags
    }

    private struct Reader {
        let data: Data
        var position = 0

        mutating func uint32() -> UInt32? {
            guard let bytes = bytes(4) else { return nil }
            return bytes.reversed().reduce(0) { $0 << 8 | UInt32($1) }
        }

        mutating func bytes(_ count: Int) -> Data? {
            guard count >= 0, position + count <= data.count else { return nil }
            defer { position += count }
            return data.subdata(in: data.startIndex + position ..< data.startIndex + position + count)
        }

        mutating func skip(_ count: Int) -> Bool {
            bytes(count) != nil
        }
    }
}

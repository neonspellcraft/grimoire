import Foundation

/// Small, fast, seedable RNG (SplitMix64), so generated levels are reproducible in tests.
public struct SplitMix64: RandomNumberGenerator, Sendable {
    private var state: UInt64

    public init(seed: UInt64) {
        state = seed
    }

    public mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }

    public mutating func unit() -> Double {
        Double(next() >> 11) / Double(1 << 53)
    }

    public mutating func int(_ range: ClosedRange<Int>) -> Int {
        Int.random(in: range, using: &self)
    }

    public mutating func chance(_ probability: Double) -> Bool {
        unit() < probability
    }
}

/// Packs 0xRRGGBB into an RGBA8 pixel (memory order R, G, B, A).
public func rgb(_ hex: UInt32) -> UInt32 {
    let r = (hex >> 16) & 0xFF, g = (hex >> 8) & 0xFF, b = hex & 0xFF
    return 0xFF00_0000 | (b << 16) | (g << 8) | r
}

/// A low-resolution RGBA8 framebuffer with the few drawing operations a pixel-art game needs.
/// Origin is the top-left; everything is clipped to the canvas.
public final class PixelCanvas {
    public let width: Int
    public let height: Int
    public private(set) var pixels: [UInt32]

    public init(width: Int, height: Int) {
        self.width = width
        self.height = height
        pixels = [UInt32](repeating: 0xFF00_0000, count: width * height)
    }

    @inline(__always)
    public func pixel(_ x: Int, _ y: Int) -> UInt32 {
        guard x >= 0, y >= 0, x < width, y < height else { return 0 }
        return pixels[y * width + x]
    }

    @inline(__always)
    public func set(_ x: Int, _ y: Int, _ color: UInt32) {
        guard x >= 0, y >= 0, x < width, y < height else { return }
        pixels[y * width + x] = color
    }

    public func fill(_ color: UInt32) {
        for i in pixels.indices { pixels[i] = color }
    }

    public func fillRect(x: Int, y: Int, width w: Int, height h: Int, _ color: UInt32) {
        let x0 = max(0, x), y0 = max(0, y), x1 = min(width, x + w), y1 = min(height, y + h)
        guard x0 < x1, y0 < y1 else { return }
        for row in y0..<y1 {
            let base = row * width
            for column in x0..<x1 { pixels[base + column] = color }
        }
    }

    /// Mixes `color` over the existing pixel by `alpha` (0...1).
    public func blend(_ x: Int, _ y: Int, _ color: UInt32, alpha: Float) {
        guard x >= 0, y >= 0, x < width, y < height, alpha > 0 else { return }
        let index = y * width + x
        pixels[index] = mix(pixels[index], color, alpha)
    }

    /// Brightens the existing pixel by `color × amount`, like additive light.
    public func add(_ x: Int, _ y: Int, _ color: UInt32, amount: Float) {
        guard x >= 0, y >= 0, x < width, y < height, amount > 0 else { return }
        let index = y * width + x
        let d = pixels[index]
        func channel(_ shift: UInt32) -> UInt32 {
            let sum = Float((d >> shift) & 0xFF) + Float((color >> shift) & 0xFF) * amount
            return UInt32(min(sum, 255)) << shift
        }
        pixels[index] = 0xFF00_0000 | channel(16) | channel(8) | channel(0)
    }

    /// A soft round glow centered on (cx, cy), added to the image.
    public func glow(cx: Int, cy: Int, radius: Int, _ color: UInt32, strength: Float) {
        guard radius > 0, strength > 0 else { return }
        let r2 = Float(radius * radius)
        for dy in -radius...radius {
            for dx in -radius...radius {
                let d2 = Float(dx * dx + dy * dy)
                guard d2 <= r2 else { continue }
                let falloff = 1 - d2 / r2
                add(cx + dx, cy + dy, color, amount: strength * falloff * falloff)
            }
        }
    }

    /// Draws a sprite with its top-left at (x, y). Transparent pixels are skipped.
    public func draw(_ sprite: Sprite, x: Int, y: Int, flipped: Bool = false, palette: [Character: UInt32]? = nil) {
        let colors = palette ?? sprite.palette
        for (row, line) in sprite.rows.enumerated() {
            for (column, symbol) in line.enumerated() where symbol != "." {
                guard let color = colors[symbol] else { continue }
                let dx = flipped ? sprite.width - 1 - column : column
                set(x + dx, y + row, color)
            }
        }
    }

    /// Draws text in the 3×5 pixel font, one pixel between letters.
    public func text(_ string: String, x: Int, y: Int, _ color: UInt32) {
        var cursor = x
        for character in string.uppercased() {
            if let glyph = PixelFont.glyphs[character] {
                for (row, line) in glyph.enumerated() {
                    for (column, bit) in line.enumerated() where bit == "#" {
                        set(cursor + column, y + row, color)
                    }
                }
            }
            cursor += 4
        }
    }
}

/// Linear blend of two RGBA8 colors.
public func mix(_ a: UInt32, _ b: UInt32, _ t: Float) -> UInt32 {
    let t = min(max(t, 0), 1)
    func channel(_ shift: UInt32) -> UInt32 {
        let x = Float((a >> shift) & 0xFF), y = Float((b >> shift) & 0xFF)
        return UInt32(x + (y - x) * t) << shift
    }
    return 0xFF00_0000 | channel(16) | channel(8) | channel(0)
}

/// Pixel art as strings: one character per pixel, "." for transparent, other characters looked up in `palette`.
public struct Sprite: Sendable {
    public let rows: [[Character]]
    public let palette: [Character: UInt32]
    public var width: Int { rows.first?.count ?? 0 }
    public var height: Int { rows.count }

    public init(_ rows: [String], palette: [Character: UInt32]) {
        self.rows = rows.map(Array.init)
        self.palette = palette
    }
}

/// 4×4 ordered-dither threshold (0...1), for banded gradients and pixel dissolves.
public func bayer(_ x: Int, _ y: Int) -> Float {
    let matrix: [Float] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]
    return (matrix[(y & 3) * 4 + (x & 3)] + 0.5) / 16
}

enum PixelFont {
    static let glyphs: [Character: [String]] = [
        "A": [".#.", "#.#", "###", "#.#", "#.#"], "B": ["##.", "#.#", "##.", "#.#", "##."],
        "C": [".##", "#..", "#..", "#..", ".##"], "D": ["##.", "#.#", "#.#", "#.#", "##."],
        "E": ["###", "#..", "##.", "#..", "###"], "F": ["###", "#..", "##.", "#..", "#.."],
        "G": [".##", "#..", "#.#", "#.#", ".##"], "H": ["#.#", "#.#", "###", "#.#", "#.#"],
        "I": ["###", ".#.", ".#.", ".#.", "###"], "J": ["..#", "..#", "..#", "#.#", ".#."],
        "K": ["#.#", "#.#", "##.", "#.#", "#.#"], "L": ["#..", "#..", "#..", "#..", "###"],
        "M": ["#.#", "###", "###", "#.#", "#.#"], "N": ["##.", "#.#", "#.#", "#.#", "#.#"],
        "O": [".#.", "#.#", "#.#", "#.#", ".#."], "P": ["##.", "#.#", "##.", "#..", "#.."],
        "Q": [".#.", "#.#", "#.#", "##.", ".##"], "R": ["##.", "#.#", "##.", "#.#", "#.#"],
        "S": [".##", "#..", ".#.", "..#", "##."], "T": ["###", ".#.", ".#.", ".#.", ".#."],
        "U": ["#.#", "#.#", "#.#", "#.#", "###"], "V": ["#.#", "#.#", "#.#", "#.#", ".#."],
        "W": ["#.#", "#.#", "###", "###", "#.#"], "X": ["#.#", "#.#", ".#.", "#.#", "#.#"],
        "Y": ["#.#", "#.#", ".#.", ".#.", ".#."], "Z": ["###", "..#", ".#.", "#..", "###"],
        "0": ["###", "#.#", "#.#", "#.#", "###"], "1": [".#.", "##.", ".#.", ".#.", "###"],
        "2": ["##.", "..#", ".#.", "#..", "###"], "3": ["##.", "..#", ".#.", "..#", "##."],
        "4": ["#.#", "#.#", "###", "..#", "..#"], "5": ["###", "#..", "##.", "..#", "##."],
        "6": [".##", "#..", "###", "#.#", "###"], "7": ["###", "..#", ".#.", ".#.", ".#."],
        "8": ["###", "#.#", "###", "#.#", "###"], "9": ["###", "#.#", "###", "..#", "##."],
        "-": ["...", "...", "###", "...", "..."], " ": ["...", "...", "...", "...", "..."],
    ]
}

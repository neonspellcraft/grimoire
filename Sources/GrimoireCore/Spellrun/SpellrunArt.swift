import Foundation

/// A biome's look: its palette and the shape of its parallax backdrop.
public struct Biome: Sendable {
    public enum Backdrop: Sendable { case mountains, pines, crystals, castle }

    public let name: String
    public let skyTop: UInt32
    public let skyBottom: UInt32
    public let far: UInt32
    public let near: UInt32
    public let nearShade: UInt32
    public let groundTop: UInt32
    public let groundTopShade: UInt32
    public let groundFill: UInt32
    public let groundDark: UInt32
    public let platform: UInt32
    public let platformShade: UInt32
    public let block: UInt32
    public let blockGlow: UInt32
    public let accent: UInt32
    public let backdrop: Backdrop
    public let hasStars: Bool
    public let hasClouds: Bool
    /// The sun or moon: color and whether it's drawn at all.
    public let orb: UInt32?

    public static let all: [Biome] = [
        Biome(name: "Sunlit Meadow",
              skyTop: rgb(0x4A8FE7), skyBottom: rgb(0xB8E3FF), far: rgb(0x8DB7D9), near: rgb(0x5DAE5B),
              nearShade: rgb(0x3F8A48), groundTop: rgb(0x6FD35B), groundTopShade: rgb(0x46A847),
              groundFill: rgb(0x9A6A3C), groundDark: rgb(0x6E4728), platform: rgb(0xC58B4A),
              platformShade: rgb(0x8C5A2C), block: rgb(0x5B4B8A), blockGlow: rgb(0x9FFFD0),
              accent: rgb(0xFF7FB0), backdrop: .mountains, hasStars: false, hasClouds: true, orb: rgb(0xFFF1A8)),
        Biome(name: "Dusk Forest",
              skyTop: rgb(0x3A2363), skyBottom: rgb(0xF08A5D), far: rgb(0x6B3E6E), near: rgb(0x2E2A4A),
              nearShade: rgb(0x1E1B33), groundTop: rgb(0x4F9A6B), groundTopShade: rgb(0x356F50),
              groundFill: rgb(0x5A3E3A), groundDark: rgb(0x3A2729), platform: rgb(0x8E5B3F),
              platformShade: rgb(0x5C3827), block: rgb(0x46305E), blockGlow: rgb(0xFFD27A),
              accent: rgb(0xFFB347), backdrop: .pines, hasStars: false, hasClouds: true, orb: rgb(0xFFC27A)),
        Biome(name: "Crystal Cavern",
              skyTop: rgb(0x0B1424), skyBottom: rgb(0x1D3B4F), far: rgb(0x23506A), near: rgb(0x1A2F44),
              nearShade: rgb(0x111F2E), groundTop: rgb(0x7E6BC9), groundTopShade: rgb(0x5A4A9A),
              groundFill: rgb(0x3B3460), groundDark: rgb(0x27223F), platform: rgb(0x5E7FA3),
              platformShade: rgb(0x3D5672), block: rgb(0x2E5B73), blockGlow: rgb(0x7CF5FF),
              accent: rgb(0x6CF3FF), backdrop: .crystals, hasStars: true, hasClouds: false, orb: nil),
        Biome(name: "Starlit Ramparts",
              skyTop: rgb(0x070B1F), skyBottom: rgb(0x26305E), far: rgb(0x1B2147), near: rgb(0x2A2F52),
              nearShade: rgb(0x1C2040), groundTop: rgb(0x8A93B8), groundTopShade: rgb(0x646C91),
              groundFill: rgb(0x474D70), groundDark: rgb(0x31354F), platform: rgb(0x9A8F7A),
              platformShade: rgb(0x6C6352), block: rgb(0x3A2E66), blockGlow: rgb(0xC9A8FF),
              accent: rgb(0xFFE38A), backdrop: .castle, hasStars: true, hasClouds: false, orb: rgb(0xF4F1D8)),
    ]
}

/// Sprites, facing right. Each is plain pixel art in strings.
public enum SpellrunArt {
    static let outline = rgb(0x1A1026)

    // The apprentice: a hooded spellcaster in violet with a staff topped by a green gem (Grimoire's colors).
    static let heroPalette: [Character: UInt32] = [
        "k": outline, "h": rgb(0x8A5CE6), "H": rgb(0x5E3BAA), "s": rgb(0xF2C29B), "e": outline,
        "y": rgb(0xFFE066), "g": rgb(0x74FFAD), "G": rgb(0xE6FFF0), "w": rgb(0x9B6A35), "b": rgb(0x3A2A50),
    ]

    private static let heroBody = [
        ".kk.........",
        ".khk......g.",
        "..khk....gGg",
        "..khhk....g.",
        "..khyhk...w.",
        ".khhhhhk..w.",
        "kkkkkkkkk.w.",
        "..ksseskk.w.",
        "..ksssssk.w.",
        "..khhhhkk.w.",
        ".khhhhhhssw.",
        ".khhhhhHk.w.",
        "khhhhhhHHkw.",
        "kHHHHHHHHkw.",
    ]

    public static let heroRun: [Sprite] = [
        Sprite(heroBody + ["..kb..kb..w.", ".kb....kb.w."], palette: heroPalette),
        Sprite(heroBody + ["...kbkb...w.", "...kb.kb..w."], palette: heroPalette),
    ]
    public static let heroJump = Sprite(heroBody + ["..kbkb....w.", "..........w."], palette: heroPalette)
    public static let heroPound = Sprite(heroBody + [".kbk..kbk.w.", "..........w."], palette: heroPalette)

    static let slimePalette: [Character: UInt32] = [
        "k": outline, "m": rgb(0x46D3A0), "M": rgb(0xB8FFE0), "d": rgb(0x2A9C78), "e": outline,
    ]
    public static let slime: [Sprite] = [
        Sprite([
            "............",
            "....kkkk....",
            "..kkmmmmkk..",
            ".kmMMmmmmmk.",
            "kmMmmmmmmmmk",
            "kmmekmmekmmk",
            "kmmmmmmmmmmk",
            "kddddddddddk",
            ".kkkkkkkkkk.",
        ], palette: slimePalette),
        Sprite([
            "............",
            "............",
            "...kkkkkk...",
            ".kkmMMmmmkk.",
            "kmMmmmmmmmmk",
            "kmmekmmekmmk",
            "kmmmmmmmmmmk",
            "kddddddddddk",
            "kkkkkkkkkkkk",
        ], palette: slimePalette),
    ]
    public static let slimeSquished = Sprite([
        "..kkkkkkkk..",
        "kkmmmmmmmmkk",
        "kmmkkmmkkmmk",
        "kkkkkkkkkkkk",
    ], palette: slimePalette)

    static let batPalette: [Character: UInt32] = [
        "k": outline, "m": rgb(0x6A4C93), "M": rgb(0x9E7CC8), "e": rgb(0xFF5C7A),
    ]
    public static let bat: [Sprite] = [
        Sprite([
            "k..........k",
            "kk........kk",
            "kMk.kkkk.kMk",
            "kmMkmmmmkMmk",
            ".kmmmemmmmk.",
            "..kkmmmmkk..",
            "....kkkk....",
            "............",
        ], palette: batPalette),
        Sprite([
            "............",
            "....kkkk....",
            "...kmmmmk...",
            "..kmemmemk..",
            "kkmmmmmmmmkk",
            "kMmkkmmkkmMk",
            "kMk..kk..kMk",
            "kk........kk",
        ], palette: batPalette),
    ]

    static let motePalette: [Character: UInt32] = ["y": rgb(0xFFD84D), "W": rgb(0xFFFBE6), "o": rgb(0xC98A1E)]
    public static let mote: [Sprite] = [
        Sprite(["..y..", ".yWy.", "yWWWo", ".yWo.", "..o.."], palette: motePalette),
        Sprite(["..y..", "..W..", ".yWo.", "..W..", "..o.."], palette: motePalette),
        Sprite(["..y..", ".yWy.", "yWWWo", ".yWo.", "..o.."], palette: motePalette),
        Sprite(["..y..", ".yWy.", ".WWW.", ".yWo.", "..o.."], palette: motePalette),
    ]

    /// 8×8 rune glyphs carved into rune blocks.
    public static let runes: [[String]] = [
        ["...##...", "..#..#..", "...##...", "#..##..#", ".######.", "...##...", "..#..#..", ".#....#."],
        ["#......#", ".#....#.", "..#..#..", "...##...", "...##...", "..#..#..", ".#....#.", "#......#"],
        ["..####..", ".#....#.", "#..##..#", "#.#..#.#", "#.#..#.#", "#..##..#", ".#....#.", "..####.."],
        ["...#....", "..###...", ".#.#.#..", "...#....", "...#.#..", "...###..", "...#.#..", "...#...."],
    ]
}

//! Biome palettes and sprites (port of `SpellrunArt.swift`). Sprites face right.

use std::sync::LazyLock;

use super::canvas::{rgb, Sprite};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    Mountains,
    Pines,
    Crystals,
    Castle,
    /// Giant glowing mushrooms between the hills.
    Mushrooms,
    /// Jagged ash peaks and a smouldering volcano.
    Volcano,
    /// Sharp ice spires.
    Glacier,
    /// Broken temple columns, under water.
    Ruins,
    /// Towering bookshelves and arches.
    Library,
    /// Islands floating in a starry void.
    Islands,
}

/// Ambient particles drifting through a biome (drawn from a hash of time, so they cost the game nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    /// Blinking fireflies that pulse with the highs.
    Fireflies,
    /// Glowing spores rising slowly.
    Spores,
    /// Embers flickering upward.
    Embers,
    /// Snow drifting down.
    Snow,
    /// Bubbles wobbling upward.
    Bubbles,
    /// Golden dust motes hanging in the air.
    Dust,
}

/// A biome's look: its palette and the shape of its parallax backdrop.
#[derive(Clone, Debug)]
pub struct Biome {
    pub name: &'static str,
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub far: u32,
    pub near: u32,
    pub near_shade: u32,
    pub ground_top: u32,
    pub ground_top_shade: u32,
    pub ground_fill: u32,
    pub ground_dark: u32,
    pub platform: u32,
    pub platform_shade: u32,
    pub block: u32,
    pub block_glow: u32,
    pub accent: u32,
    pub backdrop: Backdrop,
    pub has_stars: bool,
    pub has_clouds: bool,
    /// The sun or moon, if the biome has one.
    pub orb: Option<u32>,
    pub weather: Weather,
    pub weather_color: u32,
}

pub static BIOMES: [Biome; 10] = [
    Biome {
        name: "Sunlit Meadow",
        sky_top: rgb(0x4A8FE7),
        sky_bottom: rgb(0xB8E3FF),
        far: rgb(0x8DB7D9),
        near: rgb(0x5DAE5B),
        near_shade: rgb(0x3F8A48),
        ground_top: rgb(0x6FD35B),
        ground_top_shade: rgb(0x46A847),
        ground_fill: rgb(0x9A6A3C),
        ground_dark: rgb(0x6E4728),
        platform: rgb(0xC58B4A),
        platform_shade: rgb(0x8C5A2C),
        block: rgb(0x5B4B8A),
        block_glow: rgb(0x9FFFD0),
        accent: rgb(0xFF7FB0),
        backdrop: Backdrop::Mountains,
        has_stars: false,
        has_clouds: true,
        orb: Some(rgb(0xFFF1A8)),
        weather: Weather::Clear,
        weather_color: rgb(0xFFFFFF),
    },
    Biome {
        name: "Dusk Forest",
        sky_top: rgb(0x3A2363),
        sky_bottom: rgb(0xF08A5D),
        far: rgb(0x6B3E6E),
        near: rgb(0x2E2A4A),
        near_shade: rgb(0x1E1B33),
        ground_top: rgb(0x4F9A6B),
        ground_top_shade: rgb(0x356F50),
        ground_fill: rgb(0x5A3E3A),
        ground_dark: rgb(0x3A2729),
        platform: rgb(0x8E5B3F),
        platform_shade: rgb(0x5C3827),
        block: rgb(0x46305E),
        block_glow: rgb(0xFFD27A),
        accent: rgb(0xFFB347),
        backdrop: Backdrop::Pines,
        has_stars: false,
        has_clouds: true,
        orb: Some(rgb(0xFFC27A)),
        weather: Weather::Fireflies,
        weather_color: rgb(0xFFE680),
    },
    Biome {
        name: "Mushroom Hollow",
        sky_top: rgb(0x1E1236),
        sky_bottom: rgb(0x5B3A7A),
        far: rgb(0x4A2E66),
        near: rgb(0x2C1E48),
        near_shade: rgb(0x1E1433),
        ground_top: rgb(0x5FD6A8),
        ground_top_shade: rgb(0x3A9C7C),
        ground_fill: rgb(0x4A3550),
        ground_dark: rgb(0x2F2236),
        platform: rgb(0xD9667A),
        platform_shade: rgb(0x9C3F57),
        block: rgb(0x3E2A66),
        block_glow: rgb(0xFF9CE6),
        accent: rgb(0x9CFFE0),
        backdrop: Backdrop::Mushrooms,
        has_stars: false,
        has_clouds: false,
        orb: None,
        weather: Weather::Spores,
        weather_color: rgb(0xB8FFE8),
    },
    Biome {
        name: "Frost Peaks",
        sky_top: rgb(0x2B4C8C),
        sky_bottom: rgb(0xCFE8FF),
        far: rgb(0xA9C4E6),
        near: rgb(0x7FA3CF),
        near_shade: rgb(0x6288B8),
        ground_top: rgb(0xF4FBFF),
        ground_top_shade: rgb(0xBFD8EE),
        ground_fill: rgb(0x6B7FA0),
        ground_dark: rgb(0x4C5C7A),
        platform: rgb(0x9FD8F0),
        platform_shade: rgb(0x6AA8CC),
        block: rgb(0x3E5C9A),
        block_glow: rgb(0xBFF4FF),
        accent: rgb(0x8FF0FF),
        backdrop: Backdrop::Glacier,
        has_stars: false,
        has_clouds: true,
        orb: Some(rgb(0xFFFDF0)),
        weather: Weather::Snow,
        weather_color: rgb(0xFFFFFF),
    },
    Biome {
        name: "Crystal Cavern",
        sky_top: rgb(0x0B1424),
        sky_bottom: rgb(0x1D3B4F),
        far: rgb(0x23506A),
        near: rgb(0x1A2F44),
        near_shade: rgb(0x111F2E),
        ground_top: rgb(0x7E6BC9),
        ground_top_shade: rgb(0x5A4A9A),
        ground_fill: rgb(0x3B3460),
        ground_dark: rgb(0x27223F),
        platform: rgb(0x5E7FA3),
        platform_shade: rgb(0x3D5672),
        block: rgb(0x2E5B73),
        block_glow: rgb(0x7CF5FF),
        accent: rgb(0x6CF3FF),
        backdrop: Backdrop::Crystals,
        has_stars: true,
        has_clouds: false,
        orb: None,
        weather: Weather::Spores,
        weather_color: rgb(0x9CF6FF),
    },
    Biome {
        name: "Ember Wastes",
        sky_top: rgb(0x2A0A12),
        sky_bottom: rgb(0xC2462A),
        far: rgb(0x5A1E22),
        near: rgb(0x2E1416),
        near_shade: rgb(0x1C0C0E),
        ground_top: rgb(0x6B5A55),
        ground_top_shade: rgb(0x4A3D3A),
        ground_fill: rgb(0x3A2624),
        ground_dark: rgb(0x241615),
        platform: rgb(0x8C4A30),
        platform_shade: rgb(0x5C2E1E),
        block: rgb(0x4A1E2A),
        block_glow: rgb(0xFFB347),
        accent: rgb(0xFF6A3D),
        backdrop: Backdrop::Volcano,
        has_stars: false,
        has_clouds: false,
        orb: Some(rgb(0xFF8A5C)),
        weather: Weather::Embers,
        weather_color: rgb(0xFFAA55),
    },
    Biome {
        name: "Sunken Temple",
        sky_top: rgb(0x04303A),
        sky_bottom: rgb(0x0E7A7E),
        far: rgb(0x0A434C),
        near: rgb(0x0A3F48),
        near_shade: rgb(0x072E35),
        ground_top: rgb(0xD8C890),
        ground_top_shade: rgb(0xAE9C68),
        ground_fill: rgb(0x5E6E6A),
        ground_dark: rgb(0x3E4C4A),
        platform: rgb(0x8FB8A8),
        platform_shade: rgb(0x5E8478),
        block: rgb(0x205A6A),
        block_glow: rgb(0x8CFFE4),
        accent: rgb(0xFF8C7A),
        backdrop: Backdrop::Ruins,
        has_stars: false,
        has_clouds: false,
        orb: None,
        weather: Weather::Bubbles,
        weather_color: rgb(0xBFF6FF),
    },
    Biome {
        name: "Starlit Ramparts",
        sky_top: rgb(0x070B1F),
        sky_bottom: rgb(0x26305E),
        far: rgb(0x1B2147),
        near: rgb(0x2A2F52),
        near_shade: rgb(0x1C2040),
        ground_top: rgb(0x8A93B8),
        ground_top_shade: rgb(0x646C91),
        ground_fill: rgb(0x474D70),
        ground_dark: rgb(0x31354F),
        platform: rgb(0x9A8F7A),
        platform_shade: rgb(0x6C6352),
        block: rgb(0x3A2E66),
        block_glow: rgb(0xC9A8FF),
        accent: rgb(0xFFE38A),
        backdrop: Backdrop::Castle,
        has_stars: true,
        has_clouds: false,
        orb: Some(rgb(0xF4F1D8)),
        weather: Weather::Clear,
        weather_color: rgb(0xFFFFFF),
    },
    Biome {
        name: "Arcane Library",
        sky_top: rgb(0x1A1020),
        sky_bottom: rgb(0x3A2420),
        far: rgb(0x5C3E2A),
        near: rgb(0x34221A),
        near_shade: rgb(0x1C1210),
        ground_top: rgb(0xB0804A),
        ground_top_shade: rgb(0x7A5430),
        ground_fill: rgb(0x4A3428),
        ground_dark: rgb(0x30221A),
        platform: rgb(0x6A4A8A),
        platform_shade: rgb(0x4A3266),
        block: rgb(0x3A2A5A),
        block_glow: rgb(0xFFD27A),
        accent: rgb(0xFFE38A),
        backdrop: Backdrop::Library,
        has_stars: false,
        has_clouds: false,
        orb: None,
        weather: Weather::Dust,
        weather_color: rgb(0xFFE7A8),
    },
    Biome {
        name: "Astral Isles",
        sky_top: rgb(0x0A0620),
        sky_bottom: rgb(0x3A1E6A),
        far: rgb(0x2A1A50),
        near: rgb(0x1E1440),
        near_shade: rgb(0x140C2E),
        ground_top: rgb(0xB48CFF),
        ground_top_shade: rgb(0x8060CC),
        ground_fill: rgb(0x3A2A66),
        ground_dark: rgb(0x261A46),
        platform: rgb(0x7CE8FF),
        platform_shade: rgb(0x4AA8C8),
        block: rgb(0x2E1E5A),
        block_glow: rgb(0xFF9CF2),
        accent: rgb(0x9CFFF0),
        backdrop: Backdrop::Islands,
        has_stars: true,
        has_clouds: false,
        orb: Some(rgb(0xFFD9F4)),
        weather: Weather::Clear,
        weather_color: rgb(0xFFFFFF),
    },
];

pub const OUTLINE: u32 = rgb(0x1A1026);

pub struct Art {
    pub hero_run: [Sprite; 2],
    pub hero_jump: Sprite,
    pub hero_pound: Sprite,
    /// Staff thrust forward, casting (16 wide: the staff reaches past the usual 12).
    pub hero_cast: [Sprite; 2],
    pub slime: [Sprite; 2],
    pub slime_squished: Sprite,
    pub bat: [Sprite; 2],
    pub mote: [Sprite; 4],
}

/// 8×8 rune glyphs carved into rune blocks.
pub const RUNES: [[&str; 8]; 4] = [
    ["...##...", "..#..#..", "...##...", "#..##..#", ".######.", "...##...", "..#..#..", ".#....#."],
    ["#......#", ".#....#.", "..#..#..", "...##...", "...##...", "..#..#..", ".#....#.", "#......#"],
    ["..####..", ".#....#.", "#..##..#", "#.#..#.#", "#.#..#.#", "#..##..#", ".#....#.", "..####.."],
    ["...#....", "..###...", ".#.#.#..", "...#....", "...#.#..", "...###..", "...#.#..", "...#...."],
];

pub static ART: LazyLock<Art> = LazyLock::new(|| {
    // The apprentice: a hooded spellcaster in violet with a staff topped by a green gem (Grimoire's colors).
    let hero = [
        ('k', OUTLINE),
        ('h', rgb(0x8A5CE6)),
        ('H', rgb(0x5E3BAA)),
        ('s', rgb(0xF2C29B)),
        ('e', OUTLINE),
        ('y', rgb(0xFFE066)),
        ('g', rgb(0x74FFAD)),
        ('G', rgb(0xE6FFF0)),
        ('w', rgb(0x9B6A35)),
        ('b', rgb(0x3A2A50)),
    ];
    let body = [
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
    ];
    let hero_with = |legs: [&'static str; 2]| {
        let rows: Vec<&str> = body.iter().copied().chain(legs).collect();
        Sprite::new(&rows, &hero)
    };
    let slime = [('k', OUTLINE), ('m', rgb(0x46D3A0)), ('M', rgb(0xB8FFE0)), ('d', rgb(0x2A9C78)), ('e', OUTLINE)];
    let bat = [('k', OUTLINE), ('m', rgb(0x6A4C93)), ('M', rgb(0x9E7CC8)), ('e', rgb(0xFF5C7A))];
    let mote = [('y', rgb(0xFFD84D)), ('W', rgb(0xFFFBE6)), ('o', rgb(0xC98A1E))];
    Art {
        hero_run: [hero_with(["..kb..kb..w.", ".kb....kb.w."]), hero_with(["...kbkb...w.", "...kb.kb..w."])],
        hero_jump: hero_with(["..kbkb....w.", "..........w."]),
        hero_pound: hero_with([".kbk..kbk.w.", "..........w."]),
        hero_cast: {
            let top = [
                ".kk.............",
                ".khk............",
                "..khk...........",
                "..khhk..........",
                "..khyhk.........",
                ".khhhhhk........",
                "kkkkkkkkk.......",
                "..ksseskk.......",
                "..ksssssk.......",
                "..khhhhkk.....g.",
                ".khhhhhhsswwwgGg",
                ".khhhhhHk.....g.",
                "khhhhhhHHk......",
                "kHHHHHHHHk......",
            ];
            let cast_with = |legs: [&'static str; 2]| {
                let rows: Vec<&str> = top.iter().copied().chain(legs).collect();
                Sprite::new(&rows, &hero)
            };
            [cast_with(["..kb..kb........", ".kb....kb......."]), cast_with(["..kbkb..........", "................"])]
        },
        slime: [
            Sprite::new(
                &[
                    "............",
                    "....kkkk....",
                    "..kkmmmmkk..",
                    ".kmMMmmmmmk.",
                    "kmMmmmmmmmmk",
                    "kmmekmmekmmk",
                    "kmmmmmmmmmmk",
                    "kddddddddddk",
                    ".kkkkkkkkkk.",
                ],
                &slime,
            ),
            Sprite::new(
                &[
                    "............",
                    "............",
                    "...kkkkkk...",
                    ".kkmMMmmmkk.",
                    "kmMmmmmmmmmk",
                    "kmmekmmekmmk",
                    "kmmmmmmmmmmk",
                    "kddddddddddk",
                    "kkkkkkkkkkkk",
                ],
                &slime,
            ),
        ],
        slime_squished: Sprite::new(&["..kkkkkkkk..", "kkmmmmmmmmkk", "kmmkkmmkkmmk", "kkkkkkkkkkkk"], &slime),
        bat: [
            Sprite::new(
                &[
                    "k..........k",
                    "kk........kk",
                    "kMk.kkkk.kMk",
                    "kmMkmmmmkMmk",
                    ".kmmmemmmmk.",
                    "..kkmmmmkk..",
                    "....kkkk....",
                    "............",
                ],
                &bat,
            ),
            Sprite::new(
                &[
                    "............",
                    "....kkkk....",
                    "...kmmmmk...",
                    "..kmemmemk..",
                    "kkmmmmmmmmkk",
                    "kMmkkmmkkmMk",
                    "kMk..kk..kMk",
                    "kk........kk",
                ],
                &bat,
            ),
        ],
        mote: [
            Sprite::new(&["..y..", ".yWy.", "yWWWo", ".yWo.", "..o.."], &mote),
            Sprite::new(&["..y..", "..W..", ".yWo.", "..W..", "..o.."], &mote),
            Sprite::new(&["..y..", ".yWy.", "yWWWo", ".yWo.", "..o.."], &mote),
            Sprite::new(&["..y..", ".yWy.", ".WWW.", ".yWo.", "..o.."], &mote),
        ],
    }
});

//! Biome palettes and sprites (port of `SpellrunArt.swift`). Sprites face right.

use std::sync::LazyLock;

use super::canvas::{rgb, Sprite};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    Mountains,
    Pines,
    Crystals,
    Castle,
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
}

pub static BIOMES: [Biome; 4] = [
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
    },
];

pub const OUTLINE: u32 = rgb(0x1A1026);

pub struct Art {
    pub hero_run: [Sprite; 2],
    pub hero_jump: Sprite,
    pub hero_pound: Sprite,
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

//! Doom's key codes (doomkeys.h). Letters and digits are their lowercase ASCII codes.

pub const RIGHT: u8 = 0xae;
pub const LEFT: u8 = 0xac;
pub const UP: u8 = 0xad;
pub const DOWN: u8 = 0xaf;
pub const STRAFE_LEFT: u8 = 0xa0;
pub const STRAFE_RIGHT: u8 = 0xa1;
pub const USE: u8 = 0xa2;
pub const FIRE: u8 = 0xa3;
pub const ESCAPE: u8 = 27;
pub const ENTER: u8 = 13;
pub const TAB: u8 = 9;
pub const BACKSPACE: u8 = 0x7f;
pub const PAUSE: u8 = 0xff;
pub const RUN: u8 = 0x80 + 0x36; // right shift
pub const STRAFE: u8 = 0x80 + 0x38; // alt
pub const EQUALS: u8 = 0x3d;
pub const MINUS: u8 = 0x2d;

/// F1...F12.
pub fn function(n: u8) -> u8 {
    match n {
        1..=10 => 0x80 + 0x3a + n,
        11 => 0x80 + 0x57,
        _ => 0x80 + 0x58,
    }
}

/// A printable key (y/n prompts, weapon numbers, cheat codes).
pub fn character(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    (c.is_ascii_graphic() || c == ' ').then_some(c as u8)
}

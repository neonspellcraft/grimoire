//! Terminal-grimoire palette, from the Swift app's `Theme` (spectral green LCD text, arcane violet), pushed
//! darker and more phosphor-like.

use super::screen::Rgba;

pub const BACKGROUND: [f64; 3] = [0.018, 0.014, 0.03];

pub const GREEN: Rgba = [0.45, 1.0, 0.68, 1.0];
pub const GREEN_DIM: Rgba = [0.45, 1.0, 0.68, 0.5];
pub const GREEN_FAINT: Rgba = [0.45, 1.0, 0.68, 0.16];
pub const VIOLET: Rgba = [0.70, 0.50, 1.0, 1.0];
pub const VIOLET_DIM: Rgba = [0.70, 0.50, 1.0, 0.55];
pub const FRAME: Rgba = [0.70, 0.50, 1.0, 0.38];
pub const SELECTION: Rgba = [0.70, 0.50, 1.0, 0.16];

pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    [color[0], color[1], color[2], color[3] * alpha]
}

/// Elder Futhark, for glitch reveals and decoration.
pub const RUNES: &[char] = &[
    'ᚠ', 'ᚢ', 'ᚦ', 'ᚨ', 'ᚱ', 'ᚲ', 'ᚷ', 'ᚹ', 'ᚺ', 'ᚾ', 'ᛁ', 'ᛃ', 'ᛇ', 'ᛈ', 'ᛉ', 'ᛊ', 'ᛏ', 'ᛒ', 'ᛖ', 'ᛗ', 'ᛚ', 'ᛜ',
    'ᛞ', 'ᛟ',
];

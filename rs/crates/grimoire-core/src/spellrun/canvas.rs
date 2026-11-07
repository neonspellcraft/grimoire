//! A tiny software framebuffer for pixel art (port of `PixelCanvas.swift`), plus the seedable RNG,
//! sprites, the 3×5 pixel font and ordered dither the game is drawn with.

use std::collections::HashMap;

/// Small, fast, seedable RNG (SplitMix64), so generated levels are reproducible in tests.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in 0..<1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform integer in `low..=high` (multiply-shift, so no modulo bias worth mentioning).
    pub fn int(&mut self, low: i64, high: i64) -> i64 {
        let span = (high - low + 1) as u128;
        low + ((self.next_u64() as u128 * span) >> 64) as i64
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }
}

/// Packs 0xRRGGBB into an RGBA8 pixel (memory order R, G, B, A on little-endian machines).
pub const fn rgb(hex: u32) -> u32 {
    let (r, g, b) = ((hex >> 16) & 0xFF, (hex >> 8) & 0xFF, hex & 0xFF);
    0xFF00_0000 | (b << 16) | (g << 8) | r
}

/// Linear blend of two RGBA8 colors.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        ((x + (y - x) * t) as u32) << shift
    };
    0xFF00_0000 | channel(16) | channel(8) | channel(0)
}

/// 4×4 ordered-dither threshold (0...1), for banded gradients and pixel dissolves.
pub fn bayer(x: i64, y: i64) -> f32 {
    const MATRIX: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    (MATRIX[((y & 3) * 4 + (x & 3)) as usize] + 0.5) / 16.0
}

/// Pixel art as strings: one character per pixel, "." for transparent, other characters looked up in `palette`.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub rows: Vec<Vec<char>>,
    pub palette: HashMap<char, u32>,
}

impl Sprite {
    pub fn new(rows: &[&str], palette: &[(char, u32)]) -> Sprite {
        Sprite { rows: rows.iter().map(|r| r.chars().collect()).collect(), palette: palette.iter().copied().collect() }
    }

    pub fn width(&self) -> i64 {
        self.rows.first().map_or(0, |r| r.len() as i64)
    }

    pub fn height(&self) -> i64 {
        self.rows.len() as i64
    }
}

/// A low-resolution RGBA8 framebuffer with the few drawing operations a pixel-art game needs.
/// Origin is the top-left; everything is clipped to the canvas.
pub struct PixelCanvas {
    pub width: i64,
    pub height: i64,
    pub pixels: Vec<u32>,
}

impl PixelCanvas {
    pub fn new(width: usize, height: usize) -> PixelCanvas {
        PixelCanvas { width: width as i64, height: height as i64, pixels: vec![0xFF00_0000; width * height] }
    }

    #[inline]
    fn index(&self, x: i64, y: i64) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then(|| (y * self.width + x) as usize)
    }

    pub fn pixel(&self, x: i64, y: i64) -> u32 {
        self.index(x, y).map_or(0, |i| self.pixels[i])
    }

    #[inline]
    pub fn set(&mut self, x: i64, y: i64, color: u32) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = color;
        }
    }

    pub fn fill(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    pub fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, color: u32) {
        let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w).min(self.width), (y + h).min(self.height));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for row in y0..y1 {
            let base = (row * self.width) as usize;
            self.pixels[base + x0 as usize..base + x1 as usize].fill(color);
        }
    }

    /// Mixes `color` over the existing pixel by `alpha` (0...1).
    pub fn blend(&mut self, x: i64, y: i64, color: u32, alpha: f32) {
        if alpha <= 0.0 {
            return;
        }
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = mix(self.pixels[i], color, alpha);
        }
    }

    /// Brightens the existing pixel by `color × amount`, like additive light.
    pub fn add(&mut self, x: i64, y: i64, color: u32, amount: f32) {
        if amount <= 0.0 {
            return;
        }
        let Some(i) = self.index(x, y) else { return };
        let d = self.pixels[i];
        let channel = |shift: u32| {
            let sum = ((d >> shift) & 0xFF) as f32 + ((color >> shift) & 0xFF) as f32 * amount;
            (sum.min(255.0) as u32) << shift
        };
        self.pixels[i] = 0xFF00_0000 | channel(16) | channel(8) | channel(0);
    }

    /// A soft round glow centered on (cx, cy), added to the image.
    pub fn glow(&mut self, cx: i64, cy: i64, radius: i64, color: u32, strength: f32) {
        if radius <= 0 || strength <= 0.0 {
            return;
        }
        let r2 = (radius * radius) as f32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = (dx * dx + dy * dy) as f32;
                if d2 > r2 {
                    continue;
                }
                let falloff = 1.0 - d2 / r2;
                self.add(cx + dx, cy + dy, color, strength * falloff * falloff);
            }
        }
    }

    /// Draws a sprite with its top-left at (x, y). Transparent pixels are skipped.
    pub fn draw(&mut self, sprite: &Sprite, x: i64, y: i64, flipped: bool) {
        for (row, line) in sprite.rows.iter().enumerate() {
            for (column, symbol) in line.iter().enumerate() {
                if *symbol == '.' {
                    continue;
                }
                let Some(&color) = sprite.palette.get(symbol) else { continue };
                let dx = if flipped { sprite.width() - 1 - column as i64 } else { column as i64 };
                self.set(x + dx, y + row as i64, color);
            }
        }
    }

    /// Draws text in the 3×5 pixel font, one pixel between letters.
    pub fn text(&mut self, string: &str, x: i64, y: i64, color: u32) {
        let mut cursor = x;
        for character in string.to_uppercase().chars() {
            if let Some(glyph) = glyph(character) {
                for (row, line) in glyph.iter().enumerate() {
                    for (column, bit) in line.chars().enumerate() {
                        if bit == '#' {
                            self.set(cursor + column as i64, y + row as i64, color);
                        }
                    }
                }
            }
            cursor += 4;
        }
    }
}

fn glyph(c: char) -> Option<[&'static str; 5]> {
    Some(match c {
        'A' => [".#.", "#.#", "###", "#.#", "#.#"],
        'B' => ["##.", "#.#", "##.", "#.#", "##."],
        'C' => [".##", "#..", "#..", "#..", ".##"],
        'D' => ["##.", "#.#", "#.#", "#.#", "##."],
        'E' => ["###", "#..", "##.", "#..", "###"],
        'F' => ["###", "#..", "##.", "#..", "#.."],
        'G' => [".##", "#..", "#.#", "#.#", ".##"],
        'H' => ["#.#", "#.#", "###", "#.#", "#.#"],
        'I' => ["###", ".#.", ".#.", ".#.", "###"],
        'J' => ["..#", "..#", "..#", "#.#", ".#."],
        'K' => ["#.#", "#.#", "##.", "#.#", "#.#"],
        'L' => ["#..", "#..", "#..", "#..", "###"],
        'M' => ["#.#", "###", "###", "#.#", "#.#"],
        'N' => ["##.", "#.#", "#.#", "#.#", "#.#"],
        'O' => [".#.", "#.#", "#.#", "#.#", ".#."],
        'P' => ["##.", "#.#", "##.", "#..", "#.."],
        'Q' => [".#.", "#.#", "#.#", "##.", ".##"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        'S' => [".##", "#..", ".#.", "..#", "##."],
        'T' => ["###", ".#.", ".#.", ".#.", ".#."],
        'U' => ["#.#", "#.#", "#.#", "#.#", "###"],
        'V' => ["#.#", "#.#", "#.#", "#.#", ".#."],
        'W' => ["#.#", "#.#", "###", "###", "#.#"],
        'X' => ["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => ["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => ["###", "..#", ".#.", "#..", "###"],
        '0' => ["###", "#.#", "#.#", "#.#", "###"],
        '1' => [".#.", "##.", ".#.", ".#.", "###"],
        '2' => ["##.", "..#", ".#.", "#..", "###"],
        '3' => ["##.", "..#", ".#.", "..#", "##."],
        '4' => ["#.#", "#.#", "###", "..#", "..#"],
        '5' => ["###", "#..", "##.", "..#", "##."],
        '6' => [".##", "#..", "###", "#.#", "###"],
        '7' => ["###", "..#", ".#.", ".#.", ".#."],
        '8' => ["###", "#.#", "###", "#.#", "###"],
        '9' => ["###", "#.#", "###", "..#", "##."],
        '-' => ["...", "...", "###", "...", "..."],
        ' ' => ["...", "...", "...", "...", "..."],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_packs_memory_order_rgba() {
        assert_eq!(rgb(0x112233).to_le_bytes(), [0x11, 0x22, 0x33, 0xFF]);
        assert_eq!(mix(rgb(0x000000), rgb(0xFFFFFF), 0.5), rgb(0x7F7F7F));
    }

    #[test]
    fn rng_is_reproducible_and_in_range() {
        let (mut a, mut b) = (SplitMix64::new(9), SplitMix64::new(9));
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
            let n = a.int(-1, 3);
            assert!((-1..=3).contains(&n));
            b.int(-1, 3);
        }
    }

    #[test]
    fn drawing_clips_to_the_canvas() {
        let mut canvas = PixelCanvas::new(8, 4);
        canvas.fill_rect(-3, -3, 100, 100, rgb(0x00FF00));
        canvas.text("A1", 6, 2, rgb(0xFF0000));
        canvas.glow(0, 0, 20, rgb(0xFFFFFF), 1.0);
        assert_eq!(canvas.pixels.len(), 32);
        assert_eq!(canvas.pixel(-1, 0), 0);
    }
}

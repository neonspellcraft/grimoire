//! Fonts for the cell grid: one monospace face plus fallbacks for runes and sigils, rasterized on demand into
//! a single-channel atlas. Box-drawing and block characters are drawn as exact rectangles instead, so borders
//! and bars join seamlessly between cells (the way good terminals do it).

use std::collections::HashMap;

use fontdue::{Font, FontSettings};

/// JetBrains Mono for the grid; the Noto faces fill in runes, sigils and arrows it doesn't have.
const PRIMARY: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono.ttf");
/// Grid text size in logical pixels.
pub const LOGICAL_PX: f32 = 14.0;

const RUNIC: &[u8] = include_bytes!("../../assets/fonts/NotoSansRunic.ttf");
const SYMBOLS2: &[u8] = include_bytes!("../../assets/fonts/NotoSansSymbols2.ttf");
const SYMBOLS: &[u8] = include_bytes!("../../assets/fonts/NotoSansSymbols.ttf");
const MATH: &[u8] = include_bytes!("../../assets/fonts/NotoSansMath.ttf");

/// Where a rasterized glyph sits in the atlas and relative to its cell's top-left corner, in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub atlas: [u32; 4], // x, y, w, h
    pub offset: [f32; 2],
}

pub struct Fonts {
    faces: Vec<Font>,
    pub px: f32,
    pub cell_w: u32,
    pub cell_h: u32,
    baseline: f32,
}

impl Fonts {
    pub fn new(scale: f32) -> Fonts {
        let load = |bytes: &[u8]| Font::from_bytes(bytes, FontSettings::default()).expect("bundled font parses");
        let faces = vec![load(PRIMARY), load(RUNIC), load(SYMBOLS2), load(SYMBOLS), load(MATH)];
        let px = (LOGICAL_PX * scale).round();
        let primary = &faces[0];
        let cell_w = primary.metrics('M', px).advance_width.round().max(1.0) as u32;
        let line = primary.horizontal_line_metrics(px).expect("horizontal metrics");
        let glyph_h = line.ascent - line.descent;
        let cell_h = (glyph_h * 1.22).round().max(1.0) as u32;
        let baseline = ((cell_h as f32 - glyph_h) / 2.0 + line.ascent).round();
        Fonts { faces, px, cell_w, cell_h, baseline }
    }

    /// Rasterizes `ch` from the first face that has it. Glyphs from fallback faces are centered and scaled
    /// down if they'd spill far outside the cell.
    fn rasterize(&self, ch: char) -> Option<(fontdue::Metrics, Vec<u8>, [f32; 2])> {
        let (index, font) = self.faces.iter().enumerate().find(|(_, f)| f.lookup_glyph_index(ch) != 0)?;
        let mut px = self.px;
        if index > 0 {
            let m = font.metrics(ch, px);
            let fit = (self.cell_w as f32 * 1.25 / m.advance_width.max(1.0))
                .min(self.cell_h as f32 * 0.95 / (m.height as f32).max(1.0))
                .min(1.0);
            px *= fit;
        }
        let (metrics, bitmap) = font.rasterize(ch, px);
        let x = if index > 0 {
            ((self.cell_w as f32 - metrics.advance_width) / 2.0).round() + metrics.xmin as f32
        } else {
            metrics.xmin as f32
        };
        let y = self.baseline - (metrics.ymin as f32 + metrics.height as f32);
        Some((metrics, bitmap, [x, y]))
    }
}

/// A rectangle inside a cell (0...1 of its width and height) with a coverage alpha.
pub type Block = ([f32; 4], f32);

/// Exact shapes for box-drawing and block characters, or None to use the font.
/// Lines are `line` pixels thick (pass the cell's thickness in pixels and its size).
pub fn procedural(ch: char, cell_w: f32, cell_h: f32, line: f32) -> Option<Vec<Block>> {
    let shade = |alpha| Some(vec![([0.0, 0.0, 1.0, 1.0], alpha)]);
    match ch {
        '░' => return shade(0.16),
        '▒' => return shade(0.4),
        '▓' => return shade(0.72),
        // Spectrum bars: full and lower-eighth blocks with a gap between neighbours.
        '█' => return Some(vec![([0.12, 0.0, 0.76, 1.0], 1.0)]),
        // Seek line: a thick glowing stroke for played time, a hairline for what's left.
        '━' => {
            let thick = (line * 3.0 / cell_h).min(1.0);
            let top = ((cell_h - line * 3.0) / 2.0).round() / cell_h;
            return Some(vec![([0.0, top, 1.0, thick], 1.0)]);
        }
        '▀' => return Some(vec![([0.0, 0.0, 1.0, 0.5], 1.0)]),
        '▌' => return Some(vec![([0.0, 0.0, 0.5, 1.0], 1.0)]),
        '▐' => return Some(vec![([0.5, 0.0, 0.5, 1.0], 1.0)]),
        '▔' => return Some(vec![([0.0, 0.0, 1.0, 0.125], 1.0)]),
        '\u{2581}'..='\u{2587}' => {
            let eighths = (ch as u32 - 0x2580) as f32 / 8.0;
            return Some(vec![([0.12, 1.0 - eighths, 0.76, eighths], 1.0)]);
        }
        _ => {}
    }
    // Light box drawing: which arms reach the cell's left, right, top and bottom edges.
    let (l, r, u, d) = match ch {
        '─' => (true, true, false, false),
        '│' => (false, false, true, true),
        '┌' => (false, true, false, true),
        '┐' => (true, false, false, true),
        '└' => (false, true, true, false),
        '┘' => (true, false, true, false),
        '├' => (false, true, true, true),
        '┤' => (true, false, true, true),
        '┬' => (true, true, false, true),
        '┴' => (true, true, true, false),
        '┼' => (true, true, true, true),
        '╴' => (true, false, false, false),
        '╶' => (false, true, false, false),
        _ => return None,
    };
    let (tw, th) = (line / cell_w, line / cell_h);
    let (cx, cy) = (((cell_w - line) / 2.0).round() / cell_w, ((cell_h - line) / 2.0).round() / cell_h);
    let mut blocks = Vec::new();
    // Every pixel is covered exactly once, so translucent lines don't get bright dots where arms meet.
    // The center pixel belongs to a horizontal arm when there is one, otherwise to a vertical arm.
    if r {
        blocks.push(([cx, cy, 1.0 - cx, th], 1.0));
    }
    if l {
        blocks.push(([0.0, cy, if r { cx } else { cx + tw }, th], 1.0));
    }
    let horizontal = l || r;
    if u {
        let reach = if horizontal || d { cy } else { cy + th };
        blocks.push(([cx, 0.0, tw, reach], 1.0));
    }
    if d {
        let start = if horizontal { cy + th } else { cy };
        blocks.push(([cx, start, tw, 1.0 - start], 1.0));
    }
    Some(blocks)
}

/// Single-channel glyph atlas with simple shelf packing. Starts over if it fills up.
pub struct Atlas {
    pub size: u32,
    pub pixels: Vec<u8>,
    glyphs: HashMap<char, Option<Glyph>>,
    shelf: [u32; 3], // x, y, height
    pub dirty: bool,
}

impl Atlas {
    pub fn new(size: u32) -> Atlas {
        Atlas { size, pixels: vec![0; (size * size) as usize], glyphs: HashMap::new(), shelf: [1, 1, 0], dirty: true }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
        self.glyphs.clear();
        self.shelf = [1, 1, 0];
        self.dirty = true;
    }

    pub fn glyph(&mut self, fonts: &Fonts, ch: char) -> Option<Glyph> {
        if let Some(glyph) = self.glyphs.get(&ch) {
            return *glyph;
        }
        let glyph = self.insert(fonts, ch);
        self.glyphs.insert(ch, glyph);
        glyph
    }

    fn insert(&mut self, fonts: &Fonts, ch: char) -> Option<Glyph> {
        let (metrics, bitmap, offset) = fonts.rasterize(ch)?;
        let (w, h) = (metrics.width as u32, metrics.height as u32);
        if w == 0 || h == 0 {
            return None; // a space
        }
        if self.shelf[0] + w + 1 > self.size {
            self.shelf = [1, self.shelf[1] + self.shelf[2] + 1, 0];
        }
        if self.shelf[1] + h + 1 > self.size {
            return None; // full; the renderer clears and rebuilds next frame
        }
        let [x, y, _] = self.shelf;
        for row in 0..h {
            let src = &bitmap[(row * w) as usize..((row + 1) * w) as usize];
            let start = ((y + row) * self.size + x) as usize;
            self.pixels[start..start + w as usize].copy_from_slice(src);
        }
        self.shelf[0] += w + 1;
        self.shelf[2] = self.shelf[2].max(h);
        self.dirty = true;
        Some(Glyph { atlas: [x, y, w, h], offset })
    }

    pub fn is_full(&self) -> bool {
        self.shelf[1] + self.shelf[2] + 64 > self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_cover_the_glyphs_the_ui_uses() {
        for scale in [1.0, 1.5, 2.0] {
            let fonts = Fonts::new(scale);
            assert!(fonts.cell_w > 4 && fonts.cell_h > 8, "cell {}x{}", fonts.cell_w, fonts.cell_h);
            let mut atlas = Atlas::new(1024);
            let missing: Vec<char> = "GRIMOIRE moonrise 0123456789:/‹›▮▯◁▷▶❚✦⛧◆ᚠᚱᛉᛟ…·⟲⇄◈⚙←→↑↓⌘–×+-"
                .chars()
                .filter(|&c| c != ' ' && procedural(c, 8.0, 16.0, 1.0).is_none() && atlas.glyph(&fonts, c).is_none())
                .collect();
            assert!(missing.is_empty(), "missing glyphs: {missing:?}");
        }
    }

    #[test]
    fn box_lines_meet_in_the_middle() {
        let across = procedural('─', 8.0, 16.0, 1.0).unwrap();
        let down = procedural('│', 8.0, 16.0, 1.0).unwrap();
        // The horizontal line's row and the vertical line's column are the same centered pixel.
        assert_eq!(across[0].0[1] * 16.0, 8.0);
        assert_eq!(down[0].0[0] * 8.0, 4.0);
        assert!(procedural('a', 8.0, 16.0, 1.0).is_none());
        // A corner covers each pixel once: total area is one arm across plus one arm down, minus nothing.
        let corner = procedural('┌', 8.0, 16.0, 1.0).unwrap();
        let area: f32 = corner.iter().map(|(r, _)| r[2] * 8.0 * r[3] * 16.0).sum();
        assert_eq!(area, 4.0 + 7.0); // right arm 4 px (center + 3), down arm 7 px below the center
    }
}

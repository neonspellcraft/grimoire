//! The character grid the main window is drawn into, plus the clickable areas laid over it.
//! Pure data, so layout and hit-testing are unit tested without a GPU.

pub type Rgba = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgba,
    /// None leaves the window background showing.
    pub bg: Option<Rgba>,
}

impl Cell {
    pub const BLANK: Cell = Cell { ch: ' ', fg: [0.0; 4], bg: None };
}

/// A rectangle in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl CellRect {
    pub fn contains(&self, col: u16, row: u16) -> bool {
        col >= self.x && col < self.x + self.w && row >= self.y && row < self.y + self.h
    }
}

/// Something the pointer can do in a region. `T` is the app's action type.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit<T> {
    pub rect: CellRect,
    pub action: T,
}

pub struct Screen<T> {
    pub cols: u16,
    pub rows: u16,
    cells: Vec<Cell>,
    pub hits: Vec<Hit<T>>,
}

impl<T: Clone> Screen<T> {
    pub fn new(cols: u16, rows: u16) -> Self {
        Screen { cols, rows, cells: vec![Cell::BLANK; cols as usize * rows as usize], hits: Vec::new() }
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[cfg(test)]
    pub fn get(&self, col: u16, row: u16) -> Option<&Cell> {
        (col < self.cols && row < self.rows).then(|| &self.cells[row as usize * self.cols as usize + col as usize])
    }

    pub fn set(&mut self, col: u16, row: u16, ch: char, fg: Rgba) {
        if col < self.cols && row < self.rows {
            let cell = &mut self.cells[row as usize * self.cols as usize + col as usize];
            cell.ch = ch;
            cell.fg = fg;
        }
    }

    pub fn set_bg(&mut self, rect: CellRect, bg: Rgba) {
        for row in rect.y..(rect.y + rect.h).min(self.rows) {
            for col in rect.x..(rect.x + rect.w).min(self.cols) {
                self.cells[row as usize * self.cols as usize + col as usize].bg = Some(bg);
            }
        }
    }

    /// Writes `text` from `col`, clipped at `max` cells (and the grid edge). Returns the cells written.
    pub fn text(&mut self, col: u16, row: u16, text: &str, fg: Rgba, max: u16) -> u16 {
        let mut written = 0;
        for ch in text.chars() {
            if written >= max || col + written >= self.cols {
                break;
            }
            self.set(col + written, row, ch, fg);
            written += 1;
        }
        written
    }

    /// Like `text`, but ends with "…" when it doesn't fit.
    pub fn text_ellipsized(&mut self, col: u16, row: u16, text: &str, fg: Rgba, max: u16) -> u16 {
        if text.chars().count() as u16 <= max || max == 0 {
            return self.text(col, row, text, fg, max);
        }
        let head: String = text.chars().take(max as usize - 1).collect();
        let n = self.text(col, row, &head, fg, max - 1);
        self.set(col + n, row, '…', fg);
        n + 1
    }

    /// Text ending at `end_col` (exclusive).
    pub fn text_right(&mut self, end_col: u16, row: u16, text: &str, fg: Rgba) -> u16 {
        let len = text.chars().count() as u16;
        let start = end_col.saturating_sub(len);
        self.text(start, row, text, fg, len);
        start
    }

    pub fn hline(&mut self, col: u16, row: u16, len: u16, ch: char, fg: Rgba) {
        for i in 0..len {
            self.set(col + i, row, ch, fg);
        }
    }

    pub fn hit(&mut self, rect: CellRect, action: T) {
        self.hits.push(Hit { rect, action });
    }

    /// The topmost action under a cell (the most recently added area wins).
    pub fn hit_test(&self, col: u16, row: u16) -> Option<&Hit<T>> {
        self.hits.iter().rev().find(|h| h.rect.contains(col, row))
    }

    /// The whole row as a string, for tests.
    #[cfg(test)]
    pub fn row_text(&self, row: u16) -> String {
        (0..self.cols).map(|c| self.get(c, row).map_or(' ', |cell| cell.ch)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Rgba = [1.0; 4];

    #[test]
    fn text_clips_and_ellipsizes() {
        let mut s: Screen<()> = Screen::new(10, 1);
        assert_eq!(s.text(7, 0, "abcdef", W, 99), 3);
        assert_eq!(s.row_text(0), "       abc");
        let mut s: Screen<()> = Screen::new(10, 1);
        s.text_ellipsized(0, 0, "moonrise over", W, 6);
        assert_eq!(s.row_text(0), "moonr…    ");
        s.text_right(10, 0, "5:12", W);
        assert_eq!(s.row_text(0), "moonr…5:12");
    }

    #[test]
    fn hit_test_prefers_latest_area() {
        let mut s = Screen::new(20, 5);
        s.hit(CellRect { x: 0, y: 0, w: 20, h: 5 }, "row");
        s.hit(CellRect { x: 2, y: 1, w: 3, h: 1 }, "button");
        assert_eq!(s.hit_test(3, 1).map(|h| h.action), Some("button"));
        assert_eq!(s.hit_test(6, 1).map(|h| h.action), Some("row"));
        assert!(s.hit_test(25, 1).is_none());
    }
}

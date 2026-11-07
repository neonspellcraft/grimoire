//! Which playlist rows are selected, Finder-style: a click selects one, ⌘/Ctrl-click adds or removes one,
//! Shift-click selects the range from the anchor.

use std::collections::HashSet;

use grimoire_core::TrackId;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    /// Where arrow keys move from, what Enter plays, where Shift-click ranges start.
    pub anchor: Option<TrackId>,
    /// Every selected track; contains `anchor` when there is one.
    pub tracks: HashSet<TrackId>,
}

impl Selection {
    pub fn contains(&self, id: TrackId) -> bool {
        self.tracks.contains(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn only(&mut self, id: TrackId) {
        self.anchor = Some(id);
        self.tracks = HashSet::from([id]);
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.tracks.clear();
    }

    /// ⌘/Ctrl-click: adds or removes one track, keeping the rest. `order` is the list as shown.
    pub fn toggle(&mut self, id: TrackId, order: &[TrackId]) {
        if self.tracks.remove(&id) {
            if self.anchor == Some(id) {
                self.anchor = order.iter().copied().find(|t| self.tracks.contains(t));
            }
        } else {
            self.tracks.insert(id);
            self.anchor = Some(id);
        }
    }

    /// Shift-click: everything between the anchor and `id` in `order`; the anchor stays put.
    pub fn range_to(&mut self, id: TrackId, order: &[TrackId]) {
        let anchor = self.anchor.and_then(|a| order.iter().position(|&t| t == a));
        let (Some(anchor), Some(end)) = (anchor, order.iter().position(|&t| t == id)) else {
            self.only(id);
            return;
        };
        let (low, high) = (anchor.min(end), anchor.max(end));
        self.tracks = order[low..=high].iter().copied().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grimoire_core::Track;

    fn ids(n: usize) -> Vec<TrackId> {
        (0..n).map(|i| Track::new(format!("/m/{i}.mp3")).id).collect()
    }

    #[test]
    fn click_command_click_and_shift_click() {
        let order = ids(6);
        let mut s = Selection::default();
        s.only(order[1]);
        s.toggle(order[4], &order); // ⌘-click adds
        assert!(s.contains(order[1]) && s.contains(order[4]) && s.tracks.len() == 2);
        assert_eq!(s.anchor, Some(order[4]));
        s.toggle(order[4], &order); // ⌘-click again removes, the anchor falls back to what's left
        assert_eq!(s.tracks.len(), 1);
        assert_eq!(s.anchor, Some(order[1]));
        s.range_to(order[3], &order); // shift-click from the anchor (1) to 3
        assert_eq!(s.tracks, order[1..=3].iter().copied().collect());
        s.range_to(order[0], &order); // and back up past it: 0...1
        assert_eq!(s.tracks, order[0..=1].iter().copied().collect());
        s.clear();
        assert!(s.is_empty() && s.anchor.is_none());
        s.range_to(order[2], &order); // shift-click with nothing selected selects just that one
        assert_eq!(s.tracks.len(), 1);
    }
}

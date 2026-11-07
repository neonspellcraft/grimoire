use std::collections::HashSet;

use crate::{Track, TrackId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Playlist {
    tracks: Vec<Track>,
    current_id: Option<TrackId>,
    pub repeat_mode: RepeatMode,
    shuffle: bool,
    shuffle_order: Vec<TrackId>,
}

impl Playlist {
    pub fn new(tracks: Vec<Track>) -> Self {
        Playlist { tracks, ..Default::default() }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn current_id(&self) -> Option<TrackId> {
        self.current_id
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if on != self.shuffle {
            self.shuffle = on;
            self.rebuild_shuffle_order(true);
        }
    }

    pub fn current_index(&self) -> Option<usize> {
        let id = self.current_id?;
        self.tracks.iter().position(|t| t.id == id)
    }

    pub fn current(&self) -> Option<&Track> {
        self.current_index().map(|i| &self.tracks[i])
    }

    pub fn total_duration(&self) -> f64 {
        self.tracks.iter().map(|t| t.duration.unwrap_or(0.0)).sum()
    }

    // Editing

    pub fn append(&mut self, new: Vec<Track>) {
        let at = self.tracks.len();
        self.insert(new, at);
    }

    pub fn insert(&mut self, new: Vec<Track>, index: usize) {
        let index = index.min(self.tracks.len());
        if self.shuffle {
            let mut ids: Vec<TrackId> = new.iter().map(|t| t.id).collect();
            fastrand::shuffle(&mut ids);
            self.shuffle_order.extend(ids);
        }
        self.tracks.splice(index..index, new);
    }

    pub fn remove(&mut self, ids: &HashSet<TrackId>) {
        self.tracks.retain(|t| !ids.contains(&t.id));
        self.shuffle_order.retain(|id| !ids.contains(id));
        if self.current_id.is_some_and(|id| ids.contains(&id)) {
            self.current_id = None;
        }
    }

    pub fn remove_all(&mut self) {
        self.tracks.clear();
        self.shuffle_order.clear();
        self.current_id = None;
    }

    /// Moves the rows at `source` so they land before `destination` (an index in the list before the move),
    /// like SwiftUI's `move(fromOffsets:toOffset:)`.
    pub fn move_rows(&mut self, source: &[usize], destination: usize) {
        let mut source: Vec<usize> = source.iter().copied().filter(|&i| i < self.tracks.len()).collect();
        source.sort_unstable();
        source.dedup();
        let target = destination.min(self.tracks.len()) - source.iter().filter(|&&i| i < destination).count();
        let mut moving = Vec::with_capacity(source.len());
        for &index in source.iter().rev() {
            moving.push(self.tracks.remove(index));
        }
        moving.reverse();
        self.tracks.splice(target..target, moving);
    }

    pub fn update(&mut self, id: TrackId, change: impl FnOnce(&mut Track)) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            change(track);
        }
    }

    // Navigation

    /// Makes `id` the current track. In shuffle mode the remaining order is reshuffled from here.
    pub fn select(&mut self, id: Option<TrackId>) {
        self.current_id = id;
        if self.shuffle {
            self.rebuild_shuffle_order(true);
        }
    }

    /// Moves to and returns the track after the current one, or None at the end of the list.
    /// `automatic` is true when the current track ended by itself (so repeat-one replays it).
    pub fn advance(&mut self, automatic: bool) -> Option<&Track> {
        if self.tracks.is_empty() {
            return None;
        }
        if automatic && self.repeat_mode == RepeatMode::One && self.current().is_some() {
            return self.current();
        }
        let order = self.play_order();
        let Some(position) = self.current_id.and_then(|id| order.iter().position(|&o| o == id)) else {
            return self.set_current(order.first().copied());
        };
        if position + 1 < order.len() {
            return self.set_current(Some(order[position + 1]));
        }
        if self.repeat_mode == RepeatMode::Off {
            return None;
        }
        if self.shuffle {
            self.rebuild_shuffle_order(false);
        }
        let first = self.play_order().first().copied();
        self.set_current(first)
    }

    /// Moves to and returns the track before the current one. At the start it stays put unless repeating.
    pub fn retreat(&mut self) -> Option<&Track> {
        if self.tracks.is_empty() {
            return None;
        }
        let order = self.play_order();
        let Some(position) = self.current_id.and_then(|id| order.iter().position(|&o| o == id)) else {
            return self.set_current(order.first().copied());
        };
        if position > 0 {
            return self.set_current(Some(order[position - 1]));
        }
        if self.repeat_mode == RepeatMode::Off {
            self.current()
        } else {
            self.set_current(order.last().copied())
        }
    }

    fn play_order(&self) -> Vec<TrackId> {
        if self.shuffle {
            self.shuffle_order.clone()
        } else {
            self.tracks.iter().map(|t| t.id).collect()
        }
    }

    fn set_current(&mut self, id: Option<TrackId>) -> Option<&Track> {
        self.current_id = id;
        self.current()
    }

    fn rebuild_shuffle_order(&mut self, current_first: bool) {
        let mut ids: Vec<TrackId> = self.tracks.iter().map(|t| t.id).collect();
        fastrand::shuffle(&mut ids);
        if current_first {
            if let Some(index) = self.current_id.and_then(|id| ids.iter().position(|&o| o == id)) {
                let id = ids.remove(index);
                ids.insert(0, id);
            }
        }
        self.shuffle_order = ids;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_playlist(count: usize) -> Playlist {
        Playlist::new((0..count).map(|i| Track::new(format!("/music/{i}.mp3"))).collect())
    }

    fn ids(p: &Playlist) -> Vec<TrackId> {
        p.tracks().iter().map(|t| t.id).collect()
    }

    #[test]
    fn advances_in_order_and_stops_at_end() {
        let mut p = make_playlist(3);
        let ids = ids(&p);
        assert_eq!(p.advance(true).map(|t| t.id), Some(ids[0]));
        assert_eq!(p.advance(true).map(|t| t.id), Some(ids[1]));
        assert_eq!(p.advance(true).map(|t| t.id), Some(ids[2]));
        assert!(p.advance(true).is_none());
    }

    #[test]
    fn repeat_all_wraps() {
        let mut p = make_playlist(2);
        let ids = ids(&p);
        p.repeat_mode = RepeatMode::All;
        p.select(Some(ids[1]));
        assert_eq!(p.advance(true).map(|t| t.id), Some(ids[0]));
        assert_eq!(p.retreat().map(|t| t.id), Some(ids[1]));
    }

    #[test]
    fn repeat_one_replays_only_when_automatic() {
        let mut p = make_playlist(3);
        let ids = ids(&p);
        p.repeat_mode = RepeatMode::One;
        p.select(Some(ids[0]));
        assert_eq!(p.advance(true).map(|t| t.id), Some(ids[0]));
        assert_eq!(p.advance(false).map(|t| t.id), Some(ids[1]));
    }

    #[test]
    fn retreat_at_start_stays_without_repeat() {
        let mut p = make_playlist(3);
        let ids = ids(&p);
        p.select(Some(ids[0]));
        assert_eq!(p.retreat().map(|t| t.id), Some(ids[0]));
    }

    #[test]
    fn shuffle_visits_every_track_once() {
        let mut p = make_playlist(20);
        p.set_shuffle(true);
        let mut seen = Vec::new();
        while let Some(track) = p.advance(true) {
            seen.push(track.id);
        }
        assert_eq!(seen.len(), 20);
        let seen: HashSet<_> = seen.into_iter().collect();
        assert_eq!(seen, ids(&p).into_iter().collect());
    }

    #[test]
    fn removing_current_clears_it() {
        let mut p = make_playlist(3);
        let ids = ids(&p);
        p.select(Some(ids[1]));
        p.remove(&HashSet::from([ids[1]]));
        assert!(p.current().is_none());
        assert_eq!(p.tracks().len(), 2);
    }

    #[test]
    fn move_keeps_current_track() {
        let mut p = make_playlist(4);
        let ids = ids(&p);
        p.select(Some(ids[0]));
        p.move_rows(&[0], 3);
        assert_eq!(super::tests::ids(&p), [ids[1], ids[2], ids[0], ids[3]]);
        assert_eq!(p.current_index(), Some(2));
    }
}

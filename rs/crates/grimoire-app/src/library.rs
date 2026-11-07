//! The player and the playlist together (port of the Swift `PlayerController`): what's loaded, what plays
//! next, and tags read in the background so rows fill in with titles and durations.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use grimoire_audio::{read_info, FileInfo, PlayState, Player, PlayerEvent};
use grimoire_core::{audio_files, m3u, Playlist, RepeatMode, Track, TrackId};

use crate::settings::Settings;

pub struct Library {
    pub player: Option<Player>,
    pub playlist: Playlist,
    /// Info for the loaded track (format, rate, bitrate, tags).
    pub info: Option<FileInfo>,
    info_sender: Sender<(TrackId, FileInfo)>,
    info_receiver: Receiver<(TrackId, FileInfo)>,
    /// Set whenever something worth saving changes.
    pub dirty: bool,
}

impl Library {
    pub fn new() -> Library {
        let player = match Player::new() {
            Ok(player) => Some(player),
            Err(e) => {
                eprintln!("grimoire: playing without sound: {e}");
                None
            }
        };
        let (info_sender, info_receiver) = mpsc::channel();
        Library { player, playlist: Playlist::default(), info: None, info_sender, info_receiver, dirty: false }
    }

    pub fn state(&self) -> PlayState {
        self.player.as_ref().map_or(PlayState::Stopped, |p| p.state())
    }

    pub fn elapsed(&self) -> f64 {
        self.player.as_ref().map_or(0.0, |p| p.current_time())
    }

    pub fn duration(&self) -> Option<f64> {
        self.player.as_ref().and_then(|p| p.duration()).or_else(|| self.playlist.current().and_then(|t| t.duration))
    }

    pub fn volume(&self) -> f32 {
        self.player.as_ref().map_or(0.0, |p| p.volume())
    }

    pub fn set_volume(&mut self, volume: f32) {
        if let Some(player) = &self.player {
            player.set_volume(volume);
            self.dirty = true;
        }
    }

    /// Adds files, folders and m3u playlists. Starts the first new track if nothing was playing.
    pub fn add(&mut self, paths: Vec<PathBuf>, play: bool) {
        let tracks = expand(paths);
        if tracks.is_empty() {
            return;
        }
        let idle = self.state() == PlayState::Stopped;
        let first = tracks[0].id;
        self.load_info(&tracks);
        self.playlist.append(tracks);
        self.dirty = true;
        if play && idle {
            self.play(first);
        }
    }

    /// Restores the saved playlist, holding the current track paused where it was.
    pub fn restore(&mut self, settings: &Settings) {
        self.set_volume(settings.volume);
        self.playlist.set_shuffle(settings.shuffle);
        self.playlist.repeat_mode = match settings.repeat.as_str() {
            "all" => RepeatMode::All,
            "one" => RepeatMode::One,
            _ => RepeatMode::Off,
        };
        let tracks: Vec<Track> = settings.playlist.iter().filter(|p| p.is_file()).cloned().map(Track::new).collect();
        let current = settings
            .current
            .and_then(|i| settings.playlist.get(i))
            .and_then(|path| tracks.iter().find(|t| &t.path == path))
            .map(|t| t.id);
        self.load_info(&tracks);
        self.playlist.append(tracks);
        if let Some(id) = current {
            self.playlist.select(Some(id));
            if self.load_current(false) && settings.position > 0.0 {
                if let Some(player) = &mut self.player {
                    player.seek(settings.position);
                }
            }
        }
        self.dirty = false;
    }

    pub fn save_into(&self, settings: &mut Settings) {
        settings.volume = self.volume();
        settings.shuffle = self.playlist.shuffle();
        settings.repeat = match self.playlist.repeat_mode {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
        .into();
        settings.playlist = self.playlist.tracks().iter().map(|t| t.path.clone()).collect();
        settings.current = self.playlist.current_index();
        settings.position = if self.state() == PlayState::Stopped { 0.0 } else { self.elapsed() };
    }

    pub fn play(&mut self, id: TrackId) {
        self.playlist.select(Some(id));
        self.load_current(true);
    }

    pub fn toggle(&mut self) {
        if self.playlist.current().is_none() {
            if let Some(first) = self.playlist.tracks().first().map(|t| t.id) {
                self.play(first);
            }
            return;
        }
        if let Some(player) = &mut self.player {
            player.toggle();
        }
    }

    pub fn next(&mut self) {
        if self.playlist.advance(false).is_some() {
            self.load_current(true);
        }
    }

    /// Back to the start of the track, or the previous track if already near the start.
    pub fn previous(&mut self) {
        if self.elapsed() > 3.0 {
            self.seek(0.0);
        } else if self.playlist.retreat().is_some() {
            self.load_current(true);
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        if let Some(player) = &mut self.player {
            player.seek(seconds);
        }
    }

    pub fn toggle_shuffle(&mut self) {
        let on = !self.playlist.shuffle();
        self.playlist.set_shuffle(on);
        self.dirty = true;
    }

    pub fn cycle_repeat(&mut self) {
        self.playlist.repeat_mode = self.playlist.repeat_mode.next();
        self.dirty = true;
    }

    pub fn remove(&mut self, ids: &HashSet<TrackId>) {
        let removing_current = self.playlist.current_id().is_some_and(|id| ids.contains(&id));
        self.playlist.remove(ids);
        if removing_current {
            if let Some(player) = &mut self.player {
                player.stop();
            }
            self.info = None;
        }
        self.dirty = true;
    }

    /// Call every frame: fills in tags as they arrive and moves on when a track ends.
    pub fn tick(&mut self) {
        while let Ok((id, info)) = self.info_receiver.try_recv() {
            self.playlist.update(id, |track| {
                track.title = info.title.clone().or(track.title.take());
                track.artist = info.artist.clone().or(track.artist.take());
                track.duration = info.duration.or(track.duration);
            });
        }
        let event = self.player.as_mut().and_then(|p| p.tick());
        if event == Some(PlayerEvent::Finished) && self.playlist.advance(true).is_some() {
            self.load_current(true);
        }
    }

    /// Loads the playlist's current track, skipping files that won't open. Returns whether one loaded.
    fn load_current(&mut self, play: bool) -> bool {
        let Some(player) = &mut self.player else { return false };
        for _ in 0..self.playlist.tracks().len() {
            let Some(track) = self.playlist.current().cloned() else { return false };
            match player.load(&track.path, play) {
                Ok(info) => {
                    self.playlist.update(track.id, |t| {
                        t.title = info.title.clone().or(t.title.take());
                        t.artist = info.artist.clone().or(t.artist.take());
                        t.duration = info.duration.or(t.duration);
                    });
                    self.info = Some(info);
                    self.dirty = true;
                    return true;
                }
                Err(e) => {
                    eprintln!("grimoire: {e}");
                    if self.playlist.advance(false).is_none() {
                        return false;
                    }
                }
            }
        }
        false
    }

    /// Reads tags and durations on a background thread; results arrive in `tick`.
    fn load_info(&self, tracks: &[Track]) {
        let jobs: Vec<(TrackId, PathBuf)> = tracks.iter().map(|t| (t.id, t.path.clone())).collect();
        let sender = self.info_sender.clone();
        let _ = std::thread::Builder::new().name("grimoire-tags".into()).spawn(move || {
            for (id, path) in jobs {
                if let Ok(info) = read_info(&path) {
                    if sender.send((id, info)).is_err() {
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(1)); // stay out of the decoder's way
            }
        });
    }
}

/// Files, folders and m3u playlists → tracks.
fn expand(paths: Vec<PathBuf>) -> Vec<Track> {
    let mut expanded = Vec::new();
    for path in paths {
        if audio_files::is_playlist(&path) {
            match std::fs::read_to_string(&path) {
                Ok(text) => expanded.extend(m3u::parse(&text, path.parent().unwrap_or(&path))),
                Err(e) => eprintln!("grimoire: {}: {e}", path.display()),
            }
        } else {
            expanded.push(path);
        }
    }
    audio_files::expand(&expanded).into_iter().map(Track::new).collect()
}

/// "FLAC 44.1k 912kbps" for the info line.
pub fn format_line(info: &FileInfo) -> String {
    let rate = info.sample_rate as f64 / 1000.0;
    let rate = if rate.fract() == 0.0 { format!("{rate:.0}k") } else { format!("{rate:.1}k") };
    let channels = match info.channels {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        n => format!("{n}ch"),
    };
    let mut line = format!("{} {rate} {channels}", info.format);
    if let Some(kbps) = info.bitrate {
        line.push_str(&format!(" {kbps}kbps"));
    }
    line
}

/// Case-insensitive subsequence match, so "mnrs" finds "Moonrise".
pub fn matches(query: &str, text: &str) -> bool {
    let mut chars = text.chars().flat_map(char::to_lowercase);
    query.chars().flat_map(char::to_lowercase).filter(|c| !c.is_whitespace()).all(|q| chars.any(|c| c == q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_subsequences() {
        assert!(matches("mnrs", "Moonrise.flac"));
        assert!(matches("MOON rise", "the coven - moonrise"));
        assert!(matches("", "anything"));
        assert!(!matches("rm", "Moon"));
    }

    #[test]
    fn format_line_reads_naturally() {
        let info = FileInfo { format: "FLAC".into(), sample_rate: 44_100, channels: 2, bitrate: Some(912), ..Default::default() };
        assert_eq!(format_line(&info), "FLAC 44.1k stereo 912kbps");
        let info = FileInfo { format: "WAV".into(), sample_rate: 48_000, channels: 1, ..Default::default() };
        assert_eq!(format_line(&info), "WAV 48k mono");
    }
}

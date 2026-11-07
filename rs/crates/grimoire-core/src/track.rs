use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique per `Track` instance (Swift used a `UUID`); two tracks for the same file are still distinct rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(u64);

impl TrackId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        TrackId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: TrackId,
    pub path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
}

impl Track {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Track { id: TrackId::next(), path: path.into(), title: None, artist: None, duration: None }
    }

    pub fn with_tags(path: impl Into<PathBuf>, title: Option<&str>, artist: Option<&str>, duration: Option<f64>) -> Self {
        Track {
            title: title.map(str::to_owned),
            artist: artist.map(str::to_owned),
            duration,
            ..Track::new(path)
        }
    }

    /// "Artist - Title", falling back to the file name when tags are missing.
    pub fn display_title(&self) -> String {
        let name = non_empty(self.title.as_deref()).map(str::to_owned).unwrap_or_else(|| file_stem(&self.path));
        match non_empty(self.artist.as_deref()) {
            Some(artist) => format!("{artist} - {name}"),
            None => name,
        }
    }
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_title_falls_back_to_file_name() {
        assert_eq!(Track::new("/music/Moon Song.flac").display_title(), "Moon Song");
        let tagged = Track::with_tags("/m/x.mp3", Some(" Rise "), Some("Coven"), None);
        assert_eq!(tagged.display_title(), "Coven - Rise");
        let blank = Track::with_tags("/m/x.mp3", Some("  "), Some(""), None);
        assert_eq!(blank.display_title(), "x");
    }
}

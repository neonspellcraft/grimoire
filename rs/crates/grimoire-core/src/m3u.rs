use std::path::{Component, Path, PathBuf};

use crate::Track;

/// Returns the file paths listed in an m3u/m3u8 playlist. Relative paths resolve against `base` (the playlist's folder).
pub fn parse(text: &str, base: &Path) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            if line.starts_with("file://") {
                return url::Url::parse(line).ok()?.to_file_path().ok();
            }
            let path = Path::new(line);
            if line.starts_with('/') || line.starts_with('\\') || path.is_absolute() {
                return Some(path.to_path_buf());
            }
            Some(normalize(&base.join(path)))
        })
        .collect()
}

pub fn serialize(tracks: &[Track]) -> String {
    let mut lines = vec!["#EXTM3U".to_string()];
    for track in tracks {
        let seconds = track.duration.map_or(-1, |d| d.round() as i64);
        lines.push(format!("#EXTINF:{seconds},{}", track.display_title()));
        lines.push(track.path.to_string_lossy().into_owned());
    }
    lines.join("\n") + "\n"
}

/// Resolves `.` and `..` without touching the file system (like `URL.standardizedFileURL`).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let tracks = [
            Track::with_tags("/music/a song.flac", Some("Song"), Some("Band"), Some(61.4)),
            Track::new("/music/b.wav"),
        ];
        let text = serialize(&tracks);
        assert!(text.contains("#EXTINF:61,Band - Song"));
        let paths = parse(&text, Path::new("/tmp/"));
        assert_eq!(paths, [PathBuf::from("/music/a song.flac"), PathBuf::from("/music/b.wav")]);
    }

    #[test]
    fn relative_paths_resolve_against_playlist_folder() {
        let paths = parse("#EXTM3U\nsub/x.mp3\n\n../y.aiff\n", Path::new("/music/lists/"));
        assert_eq!(paths, [PathBuf::from("/music/lists/sub/x.mp3"), PathBuf::from("/music/y.aiff")]);
    }

    #[test]
    fn handles_crlf_and_file_urls() {
        let text = "#EXTM3U\r\na.mp3\r\n";
        assert_eq!(parse(text, Path::new("/m")), [PathBuf::from("/m/a.mp3")]);
        #[cfg(unix)]
        assert_eq!(parse("file:///music/a%20b.mp3", Path::new("/")), [PathBuf::from("/music/a b.mp3")]);
    }
}

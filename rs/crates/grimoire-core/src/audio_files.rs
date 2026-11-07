use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const EXTENSIONS: &[&str] = &["mp3", "wav", "wave", "aif", "aiff", "aifc", "flac"];
pub const PLAYLIST_EXTENSIONS: &[&str] = &["m3u", "m3u8"];

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn is_supported(path: &Path) -> bool {
    EXTENSIONS.contains(&extension(path).as_str())
}

pub fn is_playlist(path: &Path) -> bool {
    PLAYLIST_EXTENSIONS.contains(&extension(path).as_str())
}

/// Expands folders recursively into supported audio files (sorted Finder-style); plain files pass through if supported.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            walk(path, &mut found);
            found.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
            result.extend(found);
        } else if path.is_file() && is_supported(path) {
            result.push(path.clone());
        }
    }
    result
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue; // hidden, like Finder's .skipsHiddenFiles
        }
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if is_supported(&path) {
            found.push(path);
        }
    }
}

/// Case-insensitive comparison where runs of digits compare by value ("2" < "10"),
/// close to Foundation's `localizedStandardCompare`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let order = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_extensions_case_insensitively() {
        assert!(is_supported(Path::new("a/B.FLAC")));
        assert!(is_supported(Path::new("x.aifc")));
        assert!(!is_supported(Path::new("x.ogg")));
        assert!(is_playlist(Path::new("list.M3U8")));
    }

    #[test]
    fn natural_order_matches_finder() {
        let mut names = vec!["Track 10.mp3", "track 2.mp3", "Track 1.mp3", "b.mp3"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["b.mp3", "Track 1.mp3", "track 2.mp3", "Track 10.mp3"]);
    }
}

//! What Grimoire remembers between launches, as JSON in the platform's config folder
//! (`~/Library/Application Support/Grimoire` on macOS, `%APPDATA%\Grimoire` on Windows).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub shuffle: bool,
    /// "off", "all" or "one".
    pub repeat: String,
    pub playlist: Vec<PathBuf>,
    pub current: Option<usize>,
    /// Seconds into the current track when Grimoire quit.
    pub position: f64,
    pub show_remaining: bool,
    /// Logical size and position of the main window.
    pub main_size: Option<(f64, f64)>,
    pub main_position: Option<(i32, i32)>,
    pub visualizer_open: bool,
    pub visualizer_size: Option<(f64, f64)>,
    pub visualizer_position: Option<(i32, i32)>,
    pub scene: String,
    pub show_meters: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 0.8,
            shuffle: false,
            repeat: "off".into(),
            playlist: Vec::new(),
            current: None,
            position: 0.0,
            show_remaining: false,
            main_size: None,
            main_position: None,
            visualizer_open: false,
            visualizer_size: None,
            visualizer_position: None,
            scene: "threeBody".into(),
            show_meters: false,
        }
    }
}

impl Settings {
    /// `GRIMOIRE_SETTINGS` overrides the location (for testing without touching the real settings).
    pub fn path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("GRIMOIRE_SETTINGS") {
            return Some(path.into());
        }
        directories::ProjectDirs::from("", "", "Grimoire").map(|dirs| dirs.config_dir().join("settings.json"))
    }

    /// The saved settings, or defaults if there are none (or they can't be read). On the very first launch,
    /// picks up the playlist the Swift app left behind (`last-session.m3u` in the same folder).
    pub fn load() -> Settings {
        let Some(path) = Self::path() else { return Settings::default() };
        if let Ok(text) = std::fs::read_to_string(&path) {
            return serde_json::from_str(&text).unwrap_or_default();
        }
        let mut settings = Settings::default();
        let swift_session = path.with_file_name("last-session.m3u");
        if let Ok(text) = std::fs::read_to_string(&swift_session) {
            settings.playlist = grimoire_core::m3u::parse(&text, swift_session.parent().unwrap_or(&swift_session));
        }
        settings
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| {
                let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
                // Write then rename, so a crash mid-save can't leave a truncated file.
                let temporary = path.with_extension("json.tmp");
                std::fs::write(&temporary, text)?;
                std::fs::rename(&temporary, &path)
            });
        if let Err(e) = result {
            eprintln!("grimoire: couldn't save settings to {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_fields() {
        let settings = Settings {
            volume: 0.3,
            playlist: vec!["/music/a.flac".into()],
            current: Some(0),
            main_size: Some((520.0, 420.0)),
            ..Default::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
        let old: Settings = serde_json::from_str(r#"{"volume": 0.5}"#).unwrap();
        assert_eq!(old.volume, 0.5);
        assert_eq!(old.scene, "threeBody");
    }
}

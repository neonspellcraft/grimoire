//! Media keys and the system's Now Playing (port of `NowPlaying.swift`): macOS Control Center and the Windows
//! media overlay show the track and its position, and their buttons, plus the keyboard's media keys, drive the player.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use grimoire_audio::PlayState;
use grimoire_core::TrackId;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use winit::window::Window;

use crate::library::Library;

pub struct NowPlaying {
    controls: MediaControls,
    events: Receiver<MediaControlEvent>,
    shown_track: Option<TrackId>,
    shown_state: Option<PlayState>,
    /// Position last sent and when, so seeks (which don't change the state) are noticed as jumps.
    sent_position: f64,
    sent_at: Instant,
}

impl NowPlaying {
    /// None if the system media controls aren't available (they're optional; the app works without them).
    pub fn new(window: &Window) -> Option<NowPlaying> {
        let config = PlatformConfig { display_name: "Grimoire", dbus_name: "grimoire", hwnd: hwnd(window) };
        let mut controls = match MediaControls::new(config) {
            Ok(controls) => controls,
            Err(e) => {
                eprintln!("grimoire: no media key support: {e:?}");
                return None;
            }
        };
        let (sender, events) = mpsc::channel();
        // The OS may call this from its own thread; hand events to the UI thread.
        if let Err(e) = controls.attach(move |event| {
            let _ = sender.send(event);
        }) {
            eprintln!("grimoire: no media key support: {e:?}");
            return None;
        }
        Some(NowPlaying { controls, events, shown_track: None, shown_state: None, sent_position: 0.0, sent_at: Instant::now() })
    }

    /// Call every frame: applies media-key presses and keeps the system's Now Playing in sync.
    pub fn tick(&mut self, library: &mut Library) {
        while let Ok(event) = self.events.try_recv() {
            apply(event, library);
        }

        let track = library.playlist.current().cloned();
        let track_id = track.as_ref().map(|t| t.id);
        if track_id != self.shown_track {
            self.shown_track = track_id;
            let title = track.as_ref().map(|t| {
                t.title.clone().unwrap_or_else(|| {
                    t.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                })
            });
            let artist = track.as_ref().and_then(|t| t.artist.clone());
            let _ = self.controls.set_metadata(MediaMetadata {
                title: title.as_deref(),
                artist: artist.as_deref(),
                album: None,
                cover_url: None,
                duration: library.duration().map(Duration::from_secs_f64),
            });
            self.shown_state = None; // re-send the position for the new track
        }

        // The system extrapolates the position while playing, so it only needs sending on a change of state or a
        // jump (a seek from the window, the keyboard or the system itself).
        let state = library.state();
        let elapsed = library.elapsed();
        let expected = self.sent_position
            + if self.shown_state == Some(PlayState::Playing) { self.sent_at.elapsed().as_secs_f64() } else { 0.0 };
        if Some(state) != self.shown_state || (elapsed - expected).abs() > 1.0 {
            self.shown_state = Some(state);
            self.sent_position = elapsed;
            self.sent_at = Instant::now();
            let progress = Some(MediaPosition(Duration::from_secs_f64(elapsed)));
            let playback = match state {
                PlayState::Playing => MediaPlayback::Playing { progress },
                PlayState::Paused => MediaPlayback::Paused { progress },
                PlayState::Stopped => MediaPlayback::Stopped,
            };
            let _ = self.controls.set_playback(playback);
        }
    }
}

fn apply(event: MediaControlEvent, library: &mut Library) {
    let seek = |library: &mut Library, seconds: f64| {
        let target = (library.elapsed() + seconds).max(0.0);
        library.seek(target);
    };
    match event {
        MediaControlEvent::Play if library.state() != PlayState::Playing => library.toggle(),
        MediaControlEvent::Pause if library.state() == PlayState::Playing => library.toggle(),
        MediaControlEvent::Toggle => library.toggle(),
        MediaControlEvent::Next => library.next(),
        MediaControlEvent::Previous => library.previous(),
        MediaControlEvent::Stop => {
            if let Some(player) = &mut library.player {
                player.stop();
            }
        }
        MediaControlEvent::Seek(SeekDirection::Forward) => seek(library, 10.0),
        MediaControlEvent::Seek(SeekDirection::Backward) => seek(library, -10.0),
        MediaControlEvent::SeekBy(SeekDirection::Forward, by) => seek(library, by.as_secs_f64()),
        MediaControlEvent::SeekBy(SeekDirection::Backward, by) => seek(library, -by.as_secs_f64()),
        MediaControlEvent::SetPosition(MediaPosition(at)) => library.seek(at.as_secs_f64()),
        MediaControlEvent::SetVolume(volume) => library.set_volume(volume as f32),
        _ => {}
    }
}

#[cfg(target_os = "windows")]
fn hwnd(window: &Window) -> Option<*mut std::ffi::c_void> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn hwnd(_: &Window) -> Option<*mut std::ffi::c_void> {
    None
}

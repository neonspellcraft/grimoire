//! Milestone-1 app: a playlist from the command line (or dropped files) playing through the new audio engine,
//! with the visualizer window. The full main window (transport, playlist, spectrum) comes in a later milestone.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use grimoire_audio::{PlayState, Player, PlayerEvent, TapReader};
use grimoire_core::{audio_files, m3u, time_math, BandFollower, MultibandAnalyzer, Playlist, Track};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::gpu::{surface_formats, Gpu};
use crate::scenes::{SceneAudio, SceneKind, WindowScene};

pub const HELP: &str = "\
Keys:  Space play/pause   ←/→ seek 5 s (Shift: 30 s)   N/P next/previous track
       ↑/↓ volume   R new orbit   M band meters   F or double-click full screen   Q quit
Drag the window to move it; drop audio files or folders on it to add them.";

pub fn run(paths: Vec<PathBuf>) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App { initial: paths, running: None, error: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    initial: Vec<PathBuf>,
    running: Option<Running>,
    error: Option<String>,
}

struct Running {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    view_format: wgpu::TextureFormat,
    gpu: Gpu,
    kind: SceneKind,
    scene: Box<dyn WindowScene>,
    show_meters: bool,

    player: Option<Player>,
    playlist: Playlist,
    reader: TapReader,
    analyzer: MultibandAnalyzer,
    follower: BandFollower,

    last_frame: Option<Instant>,
    occluded: bool,
    modifiers: ModifiersState,
    last_click: Option<Instant>,
    last_title: Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match Running::new(event_loop, std::mem::take(&mut self.initial)) {
            Ok(running) => self.running = Some(running),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(app) = &mut self.running else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => app.resize(size.width, size.height),
            WindowEvent::Occluded(occluded) => {
                app.occluded = occluded;
                app.last_frame = None;
            }
            WindowEvent::RedrawRequested => app.frame(),
            WindowEvent::ModifiersChanged(modifiers) => app.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                if app.key(&event) {
                    event_loop.exit();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let now = Instant::now();
                if app.last_click.is_some_and(|t| now - t < Duration::from_millis(400)) {
                    app.toggle_fullscreen();
                    app.last_click = None;
                } else {
                    app.last_click = Some(now);
                    let _ = app.window.drag_window();
                }
            }
            WindowEvent::DroppedFile(path) => app.add(vec![path]),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(app) = &mut self.running else { return };
        app.tick();
        let hidden = app.occluded || app.config.width == 0 || app.config.height == 0;
        if hidden {
            // Nothing to draw, but keep the playlist moving when a track ends.
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100)));
        } else {
            event_loop.set_control_flow(ControlFlow::Poll);
            app.window.request_redraw();
        }
    }
}

impl Running {
    fn new(event_loop: &ActiveEventLoop, paths: Vec<PathBuf>) -> Result<Running, String> {
        let attributes = Window::default_attributes()
            .with_title("Grimoire Visualizer")
            .with_inner_size(LogicalSize::new(640.0, 400.0))
            .with_min_inner_size(LogicalSize::new(240.0, 150.0));
        let window = Arc::new(event_loop.create_window(attributes).map_err(|e| e.to_string())?);
        let instance = Gpu::instance();
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let gpu = Gpu::new(&instance, Some(&surface))?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let (format, view_format) = surface_formats(&capabilities);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: if view_format != format { vec![view_format] } else { vec![] },
        };
        surface.configure(&gpu.device, &config);
        let kind = SceneKind::ThreeBody;
        let scene = kind.make(&gpu.device, view_format)?;

        let player = match Player::new() {
            Ok(player) => Some(player),
            Err(e) => {
                eprintln!("grimoire: playing without sound: {e}");
                None
            }
        };
        let mut running = Running {
            window,
            surface,
            config,
            view_format,
            gpu,
            kind,
            scene,
            show_meters: false,
            player,
            playlist: Playlist::default(),
            reader: TapReader::new(2048),
            analyzer: MultibandAnalyzer::default(),
            follower: BandFollower::default(),
            last_frame: None,
            occluded: false,
            modifiers: ModifiersState::empty(),
            last_click: None,
            last_title: Instant::now(),
        };
        running.add(paths);
        Ok(running)
    }

    /// Adds files, folders and m3u playlists. Starts playing if nothing was playing.
    fn add(&mut self, paths: Vec<PathBuf>) {
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
        let tracks: Vec<Track> = audio_files::expand(&expanded).into_iter().map(Track::new).collect();
        if tracks.is_empty() {
            return;
        }
        let idle = self.player.as_ref().is_none_or(|p| p.state() == PlayState::Stopped);
        let first = tracks[0].id;
        self.playlist.append(tracks);
        if idle {
            self.playlist.select(Some(first));
            self.play_current();
        }
    }

    /// Loads the playlist's current track, skipping files that won't open.
    fn play_current(&mut self) {
        let Some(player) = &mut self.player else { return };
        for _ in 0..self.playlist.tracks().len() {
            let Some(track) = self.playlist.current().cloned() else { return };
            match player.load(&track.path, true) {
                Ok(info) => {
                    self.playlist.update(track.id, |t| {
                        t.title = info.title.clone().or(t.title.take());
                        t.artist = info.artist.clone().or(t.artist.take());
                        t.duration = info.duration.or(t.duration);
                    });
                    self.update_title();
                    return;
                }
                Err(e) => {
                    eprintln!("grimoire: {e}");
                    if self.playlist.advance(false).is_none() {
                        return;
                    }
                }
            }
        }
    }

    fn tick(&mut self) {
        let event = self.player.as_mut().and_then(|p| p.tick());
        if event == Some(PlayerEvent::Finished) && self.playlist.advance(true).is_some() {
            self.play_current();
        }
        if self.last_title.elapsed() > Duration::from_millis(500) {
            self.update_title();
        }
    }

    fn update_title(&mut self) {
        self.last_title = Instant::now();
        let mut title = format!("Grimoire — {}", self.kind.title());
        if let (Some(track), Some(player)) = (self.playlist.current(), &self.player) {
            let total = player.duration().map(time_math::clock).unwrap_or_default();
            let paused = if player.state() == PlayState::Playing { "" } else { " (paused)" };
            title = format!("{} — {} / {total}{paused}", track.display_title(), time_math::clock(player.current_time()));
        }
        self.window.set_title(&title);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        if width > 0 && height > 0 {
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = self.last_frame.map_or(1.0 / 60.0, |t| (now - t).as_secs_f64().clamp(0.0, 0.1));
        self.last_frame = Some(now);

        let raw = match &self.player {
            Some(player) if self.reader.read(player.tap(), dt, player.state() == PlayState::Playing) => {
                Some(self.analyzer.levels(&self.reader.samples, self.reader.sample_rate))
            }
            _ => None,
        };
        self.follower.step(raw, dt);
        self.scene.update(SceneAudio { levels: self.follower.envelope, hits: self.follower.hits }, dt);

        if self.config.width == 0 || self.config.height == 0 {
            return;
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.gpu.device, &self.config);
                return;
            }
            Err(e) => {
                eprintln!("grimoire: skipped a frame: {e}");
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        let size = (self.config.width, self.config.height);
        self.scene.encode(&self.gpu.device, &self.gpu.queue, &mut encoder, &view, size, self.show_meters);
        self.gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        frame.present();
    }

    fn toggle_fullscreen(&self) {
        let next = if self.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
        self.window.set_fullscreen(next);
    }

    /// Returns true to quit.
    fn key(&mut self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let big = self.modifiers.shift_key();
        match &event.logical_key {
            Key::Named(NamedKey::Space) => {
                if let Some(player) = &mut self.player {
                    player.toggle();
                }
            }
            Key::Named(NamedKey::ArrowRight) => self.seek_by(if big { 30.0 } else { 5.0 }),
            Key::Named(NamedKey::ArrowLeft) => self.seek_by(if big { -30.0 } else { -5.0 }),
            Key::Named(NamedKey::ArrowUp) => self.volume_by(0.1),
            Key::Named(NamedKey::ArrowDown) => self.volume_by(-0.1),
            Key::Named(NamedKey::Escape) if self.window.fullscreen().is_some() => self.window.set_fullscreen(None),
            Key::Character(c) => match c.to_lowercase().as_str() {
                "q" => return true,
                "f" => self.toggle_fullscreen(),
                "m" => self.show_meters = !self.show_meters,
                "r" => self.scene.reseed(),
                "n" => {
                    if self.playlist.advance(false).is_some() {
                        self.play_current();
                    }
                }
                "p" => {
                    // Like most players: back to the start of the track first, previous track if near the start.
                    let near_start = self.player.as_ref().is_none_or(|p| p.current_time() < 3.0);
                    if near_start && self.playlist.retreat().is_some() {
                        self.play_current();
                    } else if let Some(player) = &mut self.player {
                        player.seek(0.0);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.update_title();
        false
    }

    fn seek_by(&mut self, delta: f64) {
        if let Some(player) = &mut self.player {
            let target = player.current_time() + delta;
            player.seek(target);
        }
    }

    fn volume_by(&mut self, delta: f32) {
        if let Some(player) = &self.player {
            player.set_volume(player.volume() + delta);
        }
    }
}

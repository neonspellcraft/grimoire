//! The visualizer window (port of `SceneHostView`): one full-window scene driven by the shared audio feed.
//! Drag to move, double-click for full screen; 1–4 or Tab switch scenes, R reseed, M band meters, F full screen,
//! Space play/pause.

use std::time::{Duration, Instant};

use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::Fullscreen;

use crate::feed::AudioFeed;
use crate::gpu::{Gpu, WindowSurface};
use crate::library::Library;
use crate::scenes::{SceneKind, WindowScene};

pub struct VisWindow {
    pub surface: WindowSurface,
    pub kind: SceneKind,
    scene: Box<dyn WindowScene>,
    pub show_meters: bool,
    last_frame: Option<Instant>,
    last_click: Option<Instant>,
    pub occluded: bool,
    /// Off by default; then only the mouse (double-click for full screen) and Esc work here.
    pub shortcuts: bool,
}

/// Whether the visualizer window wants to close.
pub struct Close;

impl VisWindow {
    pub fn new(surface: WindowSurface, gpu: &Gpu, kind: SceneKind, show_meters: bool) -> Result<VisWindow, String> {
        // Fall back to Three-Body if the saved scene can't be built (e.g. one from a newer build).
        let (kind, scene) = match kind.make(&gpu.device, surface.view_format) {
            Ok(scene) => (kind, scene),
            Err(_) => (SceneKind::ThreeBody, SceneKind::ThreeBody.make(&gpu.device, surface.view_format)?),
        };
        surface.window.set_title(&format!("Grimoire Visualizer — {}", kind.title()));
        Ok(VisWindow { surface, kind, scene, show_meters, last_frame: None, last_click: None, occluded: false, shortcuts: false })
    }

    /// Swaps in a fresh instance of `kind`; the old scene and its GPU resources are released.
    pub fn select(&mut self, gpu: &Gpu, kind: SceneKind) {
        if kind == self.kind {
            return;
        }
        match kind.make(&gpu.device, self.surface.view_format) {
            Ok(scene) => {
                self.scene = scene;
                self.kind = kind;
                self.last_frame = None;
                self.surface.window.set_title(&format!("Grimoire Visualizer — {}", kind.title()));
            }
            Err(e) => eprintln!("grimoire: couldn't build the {} scene: {e}", kind.title()),
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, feed: &AudioFeed) {
        let now = Instant::now();
        let dt = self.last_frame.map_or(1.0 / 60.0, |t| (now - t).as_secs_f64().clamp(0.0, 0.1));
        self.last_frame = Some(now);
        self.scene.update(feed.scene_audio(), dt);
        let Some((frame, view)) = self.surface.acquire(gpu) else { return };
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        self.scene.encode(&gpu.device, &gpu.queue, &mut encoder, &view, self.surface.size(), self.show_meters);
        gpu.queue.submit([encoder.finish()]);
        self.surface.present(frame);
    }

    pub fn event(&mut self, event: &WindowEvent, gpu: &Gpu, library: &mut Library) -> Option<Close> {
        match event {
            WindowEvent::CloseRequested => return Some(Close),
            WindowEvent::Resized(size) => self.surface.resize(gpu, size.width, size.height),
            WindowEvent::Occluded(occluded) => {
                self.occluded = *occluded;
                self.last_frame = None;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let now = Instant::now();
                if self.last_click.is_some_and(|t| now - t < Duration::from_millis(400)) {
                    self.toggle_fullscreen();
                    self.last_click = None;
                } else {
                    self.last_click = Some(now);
                    let _ = self.surface.window.drag_window();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(event, gpu, library),
            WindowEvent::DroppedFile(path) => library.add(vec![path.clone()], true),
            _ => {}
        }
        None
    }

    fn key(&mut self, event: &KeyEvent, gpu: &Gpu, library: &mut Library) {
        if event.state != ElementState::Pressed {
            return;
        }
        if !self.shortcuts {
            if event.logical_key == Key::Named(NamedKey::Escape) && self.surface.window.fullscreen().is_some() {
                self.surface.window.set_fullscreen(None);
            }
            return;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Space) => library.toggle(),
            Key::Named(NamedKey::Tab) => {
                let index = SceneKind::ALL.iter().position(|&k| k == self.kind).unwrap_or(0);
                self.select(gpu, SceneKind::ALL[(index + 1) % SceneKind::ALL.len()]);
            }
            Key::Named(NamedKey::Escape) if self.surface.window.fullscreen().is_some() => {
                self.surface.window.set_fullscreen(None)
            }
            Key::Character(c) => match c.to_lowercase().as_str() {
                "f" => self.toggle_fullscreen(),
                "m" => self.show_meters = !self.show_meters,
                "r" => self.scene.reseed(),
                digit @ ("1" | "2" | "3" | "4") => {
                    let index = digit.parse::<usize>().unwrap() - 1;
                    self.select(gpu, SceneKind::ALL[index]);
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn reseed(&mut self) {
        self.scene.reseed();
    }

    fn toggle_fullscreen(&self) {
        let window = &self.surface.window;
        window.set_fullscreen(if window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
    }
}

//! The visualizer window (port of `SceneHostView`): one full-window scene driven by the shared audio feed.
//! Drag to move, double-click for full screen; 1–7 or Tab switch scenes, R reseed, M band meters, F full screen,
//! Space play/pause. While Doom is showing, every key goes to the game instead.

use std::time::{Duration, Instant};

use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use winit::window::Fullscreen;

use crate::controller::PadInput;
use crate::feed::AudioFeed;
use crate::gpu::{Gpu, WindowSurface};
use crate::library::Library;
use crate::menus::{popup, Entry};
use crate::scenes::{SceneKind, WindowScene};

pub struct VisWindow {
    pub surface: WindowSurface,
    pub kind: SceneKind,
    scene: Box<dyn WindowScene>,
    pub show_meters: bool,
    last_frame: Option<Instant>,
    last_click: Option<Instant>,
    pub occluded: bool,
    /// On by default; turned off, only the mouse (double-click for full screen) and Esc work here.
    pub shortcuts: bool,
    control_down: bool,
    focused: bool,
    /// The scene to go back to when a game (Doom) quits.
    previous: SceneKind,
    /// Doom's sound effects (Grimoire's setting; off by default).
    game_sound: bool,
    doom_controls: crate::doom_controls::DoomControls,
}

/// Whether the visualizer window wants to close.
pub struct Close;

impl VisWindow {
    pub fn new(
        surface: WindowSurface,
        gpu: &Gpu,
        kind: SceneKind,
        show_meters: bool,
        game_sound: bool,
        doom_controls: crate::doom_controls::DoomControls,
    ) -> Result<VisWindow, String> {
        // Fall back to Three-Body if the saved scene can't be built (e.g. one from a newer build).
        let (kind, mut scene) = match kind.make(&gpu.device, surface.view_format) {
            Ok(scene) => (kind, scene),
            Err(e) => {
                eprintln!("grimoire: couldn't build the {} scene: {e}", kind.title());
                (SceneKind::ThreeBody, SceneKind::ThreeBody.make(&gpu.device, surface.view_format)?)
            }
        };
        scene.set_sound(game_sound);
        scene.set_doom_controls(&doom_controls);
        surface.window.set_title(&format!("Grimoire Visualizer — {}", kind.title()));
        Ok(VisWindow {
            surface,
            kind,
            scene,
            show_meters,
            last_frame: None,
            last_click: None,
            occluded: false,
            shortcuts: true,
            control_down: false,
            focused: true,
            previous: SceneKind::ThreeBody,
            game_sound,
            doom_controls,
        })
    }

    /// Games pause unless the window is showing and focused (so keys typed elsewhere don't play them).
    fn update_visibility(&mut self) {
        self.scene.set_visible(self.focused && !self.occluded);
    }

    pub fn set_doom_controls(&mut self, controls: &crate::doom_controls::DoomControls) {
        self.doom_controls = controls.clone();
        self.scene.set_doom_controls(controls);
    }

    pub fn set_game_sound(&mut self, on: bool) {
        self.game_sound = on;
        self.scene.set_sound(on);
    }

    pub fn pad(&mut self, input: PadInput) {
        self.scene.pad(input);
    }

    /// Swaps in a fresh instance of `kind`; the old scene and its GPU resources are released.
    pub fn select(&mut self, gpu: &Gpu, kind: SceneKind) {
        if kind == self.kind {
            return;
        }
        match kind.make(&gpu.device, self.surface.view_format) {
            Ok(scene) => {
                if !self.kind.is_game() {
                    self.previous = self.kind;
                }
                self.scene = scene;
                self.kind = kind;
                self.last_frame = None;
                self.scene.set_sound(self.game_sound);
                self.scene.set_doom_controls(&self.doom_controls);
                self.update_visibility();
                self.surface.window.set_title(&format!("Grimoire Visualizer — {}", kind.title()));
            }
            Err(e) => eprintln!("grimoire: couldn't build the {} scene: {e}", kind.title()),
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, feed: &AudioFeed) {
        if self.scene.wants_to_leave() {
            self.select(gpu, self.previous);
        }
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
                self.update_visibility();
            }
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                self.update_visibility();
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => self.context_menu(gpu),
            WindowEvent::ModifiersChanged(modifiers) => self.control_down = modifiers.state().control_key(),
            // Control-click is a right-click on macOS.
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }
                if cfg!(target_os = "macos") && self.control_down =>
            {
                self.context_menu(gpu)
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
        // A game gets every key, presses and releases; leave it with the right-click menu.
        if self.kind.is_game() && self.scene.key(event) {
            return;
        }
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
                digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7") => {
                    let index = digit.parse::<usize>().unwrap() - 1;
                    self.select(gpu, SceneKind::ALL[index]);
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Right-click: two labeled sections, so choosing a scene isn't confused with the current scene's own actions.
    fn context_menu(&mut self, gpu: &Gpu) {
        enum Command {
            Scene(SceneKind),
            Reseed,
            Meters,
            FullScreen,
        }
        let mut entries = vec![Entry::Header("Scene".into())];
        for kind in SceneKind::ALL {
            entries.push(Entry::Check(kind.title().into(), kind == self.kind, Command::Scene(kind)));
        }
        entries.extend([Entry::Separator, Entry::Header(self.kind.title().into())]);
        if !self.kind.is_game() {
            entries.extend([
                Entry::Item(self.kind.reseed_title().into(), Command::Reseed),
                Entry::Check("Show Band Meters".into(), self.show_meters, Command::Meters),
                Entry::Separator,
            ]);
        }
        entries.push(Entry::Item("Toggle Full Screen".into(), Command::FullScreen));
        match popup(&self.surface.window, entries) {
            Some(Command::Scene(kind)) => self.select(gpu, kind),
            Some(Command::Reseed) => self.scene.reseed(),
            Some(Command::Meters) => self.show_meters = !self.show_meters,
            Some(Command::FullScreen) => self.toggle_fullscreen(),
            None => {}
        }
        self.last_click = None;
    }

    pub fn reseed(&mut self) {
        self.scene.reseed();
    }

    fn toggle_fullscreen(&self) {
        let window = &self.surface.window;
        window.set_fullscreen(if window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
    }
}

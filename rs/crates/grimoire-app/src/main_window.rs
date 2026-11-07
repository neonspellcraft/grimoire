//! The terminal-grimoire main window: builds a `View` from the library each frame, lays it out on the cell
//! grid, draws it with the CRT finish, and turns clicks, drags, scrolls and typing into player actions.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use grimoire_audio::PlayState;
use grimoire_core::TrackId;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Window};

use crate::feed::AudioFeed;
use crate::gpu::{Gpu, WindowSurface};
use crate::library::{format_line, matches, Library};
use crate::scenes::SceneKind;
use crate::ui::layout::{self, list_rows, MIN_COLS, MIN_ROWS};
use crate::ui::screen::CellRect;
use crate::ui::{Action, Effects, RowView, Screen, SettingsView, UiRenderer, View};

/// What the main window asks of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Quit,
    ToggleVisualizer,
    SetShortcuts(bool),
    SetScene(SceneKind),
    SetMeters(bool),
    Reseed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Seek(CellRect),
    Volume(CellRect),
    /// Reordering the playlist: the track being carried.
    Row(TrackId),
}

pub struct MainWindow {
    pub surface: WindowSurface,
    ui: UiRenderer,
    screen: Screen<Action>,
    started: Instant,

    cursor: PhysicalPosition<f64>,
    modifiers: ModifiersState,
    drag: Option<Drag>,
    /// Where the pointer is while scrubbing the seek bar (the seek happens on release).
    scrub: Option<f64>,
    last_click: Option<(Instant, Action)>,

    prompt: String,
    prompt_focused: bool,
    pub show_remaining: bool,
    selected: Option<TrackId>,
    scroll: usize,
    /// The rows on screen, top to bottom, so a click on row i finds its track.
    visible: Vec<TrackId>,
    /// Every track matching the prompt, in order, and how many rows fit.
    filtered: Vec<TrackId>,
    capacity: usize,
    revealing: Option<TrackId>,
    reveal_start: Instant,
    pub visualizer_open: bool,
    occluded: bool,
    /// Off by default: then only the mouse, the focused prompt, ⌘/Ctrl+Q and Esc (to leave full screen) act.
    pub shortcuts: bool,
    settings_open: bool,
    /// Mirrors of the visualizer's state, for the settings panel.
    pub scene: SceneKind,
    pub show_meters: bool,
}

impl MainWindow {
    pub fn new(surface: WindowSurface, gpu: &Gpu) -> MainWindow {
        let scale = surface.window.scale_factor() as f32;
        let ui = UiRenderer::new(&gpu.device, surface.view_format, scale);
        let window = MainWindow {
            surface,
            ui,
            screen: Screen::new(1, 1),
            started: Instant::now(),
            cursor: PhysicalPosition::new(-1.0, -1.0),
            modifiers: ModifiersState::empty(),
            drag: None,
            scrub: None,
            last_click: None,
            prompt: String::new(),
            prompt_focused: false,
            show_remaining: false,
            selected: None,
            scroll: 0,
            visible: Vec::new(),
            filtered: Vec::new(),
            capacity: 0,
            revealing: None,
            reveal_start: Instant::now(),
            visualizer_open: false,
            occluded: false,
            shortcuts: false,
            settings_open: false,
            scene: SceneKind::ThreeBody,
            show_meters: false,
        };
        window.apply_min_size();
        window
    }

    pub fn is_visible(&self) -> bool {
        self.surface.is_drawable() && !self.occluded && !self.surface.window.is_minimized().unwrap_or(false)
    }

    /// Whether the next frames would look different without any input: playing music, the glitch reveal,
    /// a drag, the prompt's cursor, or the spectrum and glow still settling after a pause.
    pub fn is_animating(&self, library: &Library, feed: &AudioFeed) -> bool {
        library.state() == PlayState::Playing
            || self.reveal_start.elapsed().as_secs_f32() < 0.5
            || self.drag.is_some()
            || self.prompt_focused
            || !feed.bars.is_settled()
            || feed.follower.envelope.loudest() > 0.01
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.surface.window
    }

    /// The window can't shrink below the smallest grid the layout supports.
    fn apply_min_size(&self) {
        let (cw, ch) = self.ui.cell_size();
        let scale = self.surface.window.scale_factor();
        let size = LogicalSize::new(
            (MIN_COLS as u32 * cw) as f64 / scale,
            ((MIN_ROWS + 2) as u32 * ch) as f64 / scale,
        );
        self.surface.window.set_min_inner_size(Some(size));
    }

    // MARK: Drawing

    pub fn draw(&mut self, gpu: &Gpu, library: &Library, feed: &AudioFeed) {
        if !self.surface.is_drawable() {
            return;
        }
        let (width, height) = self.surface.size();
        let (cols, rows) = self.ui.grid_size(width, height);
        let view = self.view(library, feed, rows);
        self.screen = Screen::new(cols, rows);
        layout::layout(&view, &mut self.screen);

        let Some((frame, target)) = self.surface.acquire(gpu) else { return };
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let effects = Effects { time: view.clock as f32, glow: view.beat };
        self.ui.render(&gpu.device, &gpu.queue, &mut encoder, &target, (width, height), &self.screen, effects);
        gpu.queue.submit([encoder.finish()]);
        self.surface.present(frame);
    }

    fn view(&mut self, library: &Library, feed: &AudioFeed, rows: u16) -> View {
        let clock = self.started.elapsed().as_secs_f64();
        let current = library.playlist.current();
        let current_id = current.map(|t| t.id);
        if current_id != self.revealing {
            self.revealing = current_id;
            self.reveal_start = Instant::now();
        }
        let reveal = (self.reveal_start.elapsed().as_secs_f32() / 0.45).min(1.0);

        // Filtered playlist, scrolled so the selection stays in view.
        let filtered: Vec<usize> = library
            .playlist
            .tracks()
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                self.prompt.is_empty()
                    || matches(&self.prompt, &t.display_title())
                    || matches(&self.prompt, &t.path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default())
            })
            .map(|(i, _)| i)
            .collect();
        let capacity = list_rows(rows);
        self.scroll = self.scroll.min(filtered.len().saturating_sub(capacity));
        let tracks = library.playlist.tracks();
        self.capacity = capacity;
        self.filtered = filtered.iter().map(|&i| tracks[i].id).collect();
        self.visible = filtered.iter().skip(self.scroll).take(capacity).map(|&i| tracks[i].id).collect();
        let rows_view = filtered
            .iter()
            .skip(self.scroll)
            .take(capacity)
            .map(|&i| {
                let track = &tracks[i];
                RowView {
                    number: i + 1,
                    title: track.display_title().to_lowercase(),
                    duration: track.duration,
                    current: Some(track.id) == current_id,
                    selected: Some(track.id) == self.selected,
                }
            })
            .collect();

        let duration = library.duration();
        let elapsed = match (self.scrub, duration) {
            (Some(fraction), Some(d)) => fraction * d,
            _ => library.elapsed(),
        };
        let playing = library.state() == PlayState::Playing;
        View {
            clock,
            beat: if playing { feed.follower.envelope.low } else { 0.0 },
            state: library.state(),
            file_name: current
                .and_then(|t| t.path.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
            byline: current.and_then(|t| t.artist.clone()).unwrap_or_default().to_lowercase(),
            format: library.info.as_ref().map(format_line).unwrap_or_default(),
            elapsed,
            duration,
            show_remaining: self.show_remaining,
            volume: library.volume(),
            shuffle: library.playlist.shuffle(),
            repeat: library.playlist.repeat_mode,
            visualizer: self.visualizer_open,
            prompt: self.prompt.clone(),
            prompt_focused: self.prompt_focused,
            spectrum: feed.bars.levels.clone(),
            rows: rows_view,
            track_count: library.playlist.tracks().len(),
            total_duration: library.playlist.total_duration(),
            reveal,
            settings: self.settings_open.then(|| SettingsView {
                shortcuts: self.shortcuts,
                scene: self.scene.title().into(),
                meters: self.show_meters,
                visualizer_open: self.visualizer_open,
            }),
        }
    }

    // MARK: Events

    pub fn event(&mut self, event: &WindowEvent, gpu: &Gpu, library: &mut Library) -> Option<Request> {
        match event {
            WindowEvent::Resized(size) => self.surface.resize(gpu, size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui.set_scale(*scale_factor as f32);
                self.apply_min_size();
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Occluded(occluded) => self.occluded = *occluded,
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = *position;
                self.pointer_moved(library);
            }
            WindowEvent::CursorLeft { .. } => self.cursor = PhysicalPosition::new(-1.0, -1.0),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => return self.pressed(library),
                ElementState::Released => self.released(library),
            },
            WindowEvent::MouseWheel { delta, .. } => self.scrolled(*delta, library),
            WindowEvent::KeyboardInput { event, .. } => return self.key(event, library),
            WindowEvent::DroppedFile(path) => library.add(vec![path.clone()], true),
            _ => {}
        }
        None
    }

    fn cell_under_pointer(&self) -> Option<(u16, u16)> {
        let (width, height) = self.surface.size();
        self.ui.cell_at(self.cursor.x, self.cursor.y, width, height, self.screen.cols, self.screen.rows)
    }

    fn action_under_pointer(&self) -> Option<(Action, CellRect)> {
        let (col, row) = self.cell_under_pointer()?;
        self.screen.hit_test(col, row).map(|hit| (hit.action, hit.rect))
    }

    /// 0...1 across a slider's cells.
    fn fraction(&self, rect: CellRect) -> f64 {
        let (width, height) = self.surface.size();
        let col = self.ui.column_at(self.cursor.x, width, height, self.screen.cols, self.screen.rows);
        ((col - rect.x as f64) / rect.w.max(1) as f64).clamp(0.0, 1.0)
    }

    /// Windows has no native frame on a borderless window, so the edges resize it by hand.
    fn resize_edge(&self) -> Option<ResizeDirection> {
        if cfg!(target_os = "macos") {
            return None; // macOS keeps its native resizable frame
        }
        let (width, height) = self.surface.size();
        let margin = 6.0 * self.surface.window.scale_factor();
        let (x, y) = (self.cursor.x, self.cursor.y);
        let (left, right) = (x < margin, x > width as f64 - margin);
        let (top, bottom) = (y < margin, y > height as f64 - margin);
        Some(match (left, right, top, bottom) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (_, true, true, _) => ResizeDirection::NorthEast,
            (true, _, _, true) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::West,
            (_, true, ..) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            (.., true) => ResizeDirection::South,
            _ => return None,
        })
    }

    fn pointer_moved(&mut self, library: &mut Library) {
        match self.drag {
            Some(Drag::Seek(rect)) => self.scrub = Some(self.fraction(rect)),
            Some(Drag::Volume(rect)) => library.set_volume(self.fraction(rect) as f32),
            Some(Drag::Row(id)) => self.carry(id, library),
            None => {}
        }
        let icon = match (self.resize_edge(), self.action_under_pointer().map(|(a, _)| a)) {
            _ if matches!(self.drag, Some(Drag::Row(_))) => CursorIcon::Grabbing,
            (Some(direction), _) => CursorIcon::from(direction),
            (None, Some(Action::Prompt)) => CursorIcon::Text,
            (None, Some(Action::Drag | Action::List) | None) => CursorIcon::Default,
            (None, Some(_)) => CursorIcon::Pointer,
        };
        self.surface.window.set_cursor(icon);
    }

    fn pressed(&mut self, library: &mut Library) -> Option<Request> {
        if let Some(direction) = self.resize_edge() {
            let _ = self.surface.window.drag_resize_window(direction);
            return None;
        }
        let Some((action, rect)) = self.action_under_pointer() else { return None };
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(t, a)| a == action && now - t < Duration::from_millis(400));
        self.last_click = Some((now, action));
        if action != Action::Prompt {
            self.prompt_focused = false;
        }
        match action {
            Action::Close => return Some(Request::Quit),
            Action::Minimize => self.surface.window.set_minimized(true),
            Action::Drag => {
                let _ = self.surface.window.drag_window();
            }
            Action::Prompt => self.prompt_focused = true,
            Action::Seek => {
                self.drag = Some(Drag::Seek(rect));
                self.scrub = Some(self.fraction(rect));
            }
            Action::Volume => {
                self.drag = Some(Drag::Volume(rect));
                library.set_volume(self.fraction(rect) as f32);
            }
            Action::ToggleRemaining => self.show_remaining = !self.show_remaining,
            Action::Previous => library.previous(),
            Action::PlayPause => library.toggle(),
            Action::Next => library.next(),
            Action::Shuffle => library.toggle_shuffle(),
            Action::Repeat => library.cycle_repeat(),
            Action::Visualizer => return Some(Request::ToggleVisualizer),
            Action::Open => self.open_dialog(library),
            Action::Row(i) => {
                if let Some(&id) = self.visible.get(i) {
                    self.selected = Some(id);
                    if double {
                        library.play(id);
                    } else if self.prompt.is_empty() {
                        // Only the full list can be reordered; a filtered one would move tracks unpredictably.
                        self.drag = Some(Drag::Row(id));
                    }
                }
            }
            Action::List => self.selected = None,
            Action::Settings => self.settings_open = !self.settings_open,
            Action::ToggleShortcuts => return Some(Request::SetShortcuts(!self.shortcuts)),
            Action::PreviousScene | Action::NextScene => {
                let all = SceneKind::ALL;
                let index = all.iter().position(|&k| k == self.scene).unwrap_or(0);
                let step = if action == Action::NextScene { 1 } else { all.len() - 1 };
                return Some(Request::SetScene(all[(index + step) % all.len()]));
            }
            Action::ToggleMeters => return Some(Request::SetMeters(!self.show_meters)),
            Action::Reseed => return Some(Request::Reseed),
        }
        None
    }

    /// While dragging a row, the carried track moves to whichever row is under the pointer.
    fn carry(&mut self, id: TrackId, library: &mut Library) {
        let Some((Action::Row(i), _)) = self.action_under_pointer() else { return };
        let Some(&target) = self.visible.get(i) else { return };
        let tracks = library.playlist.tracks();
        let (Some(from), Some(to)) =
            (tracks.iter().position(|t| t.id == id), tracks.iter().position(|t| t.id == target))
        else {
            return;
        };
        if from != to {
            // `move_rows` takes the destination as an index before the move.
            library.playlist.move_rows(&[from], if to > from { to + 1 } else { to });
            library.dirty = true;
        }
    }

    fn released(&mut self, library: &mut Library) {
        if let (Some(Drag::Seek(_)), Some(fraction), Some(duration)) = (self.drag, self.scrub, library.duration()) {
            library.seek(fraction * duration);
        }
        self.drag = None;
        self.scrub = None;
    }

    fn scrolled(&mut self, delta: MouseScrollDelta, library: &mut Library) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y as f64,
            MouseScrollDelta::PixelDelta(p) => p.y / self.ui.cell_size().1 as f64,
        };
        match self.action_under_pointer().map(|(a, _)| a) {
            Some(Action::Volume) => library.set_volume((library.volume() as f64 + lines * 0.05).clamp(0.0, 1.0) as f32),
            Some(Action::List | Action::Row(_)) => {
                let steps = lines.round() as i64;
                self.scroll = (self.scroll as i64 - steps).max(0) as usize;
            }
            _ => {}
        }
    }

    fn open_dialog(&mut self, library: &mut Library) {
        let mut extensions: Vec<&str> = grimoire_core::audio_files::EXTENSIONS.to_vec();
        extensions.extend(grimoire_core::audio_files::PLAYLIST_EXTENSIONS);
        if let Some(paths) = rfd::FileDialog::new().set_title("Summon audio").add_filter("Audio", &extensions).pick_files() {
            library.add(paths, true);
        }
    }

    /// Keys. With shortcuts off (the default) only a few always work: ⌘/Ctrl+Q, and editing the prompt once it's
    /// been clicked. With them on, typing anywhere searches and the player keys below apply.
    fn key(&mut self, event: &KeyEvent, library: &mut Library) -> Option<Request> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let command = if cfg!(target_os = "macos") { self.modifiers.super_key() } else { self.modifiers.control_key() };
        if command {
            match &event.logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("q") => return Some(Request::Quit),
                _ if !self.shortcuts => {}
                Key::Character(c) if c.eq_ignore_ascii_case("o") => self.open_dialog(library),
                Key::Character(c) if c.eq_ignore_ascii_case("v") => return Some(Request::ToggleVisualizer),
                Key::Named(NamedKey::ArrowRight) => library.next(),
                Key::Named(NamedKey::ArrowLeft) => library.previous(),
                _ => {}
            }
            return None;
        }
        if self.prompt_focused {
            self.prompt_key(event, library);
            return None;
        }
        if !self.shortcuts {
            return None;
        }
        let big = self.modifiers.shift_key();
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.prompt.clear(),
            Key::Named(NamedKey::Enter) => self.play_selection(library),
            Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                if let Some(id) = self.selected.take() {
                    library.remove(&HashSet::from([id]));
                }
            }
            Key::Named(NamedKey::ArrowDown) => self.move_selection(1),
            Key::Named(NamedKey::ArrowUp) => self.move_selection(-1),
            Key::Named(NamedKey::ArrowRight) => library.seek(library.elapsed() + if big { 30.0 } else { 5.0 }),
            Key::Named(NamedKey::ArrowLeft) => library.seek((library.elapsed() - if big { 30.0 } else { 5.0 }).max(0.0)),
            Key::Named(NamedKey::Space) => library.toggle(),
            _ => {
                // Type-to-search: the first letter focuses the prompt.
                if let Some(text) = event.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control())) {
                    self.prompt.push_str(text);
                    self.prompt_focused = true;
                    self.scroll = 0;
                    self.selected = None;
                }
            }
        }
        None
    }

    /// Editing the focused prompt: text, Backspace, Esc to leave, Enter to play, ↑/↓ through the matches.
    fn prompt_key(&mut self, event: &KeyEvent, library: &mut Library) {
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.prompt.clear();
                self.prompt_focused = false;
            }
            Key::Named(NamedKey::Enter) => {
                self.play_selection(library);
                self.prompt.clear();
                self.prompt_focused = false;
            }
            Key::Named(NamedKey::Backspace) => {
                self.prompt.pop();
                self.scroll = 0;
            }
            Key::Named(NamedKey::ArrowDown) => self.move_selection(1),
            Key::Named(NamedKey::ArrowUp) => self.move_selection(-1),
            _ => {
                if let Some(text) = event.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control())) {
                    self.prompt.push_str(text);
                    self.scroll = 0;
                    self.selected = None;
                }
            }
        }
    }

    /// Plays the selected row if it's on screen, otherwise the first match.
    fn play_selection(&mut self, library: &mut Library) {
        let target = self.selected.filter(|id| self.visible.contains(id)).or(self.visible.first().copied());
        if let Some(id) = target {
            library.play(id);
            self.selected = Some(id);
        }
    }

    /// Moves the selection through the filtered list, scrolling to keep it on screen.
    fn move_selection(&mut self, step: i64) {
        if self.filtered.is_empty() {
            return;
        }
        let index = match self.selected.and_then(|id| self.filtered.iter().position(|&f| f == id)) {
            None => self.scroll,
            Some(i) => (i as i64 + step).clamp(0, self.filtered.len() as i64 - 1) as usize,
        };
        self.selected = Some(self.filtered[index]);
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + self.capacity.max(1) {
            self.scroll = index + 1 - self.capacity.max(1);
        }
    }
}

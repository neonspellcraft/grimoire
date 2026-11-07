//! The app: the main terminal window, the optional visualizer window, the library and the audio feed,
//! with settings restored at launch and saved as things change.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::controller::Controllers;
use crate::feed::AudioFeed;
use crate::gpu::{Gpu, WindowSurface};
use crate::library::Library;
use crate::main_window::{MainWindow, Request};
use crate::media::NowPlaying;
use crate::scenes::SceneKind;
use crate::settings::Settings;
use crate::vis_window::VisWindow;

pub const HELP: &str = "\
Click the prompt to search the playlist; Enter plays the match. ⌘/Ctrl+Q quits. Drop audio files, folders
or .m3u playlists on either window. Keyboard shortcuts (on by default; turn them off in settings ⚙):
Space play/pause, ←/→ seek (Shift: 30 s), ↑/↓ select, Delete removes, type anywhere to search,
⌘/Ctrl+←/→ previous/next, ⌘/Ctrl+O open, ⌘/Ctrl+V visualizer;
visualizer window: 1–4 or Tab switch scenes, R reseed, M band meters, F full screen.";

pub fn run(paths: Vec<PathBuf>) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App { initial: Some(paths), running: None, error: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    initial: Option<Vec<PathBuf>>,
    running: Option<Running>,
    error: Option<String>,
}

struct Running {
    instance: wgpu::Instance,
    gpu: Gpu,
    main: MainWindow,
    vis: Option<VisWindow>,
    library: Library,
    now_playing: Option<NowPlaying>,
    controllers: Controllers,
    feed: AudioFeed,
    settings: Settings,
    last_save: Instant,
    next_main_frame: Instant,
    next_vis_frame: Instant,
}

/// Frame interval while something is moving (scenes, playback, typing). 60 fps, as in the Swift app.
const FRAME: Duration = Duration::from_micros(16_667);
/// Idle main window: fast enough for the blinking clock and cursor.
const IDLE_FRAME: Duration = Duration::from_millis(125);

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(paths) = self.initial.take() else { return };
        match Running::new(event_loop, paths) {
            Ok(running) => self.running = Some(running),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(app) = &mut self.running else { return };
        if id == app.main.window().id() {
            let request = match &event {
                WindowEvent::CloseRequested => Some(Request::Quit),
                WindowEvent::RedrawRequested => {
                    app.main.draw(&app.gpu, &app.library, &app.feed);
                    None
                }
                other => {
                    // Input shows its effect right away instead of waiting for the next scheduled frame.
                    app.main.window().request_redraw();
                    app.main.event(other, &app.gpu, &mut app.library)
                }
            };
            match request {
                Some(Request::Quit) => event_loop.exit(),
                Some(Request::ToggleVisualizer) => app.toggle_visualizer(event_loop),
                Some(Request::SetShortcuts(on)) => {
                    app.settings.keyboard_shortcuts = on;
                    app.main.shortcuts = on;
                    if let Some(vis) = &mut app.vis {
                        vis.shortcuts = on;
                    }
                    app.library.dirty = true;
                }
                Some(Request::SetScene(kind)) => {
                    app.settings.scene = kind.raw().into();
                    if let Some(vis) = &mut app.vis {
                        vis.select(&app.gpu, kind);
                    }
                    app.library.dirty = true;
                }
                Some(Request::SetMeters(on)) => {
                    app.settings.show_meters = on;
                    if let Some(vis) = &mut app.vis {
                        vis.show_meters = on;
                    }
                    app.library.dirty = true;
                }
                Some(Request::SetDoomSound(on)) => {
                    app.settings.doom_sound = on;
                    app.main.doom_sound = on;
                    if let Some(vis) = &mut app.vis {
                        vis.set_game_sound(on);
                    }
                    app.library.dirty = true;
                }
                Some(Request::Reseed) => {
                    if let Some(vis) = &mut app.vis {
                        vis.reseed();
                    }
                }
                None => {}
            }
            app.sync_visualizer_state();
        } else if let Some(vis) = app.vis.as_mut().filter(|v| v.surface.window.id() == id) {
            if matches!(event, WindowEvent::RedrawRequested) {
                vis.draw(&app.gpu, &app.feed);
            } else if vis.event(&event, &app.gpu, &mut app.library).is_some() {
                app.close_visualizer();
            }
            app.sync_visualizer_state();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(app) = &mut self.running else { return };
        app.library.tick();
        // Controllers only ever drive the visualizer's game (Doom).
        for input in app.controllers.poll() {
            if let Some(vis) = &mut app.vis {
                vis.pad(input);
            }
        }
        if let Some(now_playing) = &mut app.now_playing {
            now_playing.tick(&mut app.library);
        }
        app.feed.update(&app.library);
        if app.library.dirty && app.last_save.elapsed() > Duration::from_secs(5) {
            app.save();
        }
        // Draw on a schedule instead of as fast as possible: 60 fps for anything moving, a slow tick for the
        // blinking clock and cursor when the main window is idle, nothing for hidden windows. Input redraws at once.
        let now = Instant::now();
        let mut wake = now + Duration::from_millis(250);
        if app.main.is_visible() {
            if now >= app.next_main_frame {
                app.main.window().request_redraw();
                let interval = if app.main.is_animating(&app.library, &app.feed) { FRAME } else { IDLE_FRAME };
                app.next_main_frame = now + interval;
            }
            wake = wake.min(app.next_main_frame);
        }
        if let Some(vis) = app.vis.as_ref().filter(|v| v.surface.is_drawable() && !v.occluded) {
            if now >= app.next_vis_frame {
                vis.surface.window.request_redraw();
                app.next_vis_frame = now + FRAME;
            }
            wake = wake.min(app.next_vis_frame);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(app) = &mut self.running {
            app.save();
        }
    }
}

impl Running {
    fn new(event_loop: &ActiveEventLoop, paths: Vec<PathBuf>) -> Result<Running, String> {
        let settings = Settings::load();
        let mut attributes = main_window_attributes()
            .with_inner_size(LogicalSize::<f64>::from(settings.main_size.unwrap_or((520.0, 420.0))));
        if let Some(position) = settings.main_position.filter(|&p| on_screen(event_loop, p)) {
            attributes = attributes.with_position(PhysicalPosition::<i32>::from(position));
        }
        let window = Arc::new(event_loop.create_window(attributes).map_err(|e| e.to_string())?);
        let instance = Gpu::instance();
        let (surface, gpu) = WindowSurface::new(&instance, None, window)?;
        let gpu = gpu.expect("the first window creates the device");
        let mut main = MainWindow::new(surface, &gpu);
        main.show_remaining = settings.show_remaining;
        main.shortcuts = settings.keyboard_shortcuts;
        main.doom_sound = settings.doom_sound;

        let mut library = Library::new();
        if paths.is_empty() {
            library.restore(&settings);
        } else {
            library.restore(&Settings { playlist: Vec::new(), current: None, ..settings.clone() });
            library.add(paths, true);
        }
        let now_playing = NowPlaying::new(main.window());
        let mut running = Running {
            instance,
            gpu,
            main,
            now_playing,
            controllers: Controllers::new(),
            vis: None,
            library,
            feed: AudioFeed::default(),
            settings,
            last_save: Instant::now(),
            next_main_frame: Instant::now(),
            next_vis_frame: Instant::now(),
        };
        if running.settings.visualizer_open {
            running.open_visualizer(event_loop);
        }
        running.sync_visualizer_state();
        Ok(running)
    }

    /// Keeps the settings panel's view of the visualizer (scene, meters) current, wherever it was changed from.
    fn sync_visualizer_state(&mut self) {
        match &self.vis {
            Some(vis) => {
                self.main.scene = vis.kind;
                self.main.show_meters = vis.show_meters;
            }
            None => {
                self.main.scene = SceneKind::from_raw(&self.settings.scene).unwrap_or(SceneKind::ThreeBody);
                self.main.show_meters = self.settings.show_meters;
            }
        }
    }

    fn toggle_visualizer(&mut self, event_loop: &ActiveEventLoop) {
        if self.vis.is_some() {
            self.close_visualizer();
        } else {
            self.open_visualizer(event_loop);
        }
        self.library.dirty = true;
    }

    fn open_visualizer(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = Window::default_attributes()
            .with_title("Grimoire Visualizer")
            .with_inner_size(LogicalSize::<f64>::from(self.settings.visualizer_size.unwrap_or((640.0, 400.0))))
            .with_min_inner_size(LogicalSize::new(240.0, 150.0));
        if let Some(position) = self.settings.visualizer_position.filter(|&p| on_screen(event_loop, p)) {
            attributes = attributes.with_position(PhysicalPosition::<i32>::from(position));
        }
        let opened = event_loop
            .create_window(attributes)
            .map_err(|e| e.to_string())
            .and_then(|window| WindowSurface::new(&self.instance, Some(&self.gpu), Arc::new(window)))
            .and_then(|(surface, _)| {
                let kind = SceneKind::from_raw(&self.settings.scene).unwrap_or(SceneKind::ThreeBody);
                VisWindow::new(surface, &self.gpu, kind, self.settings.show_meters, self.settings.doom_sound)
            });
        match opened {
            Ok(mut vis) => {
                vis.shortcuts = self.settings.keyboard_shortcuts;
                self.vis = Some(vis);
                self.main.visualizer_open = true;
            }
            Err(e) => eprintln!("grimoire: couldn't open the visualizer: {e}"),
        }
    }

    fn close_visualizer(&mut self) {
        if let Some(vis) = self.vis.take() {
            self.remember_visualizer(&vis);
        }
        self.main.visualizer_open = false;
        self.library.dirty = true;
    }

    fn remember_visualizer(&mut self, vis: &VisWindow) {
        let window = &vis.surface.window;
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        self.settings.visualizer_size = Some((size.width, size.height));
        self.settings.visualizer_position = window.outer_position().ok().map(|p| (p.x, p.y));
        self.settings.scene = vis.kind.raw().into();
        self.settings.show_meters = vis.show_meters;
    }

    fn save(&mut self) {
        self.library.save_into(&mut self.settings);
        let window = self.main.window();
        if !window.is_minimized().unwrap_or(false) {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            self.settings.main_size = Some((size.width, size.height));
            self.settings.main_position = window.outer_position().ok().map(|p| (p.x, p.y));
        }
        self.settings.show_remaining = self.main.show_remaining;
        self.settings.visualizer_open = self.vis.is_some();
        if let Some(vis) = self.vis.take() {
            self.remember_visualizer(&vis);
            self.vis = Some(vis);
        }
        self.settings.save();
        self.library.dirty = false;
        self.last_save = Instant::now();
    }
}

/// Borderless on Windows (the app draws its own frame); on macOS the native frame stays, with its title bar
/// made invisible, so resizing, shadows and rounded corners still come from the system.
fn main_window_attributes() -> WindowAttributes {
    let attributes = Window::default_attributes().with_title("Grimoire");
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attributes
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
            .with_titlebar_buttons_hidden(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        attributes.with_decorations(false).with_window_icon(window_icon())
    }
}

/// True if a saved window position is still on one of the connected screens.
fn on_screen(event_loop: &ActiveEventLoop, (x, y): (i32, i32)) -> bool {
    event_loop.available_monitors().any(|monitor| {
        let (origin, size) = (monitor.position(), monitor.size());
        x >= origin.x - 50
            && y >= origin.y - 50
            && x < origin.x + size.width as i32 - 50
            && y < origin.y + size.height as i32 - 50
    })
}

/// The app icon for the title bar and taskbar (macOS takes it from the bundle instead).
#[cfg(not(target_os = "macos"))]
fn window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
}

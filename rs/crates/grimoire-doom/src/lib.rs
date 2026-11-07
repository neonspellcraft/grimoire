//! The Doom engine (doomgeneric, from id Software's GPL source release) running inside Grimoire.
//!
//! The engine keeps all its state in C globals, so there is one engine per process. It runs on its own thread,
//! started the first time Doom is shown. That thread calls back into this file for every frame (`DG_DrawFrame`),
//! for the time, and for key presses. While Doom isn't on screen the engine is paused inside `DG_DrawFrame`
//! and the clock stops, so the game picks up exactly where it was left.
//!
//! Copyright (C) 2026 Grimoire contributors. GPL-2.0-or-later, like the engine.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{c_char, c_int, c_uchar, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub mod keys;

/// The engine's frame size (doomgeneric's default: Doom's 320×200, doubled).
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 400;

struct Shared {
    /// The latest frame, 0x00RRGGBB per pixel.
    frame: Mutex<Vec<u32>>,
    frame_serial: AtomicU64,
    keys: Mutex<VecDeque<(bool, u8)>>,
    /// Keys currently held, so they can be released when Doom loses the screen (no stuck "keep running forward").
    held: Mutex<HashSet<u8>>,
    active: Mutex<bool>,
    wake: Condvar,
    /// Time the engine has spent paused, subtracted from its clock.
    paused: Mutex<Duration>,
    start: Instant,
    quit_requested: AtomicBool,
    stopped: AtomicBool,
    config_dir: CString,
    sound: Arc<sound::Mixer>,
}

static ENGINE: OnceLock<Arc<Shared>> = OnceLock::new();

/// A handle to the one Doom engine.
#[derive(Clone)]
pub struct Doom(Arc<Shared>);

impl Doom {
    /// Starts the engine on `iwad` (the first call), or returns the running one. `config_dir` holds Doom's
    /// settings and savegames.
    pub fn start(iwad: &Path, config_dir: &Path) -> Result<Doom, String> {
        if let Some(shared) = ENGINE.get() {
            return Ok(Doom(shared.clone()));
        }
        if !iwad.is_file() {
            return Err(format!("can't find the game data at {}", iwad.display()));
        }
        std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
        let mut dir = config_dir.to_string_lossy().into_owned();
        if !dir.ends_with(std::path::MAIN_SEPARATOR) {
            dir.push(std::path::MAIN_SEPARATOR);
        }
        let shared = Arc::new(Shared {
            frame: Mutex::new(vec![0; WIDTH * HEIGHT]),
            frame_serial: AtomicU64::new(0),
            keys: Mutex::new(VecDeque::new()),
            held: Mutex::new(HashSet::new()),
            active: Mutex::new(true),
            wake: Condvar::new(),
            paused: Mutex::new(Duration::ZERO),
            start: Instant::now(),
            quit_requested: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            config_dir: CString::new(dir).map_err(|e| e.to_string())?,
            sound: Arc::new(sound::Mixer::default()),
        });
        if ENGINE.set(shared.clone()).is_err() {
            return Ok(Doom(ENGINE.get().unwrap().clone())); // another thread got there first
        }
        let args: Vec<CString> = ["grimoire", "-iwad", &iwad.to_string_lossy()]
            .iter()
            .map(|a| CString::new(*a).unwrap())
            .collect();
        std::thread::Builder::new()
            .name("doom".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
                // SAFETY: argv points at CStrings that live for the whole call; the engine never frees them.
                unsafe { grimoire_doom_run(argv.len() as c_int, argv.as_mut_ptr()) };
                if let Some(shared) = ENGINE.get() {
                    shared.stopped.store(true, Ordering::SeqCst);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Doom(shared))
    }

    /// Copies the newest frame into `out` if it changed since `serial`; returns the frame's serial.
    pub fn frame(&self, serial: u64, out: &mut Vec<u32>) -> u64 {
        let current = self.0.frame_serial.load(Ordering::Acquire);
        if current != serial {
            let frame = self.0.frame.lock().unwrap();
            out.clear();
            out.extend_from_slice(&frame);
        }
        current
    }

    /// A key went down or up (`code` from [`keys`]).
    pub fn key(&self, pressed: bool, code: u8) {
        {
            let mut held = self.0.held.lock().unwrap();
            if pressed {
                held.insert(code);
            } else {
                held.remove(&code);
            }
        }
        self.0.keys.lock().unwrap().push_back((pressed, code));
    }

    /// Runs the game while Doom is on screen and pauses it otherwise.
    pub fn set_active(&self, active: bool) {
        if !active {
            let held: Vec<u8> = self.0.held.lock().unwrap().drain().collect();
            let mut keys = self.0.keys.lock().unwrap();
            for code in held {
                keys.push_back((false, code));
            }
        }
        *self.0.active.lock().unwrap() = active;
        self.0.wake.notify_all();
    }

    /// True once after the player chose Quit in Doom's menu.
    pub fn take_quit_request(&self) -> bool {
        self.0.quit_requested.swap(false, Ordering::SeqCst)
    }

    /// The engine hit a fatal error and won't run again this session.
    pub fn has_stopped(&self) -> bool {
        self.0.stopped.load(Ordering::SeqCst)
    }

    /// Doom's sound effects (off unless turned on in Grimoire's settings).
    pub fn set_sound(&self, on: bool) {
        self.0.sound.enabled.store(on, Ordering::SeqCst);
        if !on {
            self.0.sound.silence();
        }
    }
}

/// Freedoom's first game, which ships with Grimoire: inside the macOS app's Resources, next to the Windows exe,
/// or (for development builds) where `scripts/fetch-freedoom.sh` puts it.
pub fn bundled_freedoom() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        exe_dir.join("freedoom1.wad"),
        exe_dir.join("../Resources/freedoom1.wad"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("freedoom/freedoom1.wad"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

extern "C" {
    fn grimoire_doom_run(argc: c_int, argv: *mut *mut c_char) -> c_int;
    static DG_ScreenBuffer: *mut u32;
}

fn engine() -> &'static Arc<Shared> {
    ENGINE.get().expect("the engine calls back only after it was started")
}

// Callbacks from the engine thread.

#[no_mangle]
extern "C" fn DG_Init() {}

#[no_mangle]
extern "C" fn DG_DrawFrame() {
    let shared = engine();
    {
        let mut frame = shared.frame.lock().unwrap();
        // SAFETY: doomgeneric allocates DG_ScreenBuffer as WIDTH × HEIGHT pixels before the first frame.
        let source = unsafe { std::slice::from_raw_parts(DG_ScreenBuffer, WIDTH * HEIGHT) };
        frame.copy_from_slice(source);
    }
    shared.frame_serial.fetch_add(1, Ordering::Release);
    // Off screen: wait here, with the clock stopped, until Doom is shown again.
    let mut active = shared.active.lock().unwrap();
    if !*active {
        let paused_at = Instant::now();
        while !*active {
            active = shared.wake.wait(active).unwrap();
        }
        *shared.paused.lock().unwrap() += paused_at.elapsed();
    }
}

#[no_mangle]
extern "C" fn DG_SleepMs(ms: u32) {
    std::thread::sleep(Duration::from_millis(ms as u64));
}

#[no_mangle]
extern "C" fn DG_GetTicksMs() -> u32 {
    let shared = engine();
    let paused = *shared.paused.lock().unwrap();
    (shared.start.elapsed().saturating_sub(paused).as_millis() & 0xFFFF_FFFF) as u32
}

#[no_mangle]
extern "C" fn DG_GetKey(pressed: *mut c_int, key: *mut c_uchar) -> c_int {
    let Some((down, code)) = engine().keys.lock().unwrap().pop_front() else { return 0 };
    // SAFETY: the engine passes valid pointers to locals.
    unsafe {
        *pressed = down as c_int;
        *key = code;
    }
    1
}

#[no_mangle]
extern "C" fn DG_SetWindowTitle(_title: *const c_char) {}

#[no_mangle]
extern "C" fn grimoire_doom_config_dir() -> *const c_char {
    engine().config_dir.as_ptr()
}

/// Doom's menu asked to quit: hand the window back to Grimoire and keep the engine running.
#[no_mangle]
extern "C" fn grimoire_doom_quit() -> c_int {
    engine().quit_requested.store(true, Ordering::SeqCst);
    1
}

mod sound {
    //! Doom's sound effects: DMX sound lumps (8-bit mono) mixed into their own output stream, so they play
    //! alongside Grimoire's music without touching it.

    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    const VOICES: usize = 16;

    #[derive(Clone, Default)]
    struct Voice {
        samples: Option<Arc<Vec<f32>>>,
        /// Source samples per output sample.
        step: f64,
        position: f64,
        left: f32,
        right: f32,
    }

    #[derive(Default)]
    pub struct Mixer {
        pub enabled: AtomicBool,
        voices: Mutex<Vec<Voice>>,
        cache: Mutex<HashMap<i32, (Arc<Vec<f32>>, u32)>>,
        output_rate: Mutex<Option<u32>>,
    }

    thread_local! {
        /// The output stream lives on the engine thread, which never ends.
        static STREAM: std::cell::RefCell<Option<cpal::Stream>> = const { std::cell::RefCell::new(None) };
    }

    impl Mixer {
        pub fn silence(&self) {
            for voice in self.voices.lock().unwrap().iter_mut() {
                voice.samples = None;
            }
        }

        /// Opens the output the first time a sound plays with sound on.
        fn ensure_stream(self: &Arc<Self>) -> Option<u32> {
            if let Some(rate) = *self.output_rate.lock().unwrap() {
                return Some(rate);
            }
            let device = cpal::default_host().default_output_device()?;
            let config = device.default_output_config().ok()?;
            let rate = config.sample_rate();
            let channels = config.channels() as usize;
            let mixer = self.clone();
            let stream = device
                .build_output_stream(
                    config.config(),
                    move |data: &mut [f32], _| mixer.render(data, channels),
                    |e| eprintln!("grimoire-doom: sound stream error: {e}"),
                    None,
                )
                .ok()?;
            stream.play().ok()?;
            STREAM.with(|s| *s.borrow_mut() = Some(stream));
            *self.voices.lock().unwrap() = vec![Voice::default(); VOICES];
            *self.output_rate.lock().unwrap() = Some(rate);
            Some(rate)
        }

        fn render(&self, data: &mut [f32], channels: usize) {
            data.fill(0.0);
            if !self.enabled.load(Ordering::Relaxed) {
                return;
            }
            let Ok(mut voices) = self.voices.try_lock() else { return };
            for voice in voices.iter_mut() {
                let Some(samples) = voice.samples.clone() else { continue };
                for frame in data.chunks_exact_mut(channels) {
                    let index = voice.position as usize;
                    if index + 1 >= samples.len() {
                        voice.samples = None;
                        break;
                    }
                    let t = (voice.position - index as f64) as f32;
                    let s = samples[index] * (1.0 - t) + samples[index + 1] * t;
                    frame[0] += s * voice.left;
                    if channels > 1 {
                        frame[1] += s * voice.right;
                    }
                    voice.position += voice.step;
                }
            }
            for sample in data.iter_mut() {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }

        /// Doom's volume is 0...127 and separation 0 (left) ... 254 (right), as the original sound code used them.
        fn gains(volume: c_int, separation: c_int) -> (f32, f32) {
            let (volume, separation) = (volume.clamp(0, 127) as f32, separation.clamp(0, 254) as f32);
            let left = (254.0 - separation) * volume / 127.0 / 255.0;
            let right = separation * volume / 127.0 / 255.0;
            (left * 0.7, right * 0.7)
        }

        /// DMX sound lump: format (3), sample rate, sample count, then 16 bytes of padding either side of the
        /// 8-bit unsigned samples.
        fn decode(data: &[u8]) -> Option<(Vec<f32>, u32)> {
            if data.len() < 8 || u16::from_le_bytes([data[0], data[1]]) != 3 {
                return None;
            }
            let rate = u16::from_le_bytes([data[2], data[3]]) as u32;
            let count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
            let body = data.get(8 + 16..8 + count.saturating_sub(16).min(data.len() - 8))?;
            Some((body.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(), rate.max(1)))
        }
    }

    #[no_mangle]
    extern "C" fn grimoire_doom_sfx_init() -> c_int {
        1
    }

    #[no_mangle]
    extern "C" fn grimoire_doom_sfx_start(
        channel: c_int,
        data: *const c_uchar,
        length: c_int,
        lump: c_int,
        volume: c_int,
        separation: c_int,
    ) {
        let mixer = &engine().sound;
        if !mixer.enabled.load(Ordering::Relaxed) || data.is_null() || length <= 0 {
            return;
        }
        let Some(output_rate) = mixer.ensure_stream() else { return };
        let decoded = {
            let mut cache = mixer.cache.lock().unwrap();
            match cache.get(&lump) {
                Some(entry) => Some(entry.clone()),
                None => {
                    // SAFETY: the engine passes a cached lump of `length` bytes.
                    let bytes = unsafe { std::slice::from_raw_parts(data, length as usize) };
                    Mixer::decode(bytes).map(|(samples, rate)| {
                        let entry = (Arc::new(samples), rate);
                        cache.insert(lump, entry.clone());
                        entry
                    })
                }
            }
        };
        let Some((samples, rate)) = decoded else { return };
        let (left, right) = Mixer::gains(volume, separation);
        let mut voices = mixer.voices.lock().unwrap();
        if let Some(voice) = voices.get_mut(channel as usize) {
            *voice = Voice { samples: Some(samples), step: rate as f64 / output_rate as f64, position: 0.0, left, right };
        }
    }

    #[no_mangle]
    extern "C" fn grimoire_doom_sfx_params(channel: c_int, volume: c_int, separation: c_int) {
        let (left, right) = Mixer::gains(volume, separation);
        if let Some(voice) = engine().sound.voices.lock().unwrap().get_mut(channel as usize) {
            voice.left = left;
            voice.right = right;
        }
    }

    #[no_mangle]
    extern "C" fn grimoire_doom_sfx_stop(channel: c_int) {
        if let Some(voice) = engine().sound.voices.lock().unwrap().get_mut(channel as usize) {
            voice.samples = None;
        }
    }

    #[no_mangle]
    extern "C" fn grimoire_doom_sfx_playing(channel: c_int) -> c_int {
        engine().sound.voices.lock().unwrap().get(channel as usize).is_some_and(|v| v.samples.is_some()) as c_int
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decodes_dmx_sound_lumps() {
            let mut lump = vec![3, 0, 0x11, 0x2B]; // format 3, 11025 Hz
            lump.extend(36u32.to_le_bytes()); // 4 samples + 32 bytes padding
            lump.extend([128; 16]);
            lump.extend([128, 255, 0, 192]);
            lump.extend([128; 16]);
            let (samples, rate) = Mixer::decode(&lump).unwrap();
            assert_eq!(rate, 11025);
            assert_eq!(samples.len(), 4);
            assert_eq!(samples[0], 0.0);
            assert!(samples[1] > 0.99 && samples[2] == -1.0);
            assert!(Mixer::decode(&[0, 0, 0]).is_none());
        }

        #[test]
        fn pans_like_the_original() {
            let (left, right) = Mixer::gains(127, 128);
            assert!((left - right).abs() < 0.01, "center");
            let (left, right) = Mixer::gains(127, 0);
            assert!(left > 0.6 && right == 0.0, "hard left");
        }
    }
}

//! Plays one audio file at a time (replaces `PlayerEngine`).
//!
//! Threads:
//! - the caller's (UI) thread owns `Player` and the cpal stream;
//! - a decode thread reads the file, converts it to the device format and fills a lock-free ring;
//! - the real-time audio callback drains the ring, applies volume and feeds the visualizer tap.
//!
//! Loads, seeks and stops bump a generation number. The decode thread stops producing old audio, publishes the
//! new generation, then waits until the callback has drained the ring under that generation, so stale audio
//! never plays after a seek.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::convert::Converter;
use crate::source::{FileInfo, Source};
use crate::{AudioTap, Error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    /// The current file played to its end by itself (not after stop or seek).
    Finished,
}

#[derive(Default)]
struct Shared {
    paused: AtomicBool,
    volume: AtomicU32,
    /// Generation whose audio the decode thread is now producing.
    decoder_gen: AtomicU64,
    /// Generation the callback has drained the ring for; audio plays only when this equals `decoder_gen`.
    drained_gen: AtomicU64,
    /// Set by the decode thread once every sample of that generation is in the ring.
    eos_gen: AtomicU64,
    /// Frames the callback has played in the current generation.
    played: AtomicU64,
    finished: AtomicBool,
    stream_failed: AtomicBool,
}

enum Command {
    Load { source: Box<Source>, start: f64, generation: u64 },
    Seek { seconds: f64, generation: u64 },
    Stop { generation: u64 },
    Output { producer: Producer<f32>, rate: u32, channels: usize },
}

struct Output {
    _stream: cpal::Stream,
    rate: u32,
    device: Option<cpal::DeviceId>,
}

pub struct Player {
    shared: Arc<Shared>,
    tap: Arc<AudioTap>,
    commands: Option<Sender<Command>>,
    thread: Option<JoinHandle<()>>,
    output: Option<Output>,
    info: Option<FileInfo>,
    state: PlayState,
    generation: u64,
    base_time: f64,
    last_device_check: Instant,
}

impl Player {
    pub fn new() -> Result<Player, Error> {
        let shared = Arc::new(Shared::default());
        shared.volume.store(1f32.to_bits(), Ordering::Relaxed);
        let tap = Arc::new(AudioTap::default());
        let (output, producer, channels) = build_output(&shared, &tap)?;
        let (sender, receiver) = mpsc::channel();
        let thread_shared = shared.clone();
        let rate = output.rate;
        let thread = thread::Builder::new()
            .name("grimoire-decode".into())
            .spawn(move || Decoding::new(thread_shared, producer, rate, channels).run(receiver))
            .map_err(|e| Error::Output(e.to_string()))?;
        Ok(Player {
            shared,
            tap,
            commands: Some(sender),
            thread: Some(thread),
            output: Some(output),
            info: None,
            state: PlayState::Stopped,
            generation: 0,
            base_time: 0.0,
            last_device_check: Instant::now(),
        })
    }

    pub fn tap(&self) -> &Arc<AudioTap> {
        &self.tap
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn info(&self) -> Option<&FileInfo> {
        self.info.as_ref()
    }

    pub fn duration(&self) -> Option<f64> {
        self.info.as_ref().and_then(|i| i.duration)
    }

    pub fn output_sample_rate(&self) -> Option<u32> {
        self.output.as_ref().map(|o| o.rate)
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Opens `path` and starts playing it (or holds it paused at the start when `play` is false).
    pub fn load(&mut self, path: &Path, play: bool) -> Result<FileInfo, Error> {
        let source = Source::open(path)?;
        let info = source.info.clone();
        let generation = self.next_generation(0.0);
        self.send(Command::Load { source: Box::new(source), start: 0.0, generation });
        self.info = Some(info.clone());
        self.set_state(if play { PlayState::Playing } else { PlayState::Paused });
        Ok(info)
    }

    pub fn play(&mut self) {
        if self.info.is_some() && self.state != PlayState::Playing {
            if self.state == PlayState::Stopped {
                // After stop or the end of the file, start over from the top.
                let generation = self.next_generation(0.0);
                self.send(Command::Seek { seconds: 0.0, generation });
            }
            self.set_state(PlayState::Playing);
        }
    }

    pub fn pause(&mut self) {
        if self.state == PlayState::Playing {
            // Stop the callback first, then fold what it played into base_time. Resuming counts from 0 again.
            self.shared.paused.store(true, Ordering::Relaxed);
            self.base_time = self.current_time();
            self.shared.played.store(0, Ordering::Relaxed);
            self.set_state(PlayState::Paused);
        }
    }

    pub fn toggle(&mut self) {
        match self.state {
            PlayState::Playing => self.pause(),
            _ => self.play(),
        }
    }

    pub fn stop(&mut self) {
        let generation = self.next_generation(0.0);
        self.send(Command::Stop { generation });
        self.set_state(PlayState::Stopped);
    }

    pub fn seek(&mut self, seconds: f64) {
        if self.info.is_none() {
            return;
        }
        let seconds = match self.duration() {
            Some(d) => seconds.clamp(0.0, d),
            None => seconds.max(0.0),
        };
        let generation = self.next_generation(seconds);
        self.send(Command::Seek { seconds, generation });
        if self.state == PlayState::Stopped {
            self.set_state(PlayState::Paused);
        }
    }

    /// Seconds into the current file.
    pub fn current_time(&self) -> f64 {
        if self.state == PlayState::Stopped {
            return 0.0;
        }
        let rate = self.output.as_ref().map_or(48_000, |o| o.rate) as f64;
        let played = if self.shared.drained_gen.load(Ordering::Acquire) == self.generation {
            self.shared.played.load(Ordering::Relaxed) as f64 / rate
        } else {
            0.0
        };
        let time = self.base_time + played;
        self.duration().map_or(time, |d| time.min(d))
    }

    /// Call regularly (e.g. every frame). Reports the end of a file and follows output device changes
    /// (headphones plugged in or out, a new default device) by moving playback to the new device.
    pub fn tick(&mut self) -> Option<PlayerEvent> {
        let failed = self.shared.stream_failed.swap(false, Ordering::Relaxed);
        if failed || self.last_device_check.elapsed() > Duration::from_secs(1) {
            self.last_device_check = Instant::now();
            let current = default_device_id();
            let changed = current.is_some() && current != self.output.as_ref().and_then(|o| o.device.clone());
            if failed || changed || self.output.is_none() {
                self.rebuild_output();
            }
        }
        if self.shared.finished.swap(false, Ordering::Relaxed) {
            self.base_time = 0.0;
            self.set_state(PlayState::Stopped);
            return Some(PlayerEvent::Finished);
        }
        None
    }

    fn rebuild_output(&mut self) {
        let resume_at = self.current_time();
        self.output = None; // release the old device first
        match build_output(&self.shared, &self.tap) {
            Ok((output, producer, channels)) => {
                let rate = output.rate;
                self.output = Some(output);
                self.send(Command::Output { producer, rate, channels });
                if self.info.is_some() && self.state != PlayState::Stopped {
                    let generation = self.next_generation(resume_at);
                    self.send(Command::Seek { seconds: resume_at, generation });
                }
            }
            Err(e) => eprintln!("grimoire: no audio output: {e}"),
        }
    }

    /// Everything the callback plays from now on belongs to a new generation starting at `base_time`.
    fn next_generation(&mut self, base_time: f64) -> u64 {
        self.generation += 1;
        self.base_time = base_time;
        self.generation
    }

    fn set_state(&mut self, state: PlayState) {
        self.state = state;
        self.shared.paused.store(state != PlayState::Playing, Ordering::Relaxed);
    }

    fn send(&self, command: Command) {
        if let Some(sender) = &self.commands {
            let _ = sender.send(command);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.commands = None; // closes the channel, which ends the decode thread
        self.output = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn default_device_id() -> Option<cpal::DeviceId> {
    cpal::default_host().default_output_device().and_then(|d| d.id().ok())
}

fn build_output(shared: &Arc<Shared>, tap: &Arc<AudioTap>) -> Result<(Output, Producer<f32>, usize), Error> {
    let device = cpal::default_host().default_output_device().ok_or(Error::NoDevice)?;
    let device_id = device.id().ok();
    let supported = device.default_output_config().map_err(|e| Error::Output(e.to_string()))?;
    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.config();
    // A quarter second of buffering: plenty against decode hiccups, small enough that seeks feel instant.
    let (producer, consumer) = RingBuffer::new((rate as usize / 4) * channels);
    let shared = shared.clone();
    let tap = tap.clone();
    use cpal::SampleFormat as F;
    let stream = match supported.sample_format() {
        F::F32 => build_stream::<f32>(&device, &config, consumer, shared, tap),
        F::I16 => build_stream::<i16>(&device, &config, consumer, shared, tap),
        F::U16 => build_stream::<u16>(&device, &config, consumer, shared, tap),
        F::I32 => build_stream::<i32>(&device, &config, consumer, shared, tap),
        F::F64 => build_stream::<f64>(&device, &config, consumer, shared, tap),
        other => return Err(Error::Output(format!("unsupported sample format {other}"))),
    }?;
    stream.play().map_err(|e| Error::Output(e.to_string()))?;
    Ok((Output { _stream: stream, rate, device: device_id }, producer, channels))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: Consumer<f32>,
    shared: Arc<Shared>,
    tap: Arc<AudioTap>,
) -> Result<cpal::Stream, Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate;
    let mut mono = vec![0.0f32; 16_384];
    let error_shared = shared.clone();
    tap.reset(rate);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                let silence = T::from_sample(0.0f32);
                let decoder_gen = shared.decoder_gen.load(Ordering::Acquire);
                if shared.drained_gen.load(Ordering::Relaxed) != decoder_gen {
                    let stale = consumer.slots();
                    if let Ok(chunk) = consumer.read_chunk(stale) {
                        chunk.commit_all();
                    }
                    shared.played.store(0, Ordering::Relaxed);
                    tap.reset(rate);
                    shared.drained_gen.store(decoder_gen, Ordering::Release);
                    data.fill(silence);
                    return;
                }
                if shared.paused.load(Ordering::Relaxed) {
                    data.fill(silence);
                    return;
                }
                let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let frames = data.len() / channels;
                let ready = (consumer.slots() / channels).min(frames);
                if mono.len() < ready {
                    mono.resize(ready, 0.0); // only if the device suddenly asks for a huge buffer
                }
                let tapped = channels.min(2);
                for (frame, out) in data.chunks_exact_mut(channels).take(ready).enumerate() {
                    let mut sum = 0.0;
                    for (c, sample) in out.iter_mut().enumerate() {
                        let value = consumer.pop().unwrap_or(0.0);
                        if c < tapped {
                            sum += value;
                        }
                        *sample = T::from_sample(value * volume);
                    }
                    mono[frame] = sum / tapped as f32;
                }
                data[ready * channels..].fill(silence);
                shared.played.fetch_add(ready as u64, Ordering::Relaxed);
                tap.push(mono[..ready].iter().copied());
                if ready < frames && consumer.is_empty() {
                    let eos = shared.eos_gen.load(Ordering::Acquire);
                    if eos != 0
                        && eos == decoder_gen
                        && shared.eos_gen.compare_exchange(eos, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok()
                    {
                        shared.finished.store(true, Ordering::Relaxed);
                    }
                }
            },
            move |error| {
                eprintln!("grimoire: audio stream error: {error}");
                error_shared.stream_failed.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| Error::Output(e.to_string()))
}

/// State owned by the decode thread.
struct Decoding {
    shared: Arc<Shared>,
    producer: Producer<f32>,
    out_rate: u32,
    out_channels: usize,
    source: Option<Box<Source>>,
    converter: Option<Converter>,
    pending: VecDeque<f32>,
    generation: u64,
    end_of_file: bool,
    end_published: bool,
}

impl Decoding {
    fn new(shared: Arc<Shared>, producer: Producer<f32>, out_rate: u32, out_channels: usize) -> Self {
        Decoding {
            shared,
            producer,
            out_rate,
            out_channels,
            source: None,
            converter: None,
            pending: VecDeque::new(),
            generation: 0,
            end_of_file: false,
            end_published: false,
        }
    }

    fn run(mut self, commands: Receiver<Command>) {
        loop {
            let busy = self.source.is_some() && !self.end_published;
            let command = if busy {
                match commands.try_recv() {
                    Ok(command) => Some(command),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                }
            } else {
                match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            };
            match command {
                Some(command) => self.handle(command),
                None => self.work(),
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load { source, start, generation } => {
                self.source = Some(source);
                self.restart(start, generation);
            }
            Command::Seek { seconds, generation } => self.restart(seconds, generation),
            Command::Stop { generation } => {
                self.source = None;
                self.restart(0.0, generation);
            }
            Command::Output { producer, rate, channels } => {
                self.producer = producer;
                self.out_rate = rate;
                self.out_channels = channels;
                self.pending.clear();
                // The player follows this with a seek, which rebuilds the converter for the new rate.
            }
        }
    }

    fn restart(&mut self, seconds: f64, generation: u64) {
        self.pending.clear();
        self.end_of_file = false;
        self.end_published = false;
        self.generation = generation;
        self.converter = None;
        if let Some(source) = &mut self.source {
            if let Err(e) = source.seek(seconds) {
                eprintln!("grimoire: seek failed: {e}");
            }
            self.converter =
                Some(Converter::new(source.info.sample_rate, source.info.channels, self.out_rate, self.out_channels));
        }
        // From here on only new audio is produced; wait for the callback to throw away what's left of the old.
        self.shared.decoder_gen.store(generation, Ordering::Release);
        let deadline = Instant::now() + Duration::from_millis(300);
        while self.shared.drained_gen.load(Ordering::Acquire) != generation && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn work(&mut self) {
        if !self.pending.is_empty() {
            let count = self.producer.slots().min(self.pending.len());
            if count == 0 {
                thread::sleep(Duration::from_millis(5));
                return;
            }
            if let Ok(chunk) = self.producer.write_chunk_uninit(count) {
                chunk.fill_from_iter(self.pending.drain(..count));
            }
            return;
        }
        if self.end_of_file {
            self.shared.eos_gen.store(self.generation, Ordering::Release);
            self.end_published = true;
            return;
        }
        let (Some(source), Some(converter)) = (&mut self.source, &mut self.converter) else { return };
        let pending = &mut self.pending;
        match source.next_chunk(|channels| converter.process(channels, pending)) {
            Ok(true) => {}
            Ok(false) => {
                converter.flush(pending);
                self.end_of_file = true;
            }
            Err(e) => {
                eprintln!("grimoire: decode error: {e}");
                converter.flush(pending);
                self.end_of_file = true;
            }
        }
    }
}

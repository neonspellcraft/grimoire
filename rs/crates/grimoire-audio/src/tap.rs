use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Captures what's playing into a mono ring buffer for visualizers.
///
/// Written from the real-time audio callback, so it never locks or allocates: samples are stored as atomic bit
/// patterns. A reader racing the writer can at worst see a few samples from one callback later than expected,
/// which is invisible in a visualizer.
pub struct AudioTap {
    ring: Box<[AtomicU32]>,
    /// Total samples written since the last reset (an absolute index into the stream).
    written: AtomicUsize,
    sample_rate: AtomicU32,
    chunk: AtomicUsize,
}

#[derive(Clone, Copy, Debug)]
pub struct TapPosition {
    pub written: usize,
    pub sample_rate: f64,
    /// Size of the most recent chunk, roughly how far behind real time the newest sample can be.
    pub chunk: usize,
}

impl Default for AudioTap {
    fn default() -> Self {
        AudioTap {
            ring: (0..Self::CAPACITY).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicUsize::new(0),
            sample_rate: AtomicU32::new(44_100),
            chunk: AtomicUsize::new(512),
        }
    }
}

impl AudioTap {
    pub const CAPACITY: usize = 16_384;

    pub fn reset(&self, sample_rate: u32) {
        self.sample_rate.store(sample_rate.max(1), Ordering::Relaxed);
        self.written.store(0, Ordering::Release);
    }

    /// Appends one callback's worth of mono samples. Real-time safe.
    pub fn push(&self, samples: impl ExactSizeIterator<Item = f32>) {
        let count = samples.len();
        if count == 0 {
            return;
        }
        let start = self.written.load(Ordering::Relaxed);
        for (i, sample) in samples.enumerate() {
            self.ring[(start + i) % Self::CAPACITY].store(sample.to_bits(), Ordering::Relaxed);
        }
        self.chunk.store(count, Ordering::Relaxed);
        self.written.store(start + count, Ordering::Release);
    }

    pub fn position(&self) -> TapPosition {
        TapPosition {
            written: self.written.load(Ordering::Acquire),
            sample_rate: self.sample_rate.load(Ordering::Relaxed) as f64,
            chunk: self.chunk.load(Ordering::Relaxed),
        }
    }

    /// Copies the `output.len()` samples that end at absolute index `end`. False if they aren't buffered.
    pub fn copy_window(&self, end: usize, output: &mut [f32]) -> bool {
        let written = self.written.load(Ordering::Acquire);
        let Some(start) = end.checked_sub(output.len()) else { return false };
        if end > written || start + Self::CAPACITY < written {
            return false;
        }
        for (i, out) in output.iter_mut().enumerate() {
            *out = f32::from_bits(self.ring[(start + i) % Self::CAPACITY].load(Ordering::Relaxed));
        }
        true
    }
}

/// Pulls a fixed-size window of recent samples from an `AudioTap` once per display frame.
///
/// The tap fills in chunks (one audio callback each), so the reader keeps its own cursor that advances in real
/// time and trails the newest sample by about one chunk. That gives smooth motion at the display's frame rate.
/// Each visualizer owns its own reader; they can share one tap.
pub struct TapReader {
    pub samples: Vec<f32>,
    pub sample_rate: f64,
    cursor: f64,
    last_written: usize,
}

impl TapReader {
    pub fn new(window_size: usize) -> Self {
        TapReader { samples: vec![0.0; window_size], sample_rate: 44_100.0, cursor: 0.0, last_written: 0 }
    }

    /// Advances by `dt` seconds and refreshes `samples`. Returns false when there's no audio to show.
    pub fn read(&mut self, tap: &AudioTap, dt: f64, playing: bool) -> bool {
        let position = tap.position();
        self.sample_rate = position.sample_rate;
        let last_written = std::mem::replace(&mut self.last_written, position.written);
        if !playing || position.written < self.samples.len() {
            return false;
        }
        let (written, chunk) = (position.written as f64, position.chunk as f64);
        if position.written < last_written || self.cursor > written + chunk {
            self.cursor = written - chunk; // tap was reset (new track or seek)
        }
        self.cursor += dt * position.sample_rate;
        self.cursor = self.cursor.max(written - 2.0 * chunk).min(written);
        tap.copy_window(self.cursor as usize, &mut self.samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_reads_back_what_was_written() {
        let tap = AudioTap::default();
        tap.reset(48_000);
        for block in 0..40 {
            tap.push((0..512).map(|i| (block * 512 + i) as f32));
        }
        let mut out = [0.0; 4];
        assert!(tap.copy_window(20_000, &mut out));
        assert_eq!(out, [19_996.0, 19_997.0, 19_998.0, 19_999.0]);
        assert!(!tap.copy_window(40 * 512 + 1, &mut out), "past the newest sample");
        assert!(!tap.copy_window(100, &mut out), "overwritten by the ring");
    }

    #[test]
    fn reader_trails_newest_sample_and_recovers_from_reset() {
        let tap = AudioTap::default();
        tap.reset(48_000);
        let mut reader = TapReader::new(2048);
        assert!(!reader.read(&tap, 1.0 / 60.0, true), "not enough audio yet");
        for _ in 0..10 {
            tap.push(std::iter::repeat(0.25).take(512));
        }
        assert!(reader.read(&tap, 1.0 / 60.0, true));
        assert!(reader.samples.iter().all(|&s| s == 0.25));
        assert!(!reader.read(&tap, 1.0 / 60.0, false), "paused shows nothing");

        tap.reset(48_000);
        for _ in 0..5 {
            tap.push(std::iter::repeat(-0.5).take(512));
        }
        assert!(reader.read(&tap, 1.0 / 60.0, true));
        assert!(reader.samples.iter().all(|&s| s == -0.5));
    }
}

//! Turns decoded planar audio at the file's rate into interleaved audio at the output device's rate and
//! channel count. AVAudioEngine did this implicitly on macOS; here it's explicit so it behaves the same on
//! WASAPI, which only accepts the device's mix format.

use std::collections::VecDeque;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

pub struct Converter {
    /// Channels taken from the file: 1 or 2 (anything past stereo is dropped for now).
    in_channels: usize,
    out_channels: usize,
    resampler: Option<SincFixedIn<f32>>,
    ratio: f64,
    input: Vec<Vec<f32>>,
    /// Leading resampler output to drop so the stream starts exactly on time.
    delay_left: usize,
    total_in: u64,
    total_out: u64,
}

impl Converter {
    pub const CHUNK: usize = 1024;

    pub fn new(in_rate: u32, file_channels: usize, out_rate: u32, out_channels: usize) -> Self {
        let in_channels = file_channels.clamp(1, 2);
        let resampler = (in_rate != out_rate).then(|| {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            SincFixedIn::new(out_rate as f64 / in_rate as f64, 1.0, params, Self::CHUNK, in_channels)
                .expect("valid resampler parameters")
        });
        Converter {
            in_channels,
            out_channels: out_channels.max(1),
            delay_left: resampler.as_ref().map_or(0, |r| r.output_delay()),
            resampler,
            ratio: out_rate as f64 / in_rate as f64,
            input: vec![Vec::new(); in_channels],
            total_in: 0,
            total_out: 0,
        }
    }

    /// Adds one decoded chunk (one slice per file channel); converted frames are appended to `out`.
    pub fn process(&mut self, channels: &[&[f32]], out: &mut VecDeque<f32>) {
        let Some(first) = channels.first() else { return };
        let frames = first.len();
        let pick = |c: usize| channels.get(c).copied().unwrap_or(first);
        if self.resampler.is_none() {
            let planar: Vec<&[f32]> = (0..self.in_channels).map(pick).collect();
            self.interleave(&planar, frames, out);
            return;
        }
        for c in 0..self.in_channels {
            self.input[c].extend_from_slice(pick(c));
        }
        self.total_in += frames as u64;
        loop {
            let resampler = self.resampler.as_mut().unwrap();
            let needed = resampler.input_frames_next();
            if self.input[0].len() < needed {
                break;
            }
            let chunk: Vec<&[f32]> = self.input.iter().map(|v| &v[..needed]).collect();
            let result = resampler.process(&chunk, None).expect("chunk sized by input_frames_next");
            for v in &mut self.input {
                v.drain(..needed);
            }
            self.emit(result, None, out);
        }
    }

    /// End of stream: pushes out what's still inside the resampler, trimmed so the output is exactly as long
    /// as the input (at the new rate).
    pub fn flush(&mut self, out: &mut VecDeque<f32>) {
        if self.resampler.is_none() {
            return;
        }
        let expected = (self.total_in as f64 * self.ratio).round() as u64;
        let rest = std::mem::replace(&mut self.input, vec![Vec::new(); self.in_channels]);
        let mut first = Some(rest);
        for _ in 0..8 {
            if self.total_out >= expected {
                break;
            }
            let resampler = self.resampler.as_mut().unwrap();
            let result = match first.take() {
                Some(rest) => resampler.process_partial(Some(&rest), None),
                None => resampler.process_partial::<Vec<f32>>(None, None),
            };
            let Ok(result) = result else { break };
            self.emit(result, Some(expected), out);
        }
    }

    /// Drops the resampler's leading delay, then interleaves; `limit` caps the total frames ever emitted.
    fn emit(&mut self, mut planar: Vec<Vec<f32>>, limit: Option<u64>, out: &mut VecDeque<f32>) {
        let skip = self.delay_left.min(planar[0].len());
        if skip > 0 {
            for channel in &mut planar {
                channel.drain(..skip);
            }
            self.delay_left -= skip;
        }
        if let Some(limit) = limit {
            let room = limit.saturating_sub(self.total_out) as usize;
            for channel in &mut planar {
                channel.truncate(room);
            }
        }
        let frames = planar[0].len();
        self.total_out += frames as u64;
        let slices: Vec<&[f32]> = planar.iter().map(|v| v.as_slice()).collect();
        self.interleave(&slices, frames, out);
    }

    fn interleave(&self, planar: &[&[f32]], frames: usize, out: &mut VecDeque<f32>) {
        out.reserve(frames * self.out_channels);
        for i in 0..frames {
            let (left, right) = (planar[0][i], planar[planar.len() - 1][i]);
            if self.out_channels == 1 {
                out.push_back(0.5 * (left + right));
                continue;
            }
            out.push_back(left);
            out.push_back(right);
            out.extend(std::iter::repeat(0.0).take(self.out_channels - 2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_interleaves_and_upmixes_mono() {
        let mut converter = Converter::new(48_000, 1, 48_000, 2);
        let mut out = VecDeque::new();
        converter.process(&[&[0.1, 0.2]], &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), [0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn resampling_keeps_length_and_pitch() {
        // One second of 1 kHz at 44.1 kHz → 48 kHz: the right number of frames, zero crossings still at 1 kHz.
        let input: Vec<f32> =
            (0..44_100).map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 44_100.0).sin() as f32 * 0.5).collect();
        let mut converter = Converter::new(44_100, 1, 48_000, 1);
        let mut out = VecDeque::new();
        for chunk in input.chunks(1152) {
            converter.process(&[chunk], &mut out);
        }
        converter.flush(&mut out);
        let out: Vec<f32> = out.into_iter().collect();
        assert!((out.len() as i64 - 48_000).abs() <= 2, "got {} frames", out.len());
        let middle = &out[4800..43_200];
        let crossings = middle.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((crossings as i64 - 800).abs() <= 1, "crossings {crossings}");
        let peak = middle.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((peak - 0.5).abs() < 0.01, "peak {peak}");
    }
}

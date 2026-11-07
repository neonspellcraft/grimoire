use std::ops::{Index, IndexMut};

use crate::{unit, PowerSpectrum};

/// One value per band: low (0–120 Hz), mid (120–800 Hz), high (800 Hz–20 kHz).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BandLevels {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl BandLevels {
    pub const ZERO: BandLevels = BandLevels { low: 0.0, mid: 0.0, high: 0.0 };

    pub const fn new(low: f32, mid: f32, high: f32) -> Self {
        BandLevels { low, mid, high }
    }

    pub fn loudest(&self) -> f32 {
        self.low.max(self.mid).max(self.high)
    }
}

impl Index<usize> for BandLevels {
    type Output = f32;
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.low,
            1 => &self.mid,
            _ => &self.high,
        }
    }
}

impl IndexMut<usize> for BandLevels {
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        match index {
            0 => &mut self.low,
            1 => &mut self.mid,
            _ => &mut self.high,
        }
    }
}

/// Measures the energy in the three bands, as 0...1 levels mapped from `floor_db...ceiling_db`.
pub struct MultibandAnalyzer {
    pub floor_db: f32,
    pub ceiling_db: f32,
    spectrum: PowerSpectrum,
}

impl Default for MultibandAnalyzer {
    fn default() -> Self {
        Self::new(2048)
    }
}

impl MultibandAnalyzer {
    /// Upper edge of each band in Hz. The low band starts just above DC.
    pub const CROSSOVERS: [f64; 3] = [120.0, 800.0, 20_000.0];

    pub fn new(fft_size: usize) -> Self {
        MultibandAnalyzer { floor_db: -66.0, ceiling_db: -10.0, spectrum: PowerSpectrum::new(fft_size) }
    }

    pub fn fft_size(&self) -> usize {
        self.spectrum.size()
    }

    /// Uses the last `fft_size` samples of `samples`.
    pub fn levels(&mut self, samples: &[f32], sample_rate: f64) -> BandLevels {
        self.spectrum.compute(samples);
        let power = self.spectrum.power();
        let bin_width = self.spectrum.bin_width(sample_rate);
        let bins = power.len();
        let mut edges = [1usize; 4];
        for (i, crossover) in Self::CROSSOVERS.iter().enumerate() {
            let edge = (crossover.min(sample_rate / 2.0) / bin_width).round() as usize;
            edges[i + 1] = edge.max(edges[i]).min(bins);
        }
        let mut result = BandLevels::ZERO;
        for band in 0..3 {
            let sum: f32 = power[edges[band]..edges[band + 1]].iter().sum();
            let amplitude = sum.sqrt() * self.spectrum.amplitude_scale();
            if amplitude <= 0.0 {
                continue;
            }
            let db = 20.0 * amplitude.log10();
            result[band] = unit((db - self.floor_db) / (self.ceiling_db - self.floor_db));
        }
        result
    }
}

/// Turns raw band levels into something good to animate with:
/// - `envelope`: fast-attack / slower-release follower, auto-ranged between each band's recent floor and peak,
///   so quiet passages, loud passages and dense mixes all swing across the full 0...1.
/// - `hits`: jumps to 1 on a transient (a kick, a snare, a hi-hat burst) and fades out.
#[derive(Clone, Debug)]
pub struct BandFollower {
    pub envelope: BandLevels,
    pub hits: BandLevels,
    pub attack: f64,
    pub release: f64,
    pub hit_fade: f64,
    /// Recent loudest and quietest level per band. Both drift slowly toward the signal so the range adapts.
    peak: BandLevels,
    floor: BandLevels,
    average: BandLevels,
}

impl Default for BandFollower {
    fn default() -> Self {
        BandFollower {
            envelope: BandLevels::ZERO,
            hits: BandLevels::ZERO,
            attack: 0.015,
            release: 0.2,
            hit_fade: 0.25,
            peak: BandLevels::new(0.5, 0.5, 0.5),
            floor: BandLevels::ZERO,
            average: BandLevels::ZERO,
        }
    }
}

impl BandFollower {
    /// The range never shrinks below this, so steady sounds still read as "on" instead of collapsing to 0.
    const MINIMUM_RANGE: f32 = 0.2;

    /// `input` None means silence.
    pub fn step(&mut self, input: Option<BandLevels>, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        for band in 0..3 {
            let raw = input.map_or(0.0, |l| l[band]);
            self.peak[band] = raw.max(self.peak[band] - (dt * 0.05) as f32);
            self.floor[band] = raw.min(self.floor[band] + (dt * 0.08) as f32);
            self.floor[band] = self.floor[band].min((self.peak[band] - Self::MINIMUM_RANGE).max(0.0));
            let range = (self.peak[band] - self.floor[band]).max(Self::MINIMUM_RANGE);
            let target = unit((raw - self.floor[band]) / range);
            let time_constant = if target > self.envelope[band] { self.attack } else { self.release };
            self.envelope[band] += (target - self.envelope[band]) * (1.0 - (-dt / time_constant).exp()) as f32;

            let rise = raw - self.average[band];
            self.average[band] += rise * (1.0 - (-dt / 0.4).exp()) as f32;
            if rise > 0.08 + 0.15 * self.average[band] && self.hits[band] < 0.5 {
                self.hits[band] = 1.0;
            } else {
                self.hits[band] = (self.hits[band] - (dt / self.hit_fade) as f32).max(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels_for_sine(frequency: f64, sample_rate: f64) -> BandLevels {
        let samples: Vec<f32> = (0..2048)
            .map(|i| 0.5 * (2.0 * std::f64::consts::PI * frequency * i as f64 / sample_rate).sin() as f32)
            .collect();
        MultibandAnalyzer::default().levels(&samples, sample_rate)
    }

    #[test]
    fn each_tone_lands_in_its_band() {
        let bass = levels_for_sine(60.0, 44_100.0);
        assert!(bass.low > bass.mid && bass.low > bass.high);
        let mid = levels_for_sine(400.0, 44_100.0);
        assert!(mid.mid > mid.low && mid.mid > mid.high);
        let high = levels_for_sine(5000.0, 96_000.0);
        assert!(high.high > high.low && high.high > high.mid);
    }

    #[test]
    fn silence_is_zero() {
        assert_eq!(MultibandAnalyzer::default().levels(&[0.0; 2048], 44_100.0), BandLevels::ZERO);
    }

    #[test]
    fn follower_auto_gains_and_releases() {
        let mut follower = BandFollower::default();
        for _ in 0..60 {
            follower.step(Some(BandLevels::new(0.6, 0.6, 0.6)), 1.0 / 60.0);
        }
        assert!(follower.envelope.low > 0.95, "a steady level should fill the range after auto-gain");
        for _ in 0..120 {
            follower.step(None, 1.0 / 60.0);
        }
        assert!(follower.envelope.low < 0.01);
    }

    #[test]
    fn follower_detects_hits() {
        let mut follower = BandFollower::default();
        for _ in 0..120 {
            follower.step(Some(BandLevels::new(0.2, 0.2, 0.2)), 1.0 / 60.0);
        }
        assert_eq!(follower.hits.low, 0.0);
        follower.step(Some(BandLevels::new(0.9, 0.2, 0.2)), 1.0 / 60.0);
        assert_eq!(follower.hits.low, 1.0);
        assert_eq!(follower.hits.mid, 0.0);
    }

    #[test]
    fn dense_loud_signal_still_swings() {
        let mut follower = BandFollower::default();
        let mut lows = Vec::new();
        for frame in 0..(60 * 20) {
            let pulse = if frame % 30 < 5 { 0.95 } else { 0.8 }; // loud bed with kicks on top
            follower.step(Some(BandLevels::new(pulse, 0.8, 0.8)), 1.0 / 60.0);
            if frame > 60 * 10 {
                lows.push(follower.envelope.low);
            }
        }
        let (max, min) = lows.iter().fold((f32::MIN, f32::MAX), |(hi, lo), &x| (hi.max(x), lo.min(x)));
        assert!(max - min > 0.4, "kicks should still move the envelope a lot");
    }
}

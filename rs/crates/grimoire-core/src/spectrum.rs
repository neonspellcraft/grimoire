use std::ops::RangeInclusive;

use crate::{unit, PowerSpectrum};

/// Turns a window of mono samples into `band_count` log-spaced spectrum levels in 0...1.
///
/// Uses `PowerSpectrum`, then takes each band's peak amplitude in dB, mapped from `floor_db...ceiling_db`.
pub struct SpectrumAnalyzer {
    pub band_count: usize,
    pub min_frequency: f64,
    pub max_frequency: f64,
    pub floor_db: f32,
    pub ceiling_db: f32,
    spectrum: PowerSpectrum,
    bin_ranges: Vec<RangeInclusive<usize>>,
    bin_ranges_sample_rate: f64,
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new(2048, 20)
    }
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, band_count: usize) -> Self {
        SpectrumAnalyzer {
            band_count,
            min_frequency: 50.0,
            max_frequency: 16_000.0,
            floor_db: -72.0,
            ceiling_db: -6.0,
            spectrum: PowerSpectrum::new(fft_size),
            bin_ranges: Vec::new(),
            bin_ranges_sample_rate: 0.0,
        }
    }

    pub fn fft_size(&self) -> usize {
        self.spectrum.size()
    }

    /// Uses the last `fft_size` samples of `samples`.
    pub fn bands(&mut self, samples: &[f32], sample_rate: f64) -> Vec<f32> {
        self.spectrum.compute(samples);
        if sample_rate != self.bin_ranges_sample_rate {
            self.compute_bin_ranges(sample_rate);
        }
        let power = self.spectrum.power();
        let scale = self.spectrum.amplitude_scale();
        self.bin_ranges
            .iter()
            .map(|range| {
                let peak = power[range.clone()].iter().copied().fold(0.0f32, f32::max);
                let amplitude = peak.sqrt() * scale;
                if amplitude <= 0.0 {
                    return 0.0;
                }
                let db = 20.0 * amplitude.log10();
                unit((db - self.floor_db) / (self.ceiling_db - self.floor_db))
            })
            .collect()
    }

    /// Index of the band that contains `frequency`, or None if it's outside the analyzed range.
    pub fn band_index(&self, frequency: f64, sample_rate: f64) -> Option<usize> {
        let top = self.max_frequency.min(sample_rate / 2.0);
        if frequency < self.min_frequency || frequency >= top {
            return None;
        }
        let position = (frequency / self.min_frequency).ln() / (top / self.min_frequency).ln();
        Some(((position * self.band_count as f64) as usize).min(self.band_count - 1))
    }

    fn compute_bin_ranges(&mut self, sample_rate: f64) {
        let bin_width = self.spectrum.bin_width(sample_rate);
        let top = self.max_frequency.min(sample_rate / 2.0);
        let last_bin = self.fft_size() / 2 - 1;
        let (min, count) = (self.min_frequency, self.band_count as f64);
        self.bin_ranges = (0..self.band_count)
            .map(|band| {
                let low = min * (top / min).powf(band as f64 / count);
                let high = min * (top / min).powf((band + 1) as f64 / count);
                let first = ((low / bin_width).round() as i64).max(1).min(last_bin as i64) as usize;
                let last = (((high / bin_width).round() as i64 - 1).max(first as i64) as usize).min(last_bin);
                first..=last
            })
            .collect();
        self.bin_ranges_sample_rate = sample_rate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sine(frequency: f64, sample_rate: f64) -> Vec<f32> {
        (0..2048)
            .map(|i| 0.5 * (2.0 * std::f64::consts::PI * frequency * i as f64 / sample_rate).sin() as f32)
            .collect()
    }

    fn loudest(bands: &[f32]) -> usize {
        (0..bands.len()).max_by(|&a, &b| bands[a].total_cmp(&bands[b])).unwrap()
    }

    #[test]
    fn sine_peaks_in_its_band() {
        let mut analyzer = SpectrumAnalyzer::default();
        let bands = analyzer.bands(&sine(1000.0, 44_100.0), 44_100.0);
        assert_eq!(Some(loudest(&bands)), analyzer.band_index(1000.0, 44_100.0));
        assert!(bands[loudest(&bands)] > 0.8);
    }

    #[test]
    fn silence_is_zero() {
        let mut analyzer = SpectrumAnalyzer::default();
        assert_eq!(analyzer.bands(&[0.0; 2048], 44_100.0), vec![0.0; analyzer.band_count]);
    }

    #[test]
    fn same_tone_same_band_across_sample_rates() {
        let mut analyzer = SpectrumAnalyzer::default();
        for rate in [48_000.0, 96_000.0] {
            let bands = analyzer.bands(&sine(3000.0, rate), rate);
            assert_eq!(Some(loudest(&bands)), analyzer.band_index(3000.0, 44_100.0), "at {rate} Hz");
        }
    }

    #[test]
    fn low_and_high_tones_land_at_opposite_ends() {
        let mut analyzer = SpectrumAnalyzer::default();
        let low = analyzer.bands(&sine(80.0, 44_100.0), 44_100.0);
        let high = analyzer.bands(&sine(12_000.0, 44_100.0), 44_100.0);
        assert!(loudest(&low) < 4);
        assert!(loudest(&high) > analyzer.band_count - 4);
    }

    #[test]
    fn full_scale_sine_reads_full_scale() {
        // Pins the vDSP-compatible scaling: a 0.5 sine centred in a bin should read ≈ -6 dB.
        let mut spectrum = PowerSpectrum::new(2048);
        let bin = 100.0;
        let samples: Vec<f32> =
            (0..2048).map(|i| 0.5 * (2.0 * std::f64::consts::PI * bin * i as f64 / 2048.0).sin() as f32).collect();
        spectrum.compute(&samples);
        let amplitude = spectrum.power()[100].sqrt() * spectrum.amplitude_scale();
        assert!((amplitude - 0.5).abs() < 1e-3, "amplitude {amplitude}");
    }
}

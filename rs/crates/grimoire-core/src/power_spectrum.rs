use std::f64::consts::PI;
use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

/// Hann-windowed real FFT producing squared magnitudes. Shared by the spectrum and multiband analyzers.
/// Buffers are allocated once, so `compute` can run every frame without allocating.
///
/// Scaled to match the Swift (vDSP) version exactly: vDSP's zrip output is 2× the DFT, so `power` holds
/// `(2·|X|)²`, and `amplitude_scale` (2/N) turns `sqrt(power)` into a sine's amplitude (full scale = 1).
pub struct PowerSpectrum {
    size: usize,
    /// Squared magnitudes of bins `0..size/2` from the last `compute` call.
    power: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    windowed: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl PowerSpectrum {
    pub fn new(size: usize) -> Self {
        assert!(size >= 64 && size.is_power_of_two(), "FFT size must be a power of two");
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        // vDSP's denormalized Hann: periodic, peak 1.
        let window = (0..size).map(|n| (0.5 * (1.0 - (2.0 * PI * n as f64 / size as f64).cos())) as f32).collect();
        PowerSpectrum {
            size,
            power: vec![0.0; size / 2],
            windowed: vec![0.0; size],
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn power(&self) -> &[f32] {
        &self.power
    }

    pub fn amplitude_scale(&self) -> f32 {
        2.0 / self.size as f32
    }

    pub fn bin_width(&self, sample_rate: f64) -> f64 {
        sample_rate / self.size as f64
    }

    /// Analyzes the last `size` samples of `samples`.
    pub fn compute(&mut self, samples: &[f32]) {
        assert!(samples.len() >= self.size);
        let input = &samples[samples.len() - self.size..];
        for ((w, &x), &h) in self.windowed.iter_mut().zip(input).zip(&self.window) {
            *w = x * h;
        }
        self.fft
            .process_with_scratch(&mut self.windowed, &mut self.output, &mut self.scratch)
            .expect("FFT buffers are sized by the planner");
        for (p, c) in self.power.iter_mut().zip(&self.output) {
            *p = 4.0 * c.norm_sqr();
        }
        // vDSP packs DC and Nyquist into bin 0; nothing downstream reads bin 0, but keep it comparable.
        self.power[0] = 4.0 * (self.output[0].re.powi(2) + self.output[self.size / 2].re.powi(2));
    }
}

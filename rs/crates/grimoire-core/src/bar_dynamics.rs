/// Classic analyzer motion: bars jump up to their target instantly and fall at a fixed rate;
/// peak caps hold for a moment, then fall.
#[derive(Clone, Debug)]
pub struct BarDynamics {
    pub levels: Vec<f32>,
    pub peaks: Vec<f32>,
    /// Full bar heights per second.
    pub fall_rate: f32,
    pub peak_hold: f64,
    pub peak_fall_rate: f32,
    peak_age: Vec<f64>,
}

impl BarDynamics {
    pub fn new(count: usize) -> Self {
        BarDynamics {
            levels: vec![0.0; count],
            peaks: vec![0.0; count],
            fall_rate: 1.6,
            peak_hold: 0.35,
            peak_fall_rate: 0.9,
            peak_age: vec![0.0; count],
        }
    }

    /// Advances by `dt` seconds. `targets` None means silence (everything falls).
    pub fn step(&mut self, targets: Option<&[f32]>, dt: f64) {
        for i in 0..self.levels.len() {
            let target = targets.and_then(|t| t.get(i).copied()).unwrap_or(0.0);
            self.levels[i] = target.max((self.levels[i] - self.fall_rate * dt as f32).max(0.0));
            if self.levels[i] >= self.peaks[i] {
                self.peaks[i] = self.levels[i];
                self.peak_age[i] = 0.0;
            } else {
                self.peak_age[i] += dt;
                if self.peak_age[i] > self.peak_hold {
                    self.peaks[i] = self.levels[i].max(self.peaks[i] - self.peak_fall_rate * dt as f32);
                }
            }
        }
    }

    /// True once every bar and peak is at zero, so animation can stop.
    pub fn is_settled(&self) -> bool {
        self.levels.iter().all(|&l| l == 0.0) && self.peaks.iter().all(|&p| p == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rises_instantly_and_falls_at_rate() {
        let mut bars = BarDynamics::new(1);
        bars.step(Some(&[0.9]), 0.016);
        assert_eq!(bars.levels[0], 0.9);
        bars.step(None, 0.25);
        assert!((bars.levels[0] - (0.9 - 1.6 * 0.25)).abs() < 1e-4);
    }

    #[test]
    fn peak_holds_then_falls() {
        let mut bars = BarDynamics::new(1);
        bars.step(Some(&[1.0]), 0.016);
        bars.step(None, 0.3);
        assert_eq!(bars.peaks[0], 1.0);
        bars.step(None, 0.1);
        assert!(bars.peaks[0] < 1.0);
    }

    #[test]
    fn settles_to_zero() {
        let mut bars = BarDynamics::new(3);
        bars.step(Some(&[1.0, 0.5, 0.2]), 0.016);
        assert!(!bars.is_settled());
        for _ in 0..200 {
            bars.step(None, 0.016);
        }
        assert!(bars.is_settled());
    }
}

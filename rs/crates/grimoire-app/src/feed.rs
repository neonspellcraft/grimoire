//! Reads the audio tap once per frame and turns it into what the windows animate with: the three smoothed
//! bands and hits for the scenes and the sigil glow, and spectrum bars for the main window.

use std::time::Instant;

use grimoire_audio::{PlayState, TapReader};
use grimoire_core::{BandFollower, BarDynamics, MultibandAnalyzer, SpectrumAnalyzer};

use crate::library::Library;
use crate::scenes::SceneAudio;

pub struct AudioFeed {
    reader: TapReader,
    multiband: MultibandAnalyzer,
    spectrum: SpectrumAnalyzer,
    pub follower: BandFollower,
    pub bars: BarDynamics,
    last: Option<Instant>,
}

impl Default for AudioFeed {
    fn default() -> Self {
        let spectrum = SpectrumAnalyzer::default();
        AudioFeed {
            reader: TapReader::new(2048),
            multiband: MultibandAnalyzer::default(),
            bars: BarDynamics::new(spectrum.band_count),
            spectrum,
            follower: BandFollower::default(),
            last: None,
        }
    }
}

impl AudioFeed {
    pub fn update(&mut self, library: &Library) {
        let now = Instant::now();
        let dt = self.last.map_or(1.0 / 60.0, |t| (now - t).as_secs_f64().clamp(0.0, 0.1));
        if dt < 0.004 {
            return; // called more than once this frame
        }
        self.last = Some(now);
        let playing = library.state() == PlayState::Playing;
        let fresh = library.player.as_ref().is_some_and(|p| self.reader.read(p.tap(), dt, playing));
        if fresh {
            let rate = self.reader.sample_rate;
            let levels = self.multiband.levels(&self.reader.samples, rate);
            self.follower.step(Some(levels), dt);
            let bands = self.spectrum.bands(&self.reader.samples, rate);
            self.bars.step(Some(&bands), dt);
        } else {
            self.follower.step(None, dt);
            self.bars.step(None, dt);
        }
    }

    pub fn scene_audio(&self) -> SceneAudio {
        SceneAudio { levels: self.follower.envelope, hits: self.follower.hits }
    }
}

//! Pure Grimoire logic, ported from `Sources/GrimoireCore`. No OS, audio-device or GPU dependencies,
//! so all of it is unit tested on every platform.

pub mod audio_files;
pub mod bar_dynamics;
pub mod flac_tags;
pub mod m3u;
pub mod multiband;
pub mod playlist;
pub mod power_spectrum;
pub mod spectrum;
pub mod spellrun;
pub mod three_body;
pub mod time_math;
pub mod track;

pub use bar_dynamics::BarDynamics;
pub use multiband::{BandFollower, BandLevels, MultibandAnalyzer};
pub use playlist::{Playlist, RepeatMode};
pub use power_spectrum::PowerSpectrum;
pub use spectrum::SpectrumAnalyzer;
pub use three_body::{Body, ThreeBodySystem};
pub use track::{Track, TrackId};

/// Clamps to 0...1 (Swift's `min(max(x, 0), 1)`, which also maps NaN to 0 the same way).
#[inline]
pub(crate) fn unit(x: f32) -> f32 {
    x.max(0.0).min(1.0)
}

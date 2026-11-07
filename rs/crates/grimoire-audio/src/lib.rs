//! Grimoire playback, the cross-platform replacement for the AVFoundation engine.

mod convert;
pub mod player;
pub mod source;
pub mod tap;

pub use player::{PlayState, Player, PlayerEvent};
pub use source::{read_info, FileInfo, Source};
pub use tap::{AudioTap, TapPosition, TapReader};

#[derive(Debug)]
pub enum Error {
    Open(String),
    Decode(String),
    NoDevice,
    Output(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Open(e) => write!(f, "couldn't open: {e}"),
            Error::Decode(e) => write!(f, "couldn't decode: {e}"),
            Error::NoDevice => write!(f, "no audio output device"),
            Error::Output(e) => write!(f, "audio output: {e}"),
        }
    }
}

impl std::error::Error for Error {}

//! Spellrun, a generated pixel platformer that the music plays (port of `Sources/GrimoireCore/Spellrun`).
//! Pure CPU code: the game, its level generator and a software renderer into a `PixelCanvas`.

pub mod art;
pub mod canvas;
pub mod game;
pub mod renderer;

pub use canvas::{PixelCanvas, SplitMix64};
pub use game::{LevelGenerator, SpellrunGame, SpellrunInput};
pub use renderer::SpellrunRenderer;

//! The main window: a "terminal grimoire" drawn on a character grid with a CRT finish.

pub mod font;
pub mod layout;
pub mod render;
pub mod screen;
pub mod theme;

pub use layout::{layout, Action, RowView, View};
pub use render::{Effects, UiRenderer};
pub use screen::Screen;

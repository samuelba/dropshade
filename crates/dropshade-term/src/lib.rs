//! Terminal widget for Dropshade: `alacritty_terminal` emulation rendered with iced.

pub mod colors;
pub mod frame;
pub mod input;
pub mod metrics;
pub mod size;
pub mod terminal;
pub mod view;

pub use alacritty_terminal::event::Event as TermEvent;
pub use colors::Palette;
pub use input::InputConfig;
pub use size::{CellSize, GridSize};
pub use terminal::{Notice, SpawnError, SpawnOptions, Terminal};
pub use view::{Interaction, TerminalView, ViewStyle};

/// Sets `TERM` and `COLORTERM` for child shells. Call once at startup, before spawning threads.
pub fn setup_env() {
    alacritty_terminal::tty::setup_env();
}

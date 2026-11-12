//! Terminal widget for Dropshade: `alacritty_terminal` emulation rendered with iced.

pub mod clicks;
pub mod colors;
pub mod frame;
pub mod input;
pub mod links;
mod marks;
pub mod metrics;
pub mod mouse;
mod osc133;
mod pty_loop;
mod reset;
pub mod size;
pub mod terminal;
pub mod view;

pub use alacritty_terminal;
pub use alacritty_terminal::event::Event as TermEvent;
pub use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
pub use colors::Palette;
pub use input::InputConfig;
pub use input::Shortcut;
pub use size::{CellSize, GridSize};
pub use terminal::{
    ForegroundProcess, Notice, SearchDirection, SelectionKind, SpawnError, SpawnOptions, Terminal,
};
pub use view::{
    Interaction, PointerState, ScrollPosition, TerminalView, TerminalWidget, ViewState, ViewStyle,
};

/// Sets `TERM` and `COLORTERM` for child shells. Call once at startup, before spawning threads.
pub fn setup_env() {
    alacritty_terminal::tty::setup_env();
}

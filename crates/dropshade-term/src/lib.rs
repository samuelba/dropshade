//! Terminal widget for Dropshade: `alacritty_terminal` emulation rendered with iced.

pub mod clicks;
mod cluster;
pub mod colors;
mod colr;
mod emoji;
pub mod frame;
mod graphics;
mod grid_links;
pub mod hints;
pub mod input;
pub mod links;
mod marks;
pub mod metrics;
pub mod mouse;
mod osc133;
mod pty_loop;
mod replay;
mod reset;
mod sequences;
pub mod size;
pub mod terminal;
pub mod vi;
pub mod view;
mod wide;

pub use alacritty_terminal;
pub use alacritty_terminal::event::Event as TermEvent;
pub use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
pub use colors::Palette;
pub use input::InputConfig;
pub use input::Shortcut;
pub use size::{CellSize, GridSize};
pub use terminal::{
    ForegroundProcess, LinkSpan, Notice, SearchDirection, SelectionKind, SpawnError, SpawnOptions,
    Terminal,
};
pub use vi::{ViAction, ViScroll, ViSelection};
pub use view::{
    Interaction, PointerState, ScrollPosition, TerminalView, TerminalWidget, ViewState, ViewStyle,
};

/// Sets `TERM` and `COLORTERM` for child shells. Call once at startup, before spawning threads.
pub fn setup_env() {
    alacritty_terminal::tty::setup_env();
}

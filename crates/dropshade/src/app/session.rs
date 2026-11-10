//! What a tab holds: a running terminal plus its blink and bell state.

use std::path::PathBuf;

use dropshade_term::{SpawnError, SpawnOptions, TermEvent, Terminal, ViewStyle};
use iced::futures::Stream;

#[derive(Debug)]
pub struct Session {
    pub terminal: Terminal,
    pub cursor_on: bool,
    pub bell: bool,
}

/// Starts a shell sized for `style`, optionally in `working_directory`.
pub fn spawn(
    style: &ViewStyle,
    working_directory: Option<PathBuf>,
) -> Result<(Session, impl Stream<Item = TermEvent> + Send + 'static), SpawnError> {
    let options = SpawnOptions {
        cell: style.cell,
        working_directory,
        ..SpawnOptions::default()
    };
    let (terminal, events) = Terminal::spawn(options)?;
    Ok((
        Session {
            terminal,
            cursor_on: true,
            bell: false,
        },
        events,
    ))
}

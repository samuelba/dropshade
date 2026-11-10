//! What a tab holds: a running terminal plus its blink and bell state.

use std::path::PathBuf;

use dropshade_term::{SpawnError, SpawnOptions, TermEvent, Terminal};
use iced::futures::Stream;

use crate::settings::Settings;

#[derive(Debug)]
pub struct Session {
    pub terminal: Terminal,
    pub cursor_on: bool,
    pub bell: bool,
    /// A layout's startup command, typed once the shell has drawn its first output (its prompt),
    /// so it is not echoed a second time as type-ahead.
    pub pending_command: Option<String>,
}

/// Starts a shell with the current settings, optionally in `working_directory`.
pub fn spawn(
    settings: &Settings,
    working_directory: Option<PathBuf>,
) -> Result<(Session, impl Stream<Item = TermEvent> + Send + 'static), SpawnError> {
    let options = SpawnOptions {
        command: settings.shell.clone(),
        working_directory,
        env: settings.env.clone(),
        scrollback_lines: settings.scrollback_lines,
        cursor_style: settings.cursor_style,
        cell: settings.style.cell,
        ..SpawnOptions::default()
    };
    let (terminal, events) = Terminal::spawn(options)?;
    Ok((
        Session {
            terminal,
            cursor_on: true,
            bell: false,
            pending_command: None,
        },
        events,
    ))
}

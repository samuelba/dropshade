//! What a tab holds: a running terminal plus its blink, bell and foreground-command state.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dropshade_term::{SpawnError, SpawnOptions, TermEvent, Terminal};
use iced::futures::Stream;

use super::commands::Running;
use crate::settings::Settings;
use crate::shell_integration;

/// How long a new tab's output does not count as activity, so the first prompt of tabs opened
/// together does not mark them.
const STARTUP_QUIET: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Session {
    pub terminal: Terminal,
    pub cursor_on: bool,
    pub bell: bool,
    /// A layout's startup command, typed once the shell has drawn its first output (its prompt),
    /// so it is not echoed a second time as type-ahead.
    pub pending_command: Option<String>,
    /// Output before this does not mark the tab as active.
    pub quiet_until: Instant,
    /// The command in the foreground at the last look, to notice when it finishes.
    pub running: Option<Running>,
}

/// Starts a shell with the current settings, optionally in `working_directory`.
pub fn spawn(
    settings: &Settings,
    working_directory: Option<PathBuf>,
) -> Result<(Session, impl Stream<Item = TermEvent> + Send + use<>), SpawnError> {
    let command = settings.shell.clone();
    let (command, env) = if settings.shell_integration
        && let Some(scripts) = shell_integration::scripts_dir()
    {
        let launch = shell_integration::inject(
            scripts,
            command,
            settings.env.clone(),
            std::env::var("SHELL")
                .ok()
                .filter(|shell| !shell.is_empty()),
            std::env::var("ZDOTDIR").ok(),
        );
        (launch.command, launch.env)
    } else {
        (command, settings.env.clone())
    };
    let options = SpawnOptions {
        command,
        working_directory,
        env,
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
            quiet_until: Instant::now() + STARTUP_QUIET,
            running: None,
        },
        events,
    ))
}

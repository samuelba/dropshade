//! What a tab holds: a running terminal plus its blink, bell and foreground-command state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
) -> Result<(Session, impl Stream<Item = TermEvent> + Send + 'static), SpawnError> {
    let (command, working_directory) = if in_flatpak() {
        let wrapped = host_command(
            settings.shell.clone(),
            working_directory.as_deref(),
            &settings.env,
        );
        (Some(wrapped), None)
    } else {
        (settings.shell.clone(), working_directory)
    };
    // Inside Flatpak the shell runs on the host, which is not set up from here.
    let (command, env) = if settings.shell_integration
        && !in_flatpak()
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

fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Inside Flatpak the user's shell runs on the host through `flatpak-spawn --host`, which hands
/// it the terminal; the directory and environment are passed along explicitly.
fn host_command(
    shell: Option<(String, Vec<String>)>,
    directory: Option<&Path>,
    env: &HashMap<String, String>,
) -> (String, Vec<String>) {
    let mut args: Vec<String> = [
        "--host",
        "--watch-bus",
        "--env=TERM=xterm-256color",
        "--env=COLORTERM=truecolor",
    ]
    .map(String::from)
    .to_vec();
    let mut variables: Vec<_> = env.iter().collect();
    variables.sort();
    args.extend(
        variables
            .into_iter()
            .map(|(key, value)| format!("--env={key}={value}")),
    );
    if let Some(directory) = directory {
        args.push(format!("--directory={}", directory.display()));
    }
    match shell {
        Some((program, shell_args)) => {
            args.push(program);
            args.extend(shell_args);
        }
        None => args.extend(["/bin/sh", "-c", r#"exec "${SHELL:-/bin/sh}" -l"#].map(String::from)),
    }
    ("flatpak-spawn".into(), args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_shell_runs_on_the_host_in_the_tab_directory() {
        let env = HashMap::from([("EDITOR".to_owned(), "nvim".to_owned())]);
        let (program, args) = host_command(None, Some(Path::new("/home/me/src")), &env);
        assert_eq!(program, "flatpak-spawn");
        assert_eq!(args[0], "--host");
        assert!(args.contains(&"--env=EDITOR=nvim".to_owned()));
        assert!(args.contains(&"--directory=/home/me/src".to_owned()));
        assert_eq!(
            args[args.len() - 3..],
            ["/bin/sh", "-c", r#"exec "${SHELL:-/bin/sh}" -l"#]
        );
    }

    #[test]
    fn a_configured_shell_is_used_as_is() {
        let (_, args) = host_command(
            Some(("/bin/zsh".into(), vec!["-l".into()])),
            None,
            &HashMap::new(),
        );
        assert_eq!(args[args.len() - 2..], ["/bin/zsh", "-l"]);
        assert!(!args.iter().any(|arg| arg.starts_with("--directory")));
    }
}

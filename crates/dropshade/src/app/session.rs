//! What a tab holds: a running terminal plus its blink and bell state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let options = SpawnOptions {
        command,
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

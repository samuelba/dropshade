//! Sets up bash and zsh to report their prompts and commands with OSC 133 marks.
//!
//! The scripts are part of the binary and written to the data directory, so they also work for a
//! binary copied anywhere. Bash is started with `--rcfile` pointing at its script, which loads
//! `~/.bashrc` first; zsh gets `ZDOTDIR` pointing at a `.zshenv` that puts `ZDOTDIR` back and
//! loads the user's own. Fish sends the marks itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BASH_SCRIPT: &str = include_str!("../shell-integration/bash/dropshade.bash");
const ZSH_ENV: &str = include_str!("../shell-integration/zsh/.zshenv");

const BASH_PATH: &str = "bash/dropshade.bash";
const ZSH_DIR: &str = "zsh";
const ZSH_ENV_PATH: &str = "zsh/.zshenv";
/// Where zsh finds the `ZDOTDIR` it was started with.
const ZDOTDIR_BACKUP: &str = "DROPSHADE_ZDOTDIR";

/// The directory with the scripts, written on first use; `None` when that failed.
pub fn scripts_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = directories::ProjectDirs::from("dev", "samuelba", "dropshade")?
            .data_dir()
            .join("shell-integration");
        match install(&dir) {
            Ok(()) => Some(dir),
            Err(error) => {
                tracing::warn!(%error, dir = %dir.display(), "could not write the shell integration scripts");
                None
            }
        }
    })
    .as_deref()
}

fn install(dir: &Path) -> std::io::Result<()> {
    for (path, content) in [(BASH_PATH, BASH_SCRIPT), (ZSH_ENV_PATH, ZSH_ENV)] {
        let path = dir.join(path);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == content) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Bash,
    Zsh,
    Other,
}

fn shell_kind(program: &str) -> Shell {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .trim_start_matches('-');
    match name {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        _ => Shell::Other,
    }
}

/// What the shell is started with once the integration is set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub command: Option<(String, Vec<String>)>,
    pub env: HashMap<String, String>,
}

/// Sets up the integration for `command` (or the login shell `default_shell` when it is `None`).
/// Shells given arguments are left alone, since the arguments may ask for something else
/// entirely. `inherited_zdotdir` is the `ZDOTDIR` dropshade itself was started with.
pub fn inject(
    scripts: &Path,
    command: Option<(String, Vec<String>)>,
    mut env: HashMap<String, String>,
    default_shell: Option<String>,
    inherited_zdotdir: Option<String>,
) -> Launch {
    let program = match &command {
        Some((program, args)) if args.is_empty() => Some(program.clone()),
        Some(_) => None,
        None => default_shell,
    };
    let command = match program.as_deref().map(shell_kind) {
        Some(Shell::Bash) => {
            let rcfile = scripts.join(BASH_PATH).to_string_lossy().into_owned();
            program.map(|program| (program, vec!["--rcfile".to_owned(), rcfile]))
        }
        Some(Shell::Zsh) => {
            if let Some(zdotdir) = env.get("ZDOTDIR").cloned().or(inherited_zdotdir) {
                env.insert(ZDOTDIR_BACKUP.to_owned(), zdotdir);
            }
            env.insert(
                "ZDOTDIR".to_owned(),
                scripts.join(ZSH_DIR).to_string_lossy().into_owned(),
            );
            command
        }
        Some(Shell::Other) | None => command,
    };
    Launch { command, env }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPTS: &str = "/data/shell-integration";

    fn launch(
        command: Option<(&str, &[&str])>,
        default_shell: Option<&str>,
        zdotdir: Option<&str>,
    ) -> Launch {
        let command = command.map(|(program, args)| {
            (
                program.to_owned(),
                args.iter().map(|&arg| arg.to_owned()).collect(),
            )
        });
        inject(
            Path::new(SCRIPTS),
            command,
            HashMap::new(),
            default_shell.map(str::to_owned),
            zdotdir.map(str::to_owned),
        )
    }

    #[test]
    fn bash_loads_the_script_as_its_rcfile() {
        let expected = Some((
            "/usr/bin/bash".to_owned(),
            vec![
                "--rcfile".to_owned(),
                format!("{SCRIPTS}/bash/dropshade.bash"),
            ],
        ));
        assert_eq!(launch(None, Some("/usr/bin/bash"), None).command, expected);
        assert_eq!(
            launch(Some(("/usr/bin/bash", &[])), None, None).command,
            expected
        );
    }

    #[test]
    fn zsh_starts_in_the_scripts_zdotdir_and_remembers_the_users() {
        let launched = launch(None, Some("/bin/zsh"), Some("/home/me/.config/zsh"));
        assert_eq!(launched.command, None);
        assert_eq!(launched.env["ZDOTDIR"], format!("{SCRIPTS}/zsh"));
        assert_eq!(launched.env[ZDOTDIR_BACKUP], "/home/me/.config/zsh");

        let launched = launch(None, Some("/bin/zsh"), None);
        assert!(!launched.env.contains_key(ZDOTDIR_BACKUP));
    }

    #[test]
    fn other_shells_and_shells_with_arguments_are_left_alone() {
        assert_eq!(launch(None, Some("/usr/bin/fish"), None).command, None);
        let with_args = launch(Some(("bash", &["-l"])), None, None);
        assert_eq!(
            with_args.command,
            Some(("bash".to_owned(), vec!["-l".to_owned()]))
        );
        assert!(with_args.env.is_empty());
        assert_eq!(launch(None, None, None).command, None);
    }

    #[test]
    fn the_scripts_are_written_once() {
        let dir = std::env::temp_dir().join(format!("dropshade-si-{}", std::process::id()));
        install(&dir).expect("install");
        assert_eq!(
            std::fs::read_to_string(dir.join(BASH_PATH)).unwrap(),
            BASH_SCRIPT
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(ZSH_ENV_PATH)).unwrap(),
            ZSH_ENV
        );
        install(&dir).expect("install again");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

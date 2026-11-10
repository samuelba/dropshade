//! Saved tab sets: the automatic session (restored on the next start) and named layouts the user
//! keeps in the config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u32 = 1;

/// One tab as saved: its custom name, directory and an optional command typed on start.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TabSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Typed into the shell once it starts (layouts only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    /// Index of the tab that was active.
    pub active: usize,
    #[serde(rename = "tab")]
    pub tabs: Vec<TabSnapshot>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            active: 0,
            tabs: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("{path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("layout name \"{0}\" may only use letters, digits, '-', '_' and '.'")]
    InvalidName(String),
    #[error("no layout named \"{0}\"")]
    UnknownLayout(String),
}

impl Snapshot {
    /// The active index, clamped to the tabs that exist.
    pub fn active_index(&self) -> usize {
        self.active.min(self.tabs.len().saturating_sub(1))
    }
}

/// Reads a snapshot; a missing file is `Ok(None)`.
///
/// # Errors
///
/// Fails when the file cannot be read or is not a valid snapshot.
pub fn load(path: &Path) -> Result<Option<Snapshot>, SessionError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SessionError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    toml::from_str(&text)
        .map(Some)
        .map_err(|error| SessionError::Invalid {
            path: path.to_path_buf(),
            message: error.message().trim().to_owned(),
        })
}

/// Writes a snapshot atomically, so a crash mid-write never leaves a truncated file.
///
/// # Errors
///
/// Fails when the directory or file cannot be written.
pub fn save(path: &Path, snapshot: &Snapshot) -> Result<(), SessionError> {
    let io_error = |source| SessionError::Io {
        path: path.to_path_buf(),
        source,
    };
    let text = toml::to_string_pretty(snapshot).map_err(|error| SessionError::Invalid {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, text).map_err(io_error)?;
    std::fs::rename(&temporary, path).map_err(io_error)
}

/// `~/.local/state/dropshade/session.toml`.
pub fn session_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("io.github", "samuelba", "dropshade")
        .and_then(|dirs| dirs.state_dir().map(|dir| dir.join("session.toml")))
}

/// `~/.config/dropshade/layouts`.
pub fn layouts_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io.github", "samuelba", "dropshade")
        .map(|dirs| dirs.config_dir().join("layouts"))
}

/// The file for a named layout inside `dir`.
///
/// # Errors
///
/// Fails for names that are empty or could leave the layouts directory.
pub fn layout_path(dir: &Path, name: &str) -> Result<PathBuf, SessionError> {
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(dir.join(format!("{name}.toml")))
    } else {
        Err(SessionError::InvalidName(name.to_owned()))
    }
}

/// Names of the layouts in `dir`, sorted.
pub fn list_layouts(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

/// Expands a leading `~` in a saved directory.
pub fn expand_home(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    std::env::var_os("HOME")
        .map_or_else(|| path.to_path_buf(), |home| PathBuf::from(home).join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dropshade-session-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn sample() -> Snapshot {
        Snapshot {
            active: 1,
            tabs: vec![
                TabSnapshot {
                    name: Some("build".into()),
                    cwd: Some("/tmp".into()),
                    command: None,
                },
                TabSnapshot {
                    name: None,
                    cwd: Some("/home".into()),
                    command: Some("htop".into()),
                },
            ],
            ..Snapshot::default()
        }
    }

    #[test]
    fn snapshots_round_trip() {
        let dir = temp_dir("round-trip");
        let path = dir.join("nested/session.toml");
        save(&path, &sample()).expect("save");
        assert_eq!(load(&path).expect("load"), Some(sample()));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn saving_leaves_no_temporary_files() {
        let dir = temp_dir("atomic");
        let path = dir.join("session.toml");
        save(&path, &sample()).expect("save");
        save(&path, &Snapshot::default()).expect("overwrite");
        let files: Vec<_> = std::fs::read_dir(&dir)
            .expect("list")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(load(&path).expect("load"), Some(Snapshot::default()));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        assert_eq!(
            load(Path::new("/nonexistent/dropshade/session.toml")).expect("ok"),
            None
        );
    }

    #[test]
    fn malformed_files_are_reported_with_their_path() {
        let dir = temp_dir("malformed");
        let path = dir.join("broken.toml");
        std::fs::write(&path, "[[tab]]\nnmae = \"x\"\n").expect("write");
        let error = load(&path).expect_err("unknown key");
        assert!(error.to_string().contains("broken.toml"), "{error}");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn hand_written_layouts_parse() {
        let layout: Snapshot = toml::from_str(
            "[[tab]]\nname = \"api\"\ncwd = \"~/projects/api\"\ncommand = \"cargo run\"\n\n[[tab]]\nname = \"logs\"\n",
        )
        .expect("valid layout");
        assert_eq!(layout.tabs.len(), 2);
        assert_eq!(layout.active_index(), 0);
        assert_eq!(layout.tabs[0].command.as_deref(), Some("cargo run"));
    }

    #[test]
    fn active_index_is_clamped() {
        let snapshot = Snapshot {
            active: 9,
            ..sample()
        };
        assert_eq!(snapshot.active_index(), 1);
        assert_eq!(Snapshot::default().active_index(), 0);
    }

    #[test]
    fn layout_names_cannot_escape_the_directory() {
        let dir = Path::new("/layouts");
        assert_eq!(
            layout_path(dir, "work").expect("valid"),
            dir.join("work.toml")
        );
        assert_eq!(
            layout_path(dir, "my-setup_2.v1").expect("valid"),
            dir.join("my-setup_2.v1.toml")
        );
        for bad in ["", "../x", "a/b", "..", ".hidden", "with space"] {
            assert!(
                matches!(layout_path(dir, bad), Err(SessionError::InvalidName(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn layouts_are_listed_by_name() {
        let dir = temp_dir("list");
        for name in ["b.toml", "a.toml", "notes.txt"] {
            std::fs::write(dir.join(name), "").expect("write");
        }
        assert_eq!(list_layouts(&dir), vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(list_layouts(&dir.join("missing")), Vec::<String>::new());
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn home_is_expanded() {
        let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
        assert_eq!(expand_home(Path::new("~/x")), home.join("x"));
        assert_eq!(expand_home(Path::new("/abs")), PathBuf::from("/abs"));
    }
}

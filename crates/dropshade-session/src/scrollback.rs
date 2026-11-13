//! The text each pane showed when Dropshade quit, kept beside the session for the next start.
//!
//! One file per pane, `<tab>-<pane>.ansi`, numbered in snapshot order; an unsplit tab is pane 0.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::{PaneSnapshot, SessionError, Snapshot, TabSnapshot};

/// Larger files are cut off when read back.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// `~/.local/state/dropshade/scrollback`.
pub fn scrollback_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .and_then(|dirs| dirs.state_dir().map(|dir| dir.join("scrollback")))
}

/// Replaces the files in `dir` with the scrollback of `snapshot`'s panes, readable only by the
/// user.
///
/// # Errors
///
/// Fails when the old files cannot be removed or a new one cannot be written.
pub fn save_scrollback(dir: &Path, snapshot: &Snapshot) -> Result<(), SessionError> {
    clear_scrollback(dir)?;
    let io_error = |source| SessionError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let saved = snapshot.tabs.iter().enumerate().flat_map(|(tab, saved)| {
        panes(saved).enumerate().filter_map(move |(pane, bytes)| {
            Some((tab, pane, bytes.filter(|bytes| !bytes.is_empty())?))
        })
    });
    for (tab, pane, bytes) in saved {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io_error)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(file_name(tab, pane)))
            .and_then(|mut file| file.write_all(bytes))
            .map_err(io_error)?;
    }
    Ok(())
}

/// `snapshot` with the scrollback saved in `dir` filled in; files that cannot be read are left
/// out.
pub fn with_scrollback(dir: &Path, snapshot: Snapshot) -> Snapshot {
    let read = |tab: usize, pane: usize| {
        let mut bytes = Vec::new();
        File::open(dir.join(file_name(tab, pane)))
            .and_then(|file| file.take(MAX_BYTES).read_to_end(&mut bytes))
            .ok()
            .filter(|&read| read > 0)
            .map(|_| bytes)
    };
    let tabs = snapshot
        .tabs
        .into_iter()
        .enumerate()
        .map(|(index, tab)| {
            if tab.panes.is_empty() {
                return TabSnapshot {
                    scrollback: read(index, 0),
                    ..tab
                };
            }
            let panes = tab
                .panes
                .into_iter()
                .enumerate()
                .map(|(pane, saved)| PaneSnapshot {
                    scrollback: read(index, pane),
                    ..saved
                })
                .collect();
            TabSnapshot { panes, ..tab }
        })
        .collect();
    Snapshot { tabs, ..snapshot }
}

/// Removes the saved scrollback; nothing to remove is fine.
///
/// # Errors
///
/// Fails when `dir` exists but cannot be removed.
pub fn clear_scrollback(dir: &Path) -> Result<(), SessionError> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(SessionError::Io {
            path: dir.to_path_buf(),
            source: error,
        }),
        _ => Ok(()),
    }
}

impl Snapshot {
    /// The same tabs with no scrollback attached.
    #[must_use]
    pub fn without_scrollback(self) -> Self {
        let tabs = self
            .tabs
            .into_iter()
            .map(|tab| TabSnapshot {
                scrollback: None,
                panes: tab
                    .panes
                    .into_iter()
                    .map(|pane| PaneSnapshot {
                        scrollback: None,
                        ..pane
                    })
                    .collect(),
                ..tab
            })
            .collect();
        Self { tabs, ..self }
    }
}

fn panes(tab: &TabSnapshot) -> Box<dyn Iterator<Item = Option<&[u8]>> + '_> {
    if tab.panes.is_empty() {
        Box::new(std::iter::once(tab.scrollback.as_deref()))
    } else {
        Box::new(tab.panes.iter().map(|pane| pane.scrollback.as_deref()))
    }
}

fn file_name(tab: usize, pane: usize) -> String {
    format!("{tab}-{pane}.ansi")
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dropshade-scrollback-{name}-{}",
            std::process::id()
        ))
    }

    fn sample() -> Snapshot {
        Snapshot {
            tabs: vec![
                TabSnapshot {
                    scrollback: Some(b"one".to_vec()),
                    ..TabSnapshot::default()
                },
                TabSnapshot {
                    panes: vec![
                        PaneSnapshot::default(),
                        PaneSnapshot {
                            scrollback: Some(b"two".to_vec()),
                            ..PaneSnapshot::default()
                        },
                    ],
                    ..TabSnapshot::default()
                },
            ],
            ..Snapshot::default()
        }
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn round_trips_through_private_files() {
        let dir = temp_dir("round-trip");
        save_scrollback(&dir, &sample()).expect("save");
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("0-0.ansi")), 0o600);
        assert!(!dir.join("1-0.ansi").exists());
        assert_eq!(
            with_scrollback(&dir, sample().without_scrollback()),
            sample()
        );
        clear_scrollback(&dir).expect("clear");
    }

    #[test]
    fn saving_replaces_the_old_files() {
        let dir = temp_dir("replace");
        save_scrollback(&dir, &sample()).expect("save");
        let fewer = Snapshot {
            tabs: sample().tabs.into_iter().take(1).collect(),
            ..Snapshot::default()
        };
        save_scrollback(&dir, &fewer).expect("save again");
        assert!(dir.join("0-0.ansi").exists());
        assert!(!dir.join("1-1.ansi").exists());
        clear_scrollback(&dir).expect("clear");
    }

    #[test]
    fn clearing_twice_is_fine() {
        let dir = temp_dir("clear");
        save_scrollback(&dir, &sample()).expect("save");
        clear_scrollback(&dir).expect("clear");
        clear_scrollback(&dir).expect("clear again");
        assert!(!dir.exists());
    }

    #[test]
    fn scrollback_is_never_written_to_the_session_file() {
        let text = toml::to_string_pretty(&sample()).expect("serialize");
        assert!(!text.contains("scrollback"));
    }
}

//! Saved tab sets: the automatic session (restored on the next start) and named layouts the user
//! keeps in the config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod scrollback;

pub use scrollback::{clear_scrollback, save_scrollback, scrollback_dir, with_scrollback};

const FORMAT_VERSION: u32 = 1;

/// One tab as saved: its custom name, directory and an optional command typed on start. A split
/// tab lists its panes instead; its `cwd` is then the default for panes without one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TabSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Typed into the shell once it starts (layouts only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The panes of a split tab, in the order they are made: each one after the first splits
    /// an earlier one.
    #[serde(rename = "pane", skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<PaneSnapshot>,
    /// What an unsplit tab showed when Dropshade quit; kept in its own file, never in the TOML.
    #[serde(skip)]
    pub scrollback: Option<Vec<u8>>,
}

/// Where a new pane goes, next to the pane it splits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Right,
    Down,
}

/// One pane of a split tab.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaneSnapshot {
    /// Which side of the pane it splits this pane goes; the first pane has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitDirection>,
    /// The pane it splits, counting from 1; the previous pane when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<usize>,
    /// Its share of the pane it splits, from 0.05 to 0.95; half when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Typed into the shell once it starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The pane that has the keyboard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
    /// What the pane showed when Dropshade quit; kept in its own file, never in the TOML.
    #[serde(skip)]
    pub scrollback: Option<Vec<u8>>,
}

/// A tab's panes as a tree of splits; leaves are indexes into the tab's pane list.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneTree {
    Pane(usize),
    Split {
        direction: SplitDirection,
        /// The share of the first part.
        ratio: f32,
        first: Box<PaneTree>,
        second: Box<PaneTree>,
    },
}

const MIN_SIZE: f64 = 0.05;
const MAX_SIZE: f64 = 0.95;
const DEFAULT_SIZE: f64 = 0.5;

impl PaneTree {
    /// The pane in the top-left corner.
    fn first_pane(&self) -> usize {
        match self {
            Self::Pane(index) => *index,
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    /// Splits the leaf `target`, putting `new` after it.
    fn split_pane(&mut self, target: usize, direction: SplitDirection, ratio: f32, new: usize) {
        match self {
            Self::Pane(index) if *index == target => {
                *self = Self::Split {
                    direction,
                    ratio,
                    first: Box::new(Self::Pane(target)),
                    second: Box::new(Self::Pane(new)),
                };
            }
            Self::Pane(_) => {}
            Self::Split { first, second, .. } => {
                first.split_pane(target, direction, ratio, new);
                second.split_pane(target, direction, ratio, new);
            }
        }
    }
}

impl TabSnapshot {
    /// The tree the pane list describes; a tab without panes is a single one.
    ///
    /// # Errors
    ///
    /// Fails with a message naming the pane when a pane has no `split`, splits a pane that does
    /// not come before it, or has a `size` out of range.
    pub fn pane_tree(&self) -> Result<PaneTree, String> {
        let mut tree = PaneTree::Pane(0);
        let Some((first, rest)) = self.panes.split_first() else {
            return Ok(tree);
        };
        if first.split.is_some() || first.of.is_some() || first.size.is_some() {
            return Err("pane 1 is the first pane, it has no other pane to split".into());
        }
        if self.command.is_some() {
            return Err("with [[tab.pane]] the command belongs in a pane".into());
        }
        for (offset, pane) in rest.iter().enumerate() {
            let index = offset + 1;
            let number = index + 1;
            let direction = pane.split.ok_or_else(|| {
                format!("pane {number} needs split = \"right\" or split = \"down\"")
            })?;
            let of = pane.of.unwrap_or(index);
            if of == 0 || of >= number {
                return Err(format!(
                    "pane {number} has of = {of}, but can only split pane 1 to {index}"
                ));
            }
            let size = pane.size.unwrap_or(DEFAULT_SIZE);
            if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
                return Err(format!(
                    "pane {number} has size = {size}, but it must be between {MIN_SIZE} and {MAX_SIZE}"
                ));
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a share between 0.05 and 0.95 fits an f32"
            )]
            let ratio = (1.0 - size) as f32;
            tree.split_pane(of - 1, direction, ratio, index);
        }
        Ok(tree)
    }
}

/// The pane list that rebuilds `tree`, pane by pane. `leaves[i]` holds the directory, command
/// and focus of leaf `i`; its split fields are filled in here.
pub fn panes_from_tree(tree: &PaneTree, leaves: &[PaneSnapshot]) -> Vec<PaneSnapshot> {
    let leaf = |index: usize| PaneSnapshot {
        split: None,
        of: None,
        size: None,
        ..leaves.get(index).cloned().unwrap_or_default()
    };
    let first = tree.first_pane();
    let mut panes = vec![leaf(first)];
    // Where each leaf ended up in the list, counting from 1.
    let mut numbers = vec![0; leaves.len().max(first + 1)];
    if let Some(number) = numbers.get_mut(first) {
        *number = 1;
    }
    add_splits(tree, &leaf, &mut panes, &mut numbers);
    panes
}

/// Adds the panes of the splits in `tree` in the order that rebuilds it: a split turns the first
/// pane of its first part into that pane plus the first pane of its second part.
fn add_splits(
    tree: &PaneTree,
    leaf: &impl Fn(usize) -> PaneSnapshot,
    panes: &mut Vec<PaneSnapshot>,
    numbers: &mut Vec<usize>,
) {
    let PaneTree::Split {
        direction,
        ratio,
        first,
        second,
    } = tree
    else {
        return;
    };
    let target = numbers.get(first.first_pane()).copied().unwrap_or(1);
    let new = second.first_pane();
    // Rounded, so dragging a divider by a hair does not rewrite the session.
    let size = ((1.0 - f64::from(*ratio)) * 100.0).round() / 100.0;
    panes.push(PaneSnapshot {
        split: Some(*direction),
        of: (target != panes.len()).then_some(target),
        size: ((size - DEFAULT_SIZE).abs() > 1e-9).then_some(size.clamp(MIN_SIZE, MAX_SIZE)),
        ..leaf(new)
    });
    if numbers.len() <= new {
        numbers.resize(new + 1, 0);
    }
    numbers[new] = panes.len();
    add_splits(first, leaf, panes, numbers);
    add_splits(second, leaf, panes, numbers);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    let invalid = |message: String| SessionError::Invalid {
        path: path.to_path_buf(),
        message,
    };
    let snapshot: Snapshot =
        toml::from_str(&text).map_err(|error| invalid(error.message().trim().to_owned()))?;
    for (index, tab) in snapshot.tabs.iter().enumerate() {
        tab.pane_tree()
            .map_err(|message| invalid(format!("tab {}: {message}", index + 1)))?;
    }
    Ok(Some(snapshot))
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
    directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .and_then(|dirs| dirs.state_dir().map(|dir| dir.join("session.toml")))
}

/// `~/.config/dropshade/layouts`.
pub fn layouts_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "samuelba", "dropshade")
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
                    ..TabSnapshot::default()
                },
                TabSnapshot {
                    name: None,
                    cwd: Some("/home".into()),
                    command: Some("htop".into()),
                    ..TabSnapshot::default()
                },
                TabSnapshot {
                    name: Some("split".into()),
                    panes: vec![
                        pane(None, "/a"),
                        PaneSnapshot {
                            size: Some(0.3),
                            focus: true,
                            ..pane(Some(SplitDirection::Right), "/b")
                        },
                    ],
                    ..TabSnapshot::default()
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
        assert_eq!(snapshot.active_index(), 2);
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

    fn pane(split: Option<SplitDirection>, cwd: &str) -> PaneSnapshot {
        PaneSnapshot {
            split,
            cwd: Some(cwd.into()),
            ..PaneSnapshot::default()
        }
    }

    #[test]
    fn scrollback_follows_its_pane_into_the_list() {
        let leaves = [
            pane(None, "/a"),
            PaneSnapshot {
                scrollback: Some(b"b".to_vec()),
                ..pane(None, "/b")
            },
        ];
        let tree = PaneTree::Split {
            direction: SplitDirection::Right,
            ratio: 0.5,
            first: Box::new(PaneTree::Pane(1)),
            second: Box::new(PaneTree::Pane(0)),
        };
        let list = panes_from_tree(&tree, &leaves);
        assert_eq!(list[0].cwd.as_deref(), Some(Path::new("/b")));
        assert_eq!(list[0].scrollback.as_deref(), Some(&b"b"[..]));
        assert_eq!(list[1].scrollback, None);
    }

    fn tab_with(panes: Vec<PaneSnapshot>) -> TabSnapshot {
        TabSnapshot {
            panes,
            ..TabSnapshot::default()
        }
    }

    /// The tree written out with each leaf's directory, to compare trees whose leaves are
    /// numbered differently.
    fn shape(tree: &PaneTree, panes: &[PaneSnapshot]) -> String {
        match tree {
            PaneTree::Pane(index) => panes[*index]
                .cwd
                .as_deref()
                .map(|cwd| cwd.display().to_string())
                .unwrap_or_default(),
            PaneTree::Split {
                direction,
                ratio,
                first,
                second,
            } => format!(
                "({} {direction:?} {ratio:.2} {})",
                shape(first, panes),
                shape(second, panes)
            ),
        }
    }

    #[test]
    fn hand_written_split_layouts_parse() {
        let layout: Snapshot = toml::from_str(
            r#"
            [[tab]]
            name = "api"
            cwd = "~/api"

            [[tab.pane]]
            command = "nvim"

            [[tab.pane]]
            split = "right"
            size = 0.35
            command = "cargo watch"

            [[tab.pane]]
            split = "down"
            of = 1
            focus = true
            "#,
        )
        .expect("valid layout");
        let tab = &layout.tabs[0];
        assert_eq!(tab.panes.len(), 3);
        assert_eq!(tab.panes[1].split, Some(SplitDirection::Right));
        assert!(tab.panes[2].focus);
        let tree = tab.pane_tree().expect("tree");
        let expected = PaneTree::Split {
            direction: SplitDirection::Right,
            ratio: 0.65,
            first: Box::new(PaneTree::Split {
                direction: SplitDirection::Down,
                ratio: 0.5,
                first: Box::new(PaneTree::Pane(0)),
                second: Box::new(PaneTree::Pane(2)),
            }),
            second: Box::new(PaneTree::Pane(1)),
        };
        assert_eq!(tree, expected);
    }

    #[test]
    fn a_tab_without_panes_is_one_pane() {
        assert_eq!(TabSnapshot::default().pane_tree(), Ok(PaneTree::Pane(0)));
    }

    #[test]
    fn pane_lists_round_trip_through_trees() {
        let panes = vec![
            pane(None, "/a"),
            PaneSnapshot {
                size: Some(0.3),
                ..pane(Some(SplitDirection::Right), "/b")
            },
            PaneSnapshot {
                of: Some(1),
                command: Some("make".into()),
                ..pane(Some(SplitDirection::Down), "/c")
            },
            PaneSnapshot {
                focus: true,
                ..pane(Some(SplitDirection::Down), "/d")
            },
            PaneSnapshot {
                of: Some(2),
                size: Some(0.25),
                ..pane(Some(SplitDirection::Down), "/e")
            },
        ];
        let tree = tab_with(panes.clone()).pane_tree().expect("tree");
        assert_eq!(panes_from_tree(&tree, &panes), panes);
    }

    #[test]
    fn any_tree_becomes_a_list_that_rebuilds_it() {
        // Leaves numbered out of order, as the app's do.
        let leaves: Vec<PaneSnapshot> = ["/x", "/y", "/z", "/w"]
            .iter()
            .map(|cwd| pane(None, cwd))
            .collect();
        let tree = PaneTree::Split {
            direction: SplitDirection::Down,
            ratio: 0.7,
            first: Box::new(PaneTree::Split {
                direction: SplitDirection::Right,
                ratio: 0.4,
                first: Box::new(PaneTree::Pane(2)),
                second: Box::new(PaneTree::Pane(0)),
            }),
            second: Box::new(PaneTree::Split {
                direction: SplitDirection::Right,
                ratio: 0.5,
                first: Box::new(PaneTree::Pane(3)),
                second: Box::new(PaneTree::Pane(1)),
            }),
        };
        let list = panes_from_tree(&tree, &leaves);
        assert_eq!(list[0].cwd.as_deref(), Some(Path::new("/z")));
        let rebuilt = tab_with(list.clone()).pane_tree().expect("valid list");
        assert_eq!(shape(&rebuilt, &list), shape(&tree, &leaves));
    }

    #[test]
    fn ratios_are_rounded_so_saves_stay_stable() {
        let tree = PaneTree::Split {
            direction: SplitDirection::Right,
            ratio: 0.333_333,
            first: Box::new(PaneTree::Pane(0)),
            second: Box::new(PaneTree::Pane(1)),
        };
        let list = panes_from_tree(&tree, &[pane(None, "/a"), pane(None, "/b")]);
        assert_eq!(list[1].size, Some(0.67));
        assert_eq!(list[1].of, None, "the previous pane is implied");
    }

    #[test]
    fn invalid_pane_lists_are_explained() {
        let error = |panes: Vec<PaneSnapshot>| tab_with(panes).pane_tree().expect_err("invalid");
        assert!(error(vec![pane(None, "/a"), pane(None, "/b")]).contains("pane 2 needs split"));
        let splits_itself = PaneSnapshot {
            of: Some(2),
            ..pane(Some(SplitDirection::Right), "/b")
        };
        assert!(error(vec![pane(None, "/a"), splits_itself]).contains("of = 2"));
        let too_big = PaneSnapshot {
            size: Some(1.5),
            ..pane(Some(SplitDirection::Right), "/b")
        };
        assert!(error(vec![pane(None, "/a"), too_big]).contains("size"));
        assert!(error(vec![pane(Some(SplitDirection::Down), "/a")]).contains("pane 1"));
        let tab_command = TabSnapshot {
            command: Some("htop".into()),
            ..tab_with(vec![pane(None, "/a")])
        };
        assert!(
            tab_command
                .pane_tree()
                .expect_err("invalid")
                .contains("command")
        );
    }

    #[test]
    fn invalid_pane_lists_in_files_are_reported_with_their_path() {
        let dir = temp_dir("bad-panes");
        let path = dir.join("bad.toml");
        std::fs::write(
            &path,
            "[[tab]]\n[[tab.pane]]\n[[tab.pane]]\nsplit = \"right\"\nof = 5\n",
        )
        .expect("write");
        let error = load(&path).expect_err("of out of range").to_string();
        assert!(
            error.contains("bad.toml") && error.contains("tab 1"),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn home_is_expanded() {
        let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
        assert_eq!(expand_home(Path::new("~/x")), home.join("x"));
        assert_eq!(expand_home(Path::new("/abs")), PathBuf::from("/abs"));
    }
}

//! Saving and restoring tabs: the automatic session and named layouts.

use std::path::{Path, PathBuf};

use dropshade_session::{
    self as sessions, PaneSnapshot, PaneTree, Snapshot, SplitDirection, TabSnapshot,
};
use iced::Task;
use iced::widget::pane_grid::{Axis, Node};

use super::session::Session;
use super::split::sync_title;
use super::{App, Message, PaneKey};
use crate::tabs::{self, Panes};

/// What the first tabs of a new instance are.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Startup {
    /// The configured layout, else the last session, else one tab.
    #[default]
    Default,
    /// One tab in this directory (`dropshade new-tab --cwd` with nothing running).
    Directory(PathBuf),
    /// A named layout (`dropshade layout load` with nothing running).
    Layout(String),
}

impl App {
    pub(super) fn startup_tabs(&mut self, startup: Startup) -> Task<Message> {
        let snapshot = match startup {
            Startup::Directory(directory) => return self.open_tab(Some(directory)),
            Startup::Layout(name) => self.read_layout(&name),
            Startup::Default => {
                let layout = self.config.session.layout.trim().to_owned();
                if layout.is_empty() {
                    self.read_session()
                } else {
                    self.read_layout(&layout)
                }
            }
        };
        let restored = snapshot.filter(|snapshot| !snapshot.tabs.is_empty());
        match restored {
            Some(snapshot) => {
                self.saved = Some(snapshot.clone());
                self.open_snapshot(&snapshot)
            }
            None => self.open_tab(self.settings.working_directory.clone()),
        }
    }

    /// Writes the session file if the tabs changed since the last save.
    pub(super) fn save_session(&mut self) {
        if !self.config.session.restore {
            return;
        }
        let Some(path) = self.session_path.as_deref() else {
            return;
        };
        let snapshot = self.snapshot();
        if self.saved.as_ref() == Some(&snapshot) {
            return;
        }
        match sessions::save(path, &snapshot) {
            Ok(()) => self.saved = Some(snapshot),
            Err(error) => tracing::warn!(%error, "could not save the session"),
        }
    }

    pub(super) fn save_layout(&mut self, name: &str) {
        let result = sessions::layouts_dir()
            .ok_or_else(|| "no config directory (HOME is not set)".to_owned())
            .and_then(|dir| sessions::layout_path(&dir, name).map_err(|error| error.to_string()))
            .and_then(|path| {
                sessions::save(&path, &self.snapshot()).map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            tracing::warn!(%error, "could not save the layout");
            self.settings
                .warnings
                .push(format!("layout \"{name}\": {error}"));
        }
    }

    pub(super) fn load_layout(&mut self, name: &str) -> Task<Message> {
        match self.read_layout(name) {
            Some(snapshot) => self.open_snapshot(&snapshot),
            None => Task::none(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        let active = self
            .tabs
            .active_id()
            .and_then(|id| self.tabs.index_of(id))
            .unwrap_or(0);
        let tabs = self
            .tabs
            .iter()
            .map(|tab| {
                let mut saved = TabSnapshot {
                    name: tab.custom_name.clone(),
                    ..TabSnapshot::default()
                };
                if tab.content.is_split() {
                    saved.panes = pane_list(&tab.content);
                } else {
                    saved.cwd = tab
                        .content
                        .focused_content()
                        .and_then(|session| session.terminal.working_directory());
                }
                saved
            })
            .collect();
        Snapshot {
            active,
            tabs,
            ..Snapshot::default()
        }
    }

    /// Opens the tabs of a snapshot after the existing ones and activates its active tab.
    fn open_snapshot(&mut self, snapshot: &Snapshot) -> Task<Message> {
        let first = self.tabs.len();
        if let Some(last) = first.checked_sub(1) {
            self.tabs.activate_index(last);
        }
        let tasks: Vec<Task<Message>> = snapshot
            .tabs
            .iter()
            .map(|tab| self.open_saved_tab(tab))
            .collect();
        self.tabs.activate_index(first + snapshot.active_index());
        Task::batch(tasks)
    }

    fn open_saved_tab(&mut self, saved: &TabSnapshot) -> Task<Message> {
        let first = saved.panes.first();
        let directory = first
            .and_then(|pane| pane.cwd.as_deref())
            .or(saved.cwd.as_deref());
        let command = first.map_or(&saved.command, |pane| &pane.command);
        let count = self.tabs.len();
        let task = self.open_tab(existing_directory(directory));
        if self.tabs.len() == count {
            return task;
        }
        let Some(id) = self.tabs.active_id() else {
            return task;
        };
        if let Some(name) = &saved.name {
            self.tabs.rename(id, name);
        }
        if let Some(session) = self
            .tabs
            .get_mut(id)
            .and_then(|tab| tab.content.focused_content_mut())
        {
            session.pending_command.clone_from(command);
        }
        let splits = self.open_saved_splits(id, saved);
        Task::batch([task, splits])
    }

    /// Splits a restored tab's first pane into the rest of its saved panes.
    fn open_saved_splits(&mut self, id: tabs::TabId, saved: &TabSnapshot) -> Task<Message> {
        let Some(first) = self.tabs.get(id).map(|tab| tab.content.focused()) else {
            return Task::none();
        };
        // The panes made so far, in list order.
        let mut made = vec![first];
        let mut tasks = Vec::new();
        for (index, pane) in saved.panes.iter().enumerate().skip(1) {
            let target = pane
                .of
                .unwrap_or(index)
                .checked_sub(1)
                .and_then(|of| made.get(of).copied());
            let (Some(target), Some(direction)) = (target, pane.split) else {
                break;
            };
            let directory = pane.cwd.as_deref().or(saved.cwd.as_deref());
            let Some((mut session, events)) = self.spawn_pane(existing_directory(directory)) else {
                break;
            };
            session.pending_command.clone_from(&pane.command);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a share between 0.05 and 0.95 fits an f32"
            )]
            let ratio = (1.0 - pane.size.unwrap_or(0.5)) as f32;
            let Some(new) = self.tabs.get_mut(id).and_then(|tab| {
                tab.content
                    .split_pane(target, split_direction(direction), ratio, session)
            }) else {
                break;
            };
            made.push(new);
            let key = PaneKey { tab: id, pane: new };
            tasks.push(Task::run(events, move |event| Message::Term(key, event)));
        }
        let focused = saved
            .panes
            .iter()
            .position(|pane| pane.focus)
            .and_then(|index| made.get(index).copied());
        if let Some(tab) = self.tabs.get_mut(id) {
            if let Some(pane) = focused {
                tab.content.focus(pane);
            }
            sync_title(tab);
        }
        Task::batch(tasks)
    }

    fn read_session(&self) -> Option<Snapshot> {
        if !self.config.session.restore {
            return None;
        }
        let path = self.session_path.as_deref()?;
        sessions::load(path).unwrap_or_else(|error| {
            tracing::warn!(%error, "ignoring the saved session");
            None
        })
    }

    fn read_layout(&mut self, name: &str) -> Option<Snapshot> {
        let loaded = sessions::layouts_dir()
            .ok_or_else(|| "no config directory (HOME is not set)".to_owned())
            .and_then(|dir| sessions::layout_path(&dir, name).map_err(|error| error.to_string()))
            .and_then(|path| match sessions::load(&path) {
                Ok(Some(snapshot)) => Ok(snapshot),
                Ok(None) => Err(sessions::SessionError::UnknownLayout(name.to_owned()).to_string()),
                Err(error) => Err(error.to_string()),
            });
        match loaded {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::warn!(%error, "could not load the layout");
                self.settings
                    .warnings
                    .push(format!("layout \"{name}\": {error}"));
                None
            }
        }
    }
}

/// A saved directory that still exists, with `~` expanded.
fn existing_directory(directory: Option<&Path>) -> Option<PathBuf> {
    directory
        .map(sessions::expand_home)
        .filter(|dir| dir.is_dir())
}

fn split_direction(direction: SplitDirection) -> tabs::SplitDirection {
    match direction {
        SplitDirection::Right => tabs::SplitDirection::Right,
        SplitDirection::Down => tabs::SplitDirection::Down,
    }
}

/// A split tab's panes as a saved list: each pane's directory, which one has the keyboard, and
/// the splits that rebuild the layout.
fn pane_list(panes: &Panes<Session>) -> Vec<PaneSnapshot> {
    let mut leaves = Vec::new();
    let tree = pane_tree(panes.layout(), panes, &mut leaves);
    sessions::panes_from_tree(&tree, &leaves)
}

fn pane_tree(node: &Node, panes: &Panes<Session>, leaves: &mut Vec<PaneSnapshot>) -> PaneTree {
    match node {
        Node::Pane(pane) => {
            leaves.push(PaneSnapshot {
                cwd: panes
                    .get(*pane)
                    .and_then(|session| session.terminal.working_directory()),
                focus: *pane == panes.focused(),
                ..PaneSnapshot::default()
            });
            PaneTree::Pane(leaves.len() - 1)
        }
        Node::Split {
            axis, ratio, a, b, ..
        } => PaneTree::Split {
            direction: match axis {
                Axis::Vertical => SplitDirection::Right,
                Axis::Horizontal => SplitDirection::Down,
            },
            ratio: *ratio,
            first: Box::new(pane_tree(a, panes, leaves)),
            second: Box::new(pane_tree(b, panes, leaves)),
        },
    }
}

//! Saving and restoring tabs: the automatic session and named layouts.

use std::path::PathBuf;

use dropshade_session::{self as sessions, Snapshot, TabSnapshot};
use iced::Task;

use super::{App, Message};

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
            .map(|tab| TabSnapshot {
                name: tab.custom_name.clone(),
                cwd: tab
                    .content
                    .focused_content()
                    .and_then(|session| session.terminal.working_directory()),
                command: None,
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
        let directory = saved
            .cwd
            .as_deref()
            .map(sessions::expand_home)
            .filter(|dir| dir.is_dir());
        let task = self.open_tab(directory);
        if let Some(id) = self.tabs.active_id() {
            if let Some(name) = &saved.name {
                self.tabs.rename(id, name);
            }
            if let Some(session) = self
                .tabs
                .get_mut(id)
                .and_then(|tab| tab.content.focused_content_mut())
            {
                session.pending_command.clone_from(&saved.command);
            }
        }
        task
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

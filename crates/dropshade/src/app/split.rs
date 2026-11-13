//! The panes of a tab: finding a pane's terminal, moving the keyboard between panes and resizing
//! them.

use std::path::PathBuf;

use dropshade_term::input::PaneDirection;
use dropshade_term::{Shortcut, TermEvent};
use iced::Task;
use iced::futures::Stream;
use iced::widget::pane_grid::{self, Direction, Pane};

use super::session::{self, Session};
use super::{App, CloseTarget, Message};
use crate::tabs::{Panes, SplitDirection, TabId};

/// One pane of one tab. `pane_grid` never reuses a pane id within a tab, so this stays unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneKey {
    pub tab: TabId,
    pub pane: Pane,
}

#[derive(Debug, Clone, Copy)]
pub enum PaneMessage {
    /// The pane was clicked.
    Focus(Pane),
    /// A divider was dragged.
    Resized(pane_grid::ResizeEvent),
}

impl App {
    /// The pane with the keyboard: the focused pane of the active tab.
    pub(super) fn focused_key(&self) -> Option<PaneKey> {
        self.tabs.active().map(|tab| PaneKey {
            tab: tab.id,
            pane: tab.content.focused(),
        })
    }

    pub(super) fn focused_session(&self) -> Option<&Session> {
        self.tabs.active()?.content.focused_content()
    }

    pub(super) fn focused_session_mut(&mut self) -> Option<&mut Session> {
        self.tabs.active_mut()?.content.focused_content_mut()
    }

    pub(super) fn session(&self, key: PaneKey) -> Option<&Session> {
        self.tabs.get(key.tab)?.content.get(key.pane)
    }

    pub(super) fn session_mut(&mut self, key: PaneKey) -> Option<&mut Session> {
        self.tabs.get_mut(key.tab)?.content.get_mut(key.pane)
    }

    /// Every terminal, in every tab.
    pub(super) fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.tabs
            .iter()
            .flat_map(|tab| tab.content.iter().map(|(_, session)| session))
    }

    pub(super) fn sessions_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        self.tabs
            .iter_mut()
            .flat_map(|tab| tab.content.iter_mut().map(|(_, session)| session))
    }

    /// Shows the tab of `key` and gives its pane the keyboard.
    pub(super) fn focus_pane(&mut self, key: PaneKey) {
        self.tabs.activate(key.tab);
        let changed = self
            .tabs
            .get_mut(key.tab)
            .is_some_and(|tab| tab.content.focus(key.pane));
        if changed {
            self.focus_changed(key.tab);
        }
    }

    /// After another pane of `tab` got the keyboard: the search was in the previous one, the new
    /// one's cursor shows at once, and the tab is named after it.
    pub(super) fn focus_changed(&mut self, tab: TabId) {
        if self.tabs.active_id() == Some(tab) && self.search.is_some() {
            self.search = None;
            self.sessions_mut()
                .for_each(|session| session.terminal.clear_search());
        }
        if let Some(tab) = self.tabs.get_mut(tab) {
            sync_title(tab);
            if let Some(session) = tab.content.focused_content_mut() {
                session.cursor_on = true;
            }
        }
    }

    /// Starts a shell for a new pane; a failure shows in the banner.
    pub(super) fn spawn_pane(
        &mut self,
        directory: Option<PathBuf>,
    ) -> Option<(Session, impl Stream<Item = TermEvent> + Send + use<>)> {
        match session::spawn(&self.settings, directory) {
            Ok(spawned) => Some(spawned),
            Err(error) => {
                tracing::error!(%error, "could not start a shell");
                self.settings
                    .warnings
                    .push(format!("Dropshade could not start a shell: {error}"));
                None
            }
        }
    }

    /// Splits the focused pane, starting the new shell in the same directory.
    fn split(&mut self, direction: SplitDirection) -> Task<Message> {
        let Some(tab) = self.tabs.active_id() else {
            return Task::none();
        };
        let directory = self
            .focused_session()
            .and_then(|session| session.terminal.working_directory());
        let Some((session, events)) = self.spawn_pane(directory) else {
            return Task::none();
        };
        let Some(pane) = self
            .tabs
            .get_mut(tab)
            .and_then(|entry| entry.content.split(direction, session))
        else {
            return Task::none();
        };
        self.focus_changed(tab);
        let key = PaneKey { tab, pane };
        Task::run(events, move |event| Message::Term(key, event))
    }

    /// The shortcuts that work on the panes of the active tab.
    pub(super) fn pane_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        let Some(tab) = self.tabs.active_id() else {
            return Task::none();
        };
        match shortcut {
            Shortcut::SplitRight => return self.split(SplitDirection::Right),
            Shortcut::SplitDown => return self.split(SplitDirection::Down),
            Shortcut::ClosePane => {
                return self
                    .focused_key()
                    .map_or_else(Task::none, |key| self.request_close(CloseTarget::Pane(key)));
            }
            Shortcut::FocusPane(direction) => {
                let moved = self
                    .tabs
                    .get_mut(tab)
                    .is_some_and(|entry| entry.content.focus_toward(direction_of(direction)));
                if moved {
                    self.focus_changed(tab);
                }
            }
            Shortcut::ResizePane(direction) => {
                if let Some(entry) = self.tabs.get_mut(tab) {
                    entry.content.resize_toward(direction_of(direction));
                }
            }
            Shortcut::TogglePaneZoom => {
                if let Some(entry) = self.tabs.get_mut(tab) {
                    entry.content.toggle_zoom();
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn pane_message(&mut self, tab: TabId, message: PaneMessage) -> Task<Message> {
        match message {
            PaneMessage::Focus(pane) => self.focus_pane(PaneKey { tab, pane }),
            PaneMessage::Resized(event) => {
                if let Some(tab) = self.tabs.get_mut(tab) {
                    tab.content.resize(event.split, event.ratio);
                }
            }
        }
        Task::none()
    }
}

fn direction_of(direction: PaneDirection) -> Direction {
    match direction {
        PaneDirection::Left => Direction::Left,
        PaneDirection::Right => Direction::Right,
        PaneDirection::Up => Direction::Up,
        PaneDirection::Down => Direction::Down,
    }
}

/// Names the tab after its focused pane's title.
pub(super) fn sync_title(tab: &mut crate::tabs::model::Tab<Panes<Session>>) {
    tab.title = tab
        .content
        .focused_content()
        .and_then(|session| session.terminal.title())
        .map(str::to_owned);
}

//! How the application reacts to messages.

use std::path::PathBuf;

use dropshade_term::{Interaction, Notice, Shortcut, TermEvent, Terminal, links};
use iced::widget::{Id, operation};
use iced::{Task, clipboard};

use super::{App, Menu, Message, PendingClose, Rename, session};
use crate::tabs::{TabId, TabMessage, bar};

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Term(id, event) => self.terminal_event(id, event),
            Message::Interaction(Interaction::ContextMenu(position)) => {
                self.terminal_menu = Some(position);
                Task::none()
            }
            Message::Interaction(Interaction::Shortcut(shortcut)) => self.shortcut(shortcut),
            Message::Interaction(interaction) => self.interaction(interaction),
            Message::Paste(text) => {
                if let (Some(tab), Some(text)) = (self.tabs.active(), text) {
                    tab.content.terminal.paste(&text);
                }
                Task::none()
            }
            Message::Blink => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.content.cursor_on = !tab.content.cursor_on;
                }
                Task::none()
            }
            Message::BellOff => {
                self.tabs
                    .iter_mut()
                    .for_each(|tab| tab.content.bell = false);
                Task::none()
            }
            Message::Tab(message) => self.tab_message(message),
            Message::Search(message) => self.search_message(message),
            Message::Menu(action) => self.menu_action(action),
            Message::Settings(message) => self.settings_message(message),
            Message::WindowResized(size) => {
                self.window_size = size;
                Task::none()
            }
            Message::Escape => {
                self.close_search();
                self.terminal_menu = None;
                self.settings_panel = None;
                self.renaming = None;
                self.menu = None;
                self.pending_close = None;
                Task::none()
            }
            Message::Enter if self.pending_close.is_some() => self.confirm_close(),
            Message::MouseReleased => {
                self.dragging = None;
                Task::none()
            }
            Message::ConfigChanged => self.reload_config(),
            Message::Ipc(command) => self.command(command),
            Message::IpcReady(Ok(connection)) => {
                self.ipc = Some(connection);
                Task::none()
            }
            Message::IpcReady(Err(error)) => {
                tracing::warn!(%error, "D-Bus service unavailable; the CLI and the extension cannot reach this instance");
                Task::none()
            }
            Message::Enter | Message::Done => Task::none(),
            Message::SaveSession => {
                self.save_session();
                Task::none()
            }
            Message::Terminate => {
                self.save_session();
                iced::exit()
            }
            Message::WindowClosed(id) => {
                if self.window == Some(id) {
                    self.window = None;
                    self.focused = false;
                }
                Task::none()
            }
            Message::CloseRequested => self.visibility(crate::ipc::client::Visibility::Hide),
            Message::WindowFocused(focused) => {
                self.focused = focused;
                Task::none()
            }
            Message::SystemTheme(mode) => {
                let prefers_light = mode == iced::theme::Mode::Light;
                if prefers_light != self.prefers_light {
                    self.prefers_light = prefers_light;
                    self.apply_settings();
                }
                Task::none()
            }
        }
    }

    pub(super) fn open_tab(&mut self, working_directory: Option<PathBuf>) -> Task<Message> {
        match session::spawn(&self.settings, working_directory) {
            Ok((session, events)) => {
                let id = self.tabs.insert(session);
                self.error = None;
                Task::run(events, move |event| Message::Term(id, event))
            }
            Err(error) => {
                tracing::error!(%error, "could not start a shell");
                self.error = Some(error.to_string());
                Task::none()
            }
        }
    }

    fn terminal_event(&mut self, id: TabId, event: TermEvent) -> Task<Message> {
        let palette = self.settings.style.palette;
        let Some(tab) = self.tabs.get_mut(id) else {
            return Task::none();
        };
        match tab.content.terminal.handle_event(event, &palette) {
            Some(Notice::Exited(_)) => self.close_tab(id),
            Some(Notice::TitleChanged) => {
                tab.title = tab.content.terminal.title().map(str::to_owned);
                Task::none()
            }
            Some(Notice::Copy { text, primary }) => copy(text, primary),
            Some(Notice::Bell) => {
                tab.content.bell = true;
                Task::none()
            }
            Some(Notice::Redraw) => {
                if let Some(command) = tab.content.pending_command.take() {
                    tab.content
                        .terminal
                        .write(format!("{command}\r").into_bytes());
                }
                Task::none()
            }
            None => Task::none(),
        }
    }

    fn interaction(&mut self, interaction: Interaction) -> Task<Message> {
        let Some(tab) = self.tabs.active_mut() else {
            return Task::none();
        };
        if matches!(interaction, Interaction::Input(_)) {
            tab.content.cursor_on = true;
        }
        if matches!(interaction, Interaction::FinishSelection) && self.settings.copy_on_select {
            let Some(text) = tab.content.terminal.selection_text() else {
                return Task::none();
            };
            return Task::batch([copy(text.clone(), true), copy(text, false)]);
        }
        apply(&mut tab.content.terminal, interaction)
    }

    fn shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        match shortcut {
            Shortcut::NewTab => {
                let directory = self
                    .tabs
                    .active()
                    .and_then(|tab| tab.content.terminal.working_directory());
                return self.open_tab(directory);
            }
            Shortcut::CloseTab => {
                return self
                    .tabs
                    .active_id()
                    .map_or_else(Task::none, |id| self.request_close(id));
            }
            Shortcut::RenameTab => {
                return self
                    .tabs
                    .active_id()
                    .map_or_else(Task::none, |id| self.start_rename(id));
            }
            Shortcut::Search => return self.open_search(),
            Shortcut::NextTab => self.tabs.cycle(1),
            Shortcut::PreviousTab => self.tabs.cycle(-1),
            Shortcut::MoveTabLeft => self.tabs.move_active(-1),
            Shortcut::MoveTabRight => self.tabs.move_active(1),
            Shortcut::GoToTab(index) => {
                self.tabs.activate_index(usize::from(index));
            }
            terminal_shortcut => {
                if let Some(tab) = self.tabs.active() {
                    return run_terminal_shortcut(&tab.content.terminal, terminal_shortcut);
                }
            }
        }
        Task::none()
    }

    fn tab_message(&mut self, message: TabMessage) -> Task<Message> {
        match message {
            TabMessage::New => return self.shortcut(Shortcut::NewTab),
            TabMessage::Pressed(id) => {
                self.tabs.activate(id);
                self.dragging = Some(id);
                self.menu = None;
            }
            TabMessage::Entered(id) => {
                self.hovered = Some(id);
                if let Some(dragged) = self.dragging.filter(|dragged| *dragged != id)
                    && let Some(index) = self.tabs.index_of(id)
                {
                    self.tabs.move_to(dragged, index);
                }
            }
            TabMessage::Exited(id) => {
                if self.hovered == Some(id) {
                    self.hovered = None;
                }
            }
            TabMessage::Released => self.dragging = None,
            TabMessage::RequestClose(id) => return self.request_close(id),
            TabMessage::StartRename(id) => return self.start_rename(id),
            TabMessage::RenameInput(draft) => {
                if let Some(rename) = self.renaming.as_mut() {
                    rename.draft = draft;
                }
            }
            TabMessage::CommitRename => {
                if let Some(rename) = self.renaming.take() {
                    self.tabs.rename(rename.id, &rename.draft);
                }
            }
            TabMessage::OpenMenu(id) => {
                self.renaming = None;
                self.menu = Some(Menu {
                    id,
                    position: self.bar_pointer,
                });
            }
            TabMessage::CloseMenu => self.menu = None,
            TabMessage::MoveLeft(id) | TabMessage::MoveRight(id) => {
                let offset = if matches!(message, TabMessage::MoveLeft(_)) {
                    -1
                } else {
                    1
                };
                if let Some(index) = self
                    .tabs
                    .index_of(id)
                    .and_then(|index| index.checked_add_signed(offset))
                {
                    self.tabs.move_to(id, index);
                }
                self.menu = None;
            }
            TabMessage::PointerMoved(position) => self.bar_pointer = position,
            TabMessage::ConfirmClose => return self.confirm_close(),
            TabMessage::CancelClose => self.pending_close = None,
        }
        Task::none()
    }

    fn request_close(&mut self, id: TabId) -> Task<Message> {
        self.menu = None;
        let running = self
            .tabs
            .get(id)
            .and_then(|tab| tab.content.terminal.foreground_process());
        match running {
            Some(process) => {
                self.tabs.activate(id);
                self.pending_close = Some(PendingClose {
                    id,
                    process: process.name,
                });
                Task::none()
            }
            None => self.close_tab(id),
        }
    }

    fn confirm_close(&mut self) -> Task<Message> {
        match self.pending_close.take() {
            Some(pending) => self.close_tab(pending.id),
            None => Task::none(),
        }
    }

    fn close_tab(&mut self, id: TabId) -> Task<Message> {
        self.tabs.close(id);
        if self.renaming.as_ref().is_some_and(|rename| rename.id == id) {
            self.renaming = None;
        }
        if self.tabs.is_empty() && self.error.is_none() {
            self.save_session();
            return iced::exit();
        }
        Task::none()
    }

    fn start_rename(&mut self, id: TabId) -> Task<Message> {
        let Some(tab) = self.tabs.get(id) else {
            return Task::none();
        };
        self.menu = None;
        self.renaming = Some(Rename {
            id,
            draft: tab.label().to_owned(),
        });
        let input = Id::new(bar::RENAME_INPUT);
        Task::batch([
            operation::focus(input.clone()),
            operation::select_all(input),
        ])
    }
}

fn apply(terminal: &mut Terminal, interaction: Interaction) -> Task<Message> {
    match interaction {
        Interaction::Input(bytes) => {
            terminal.scroll_to_bottom();
            terminal.write(bytes);
        }
        Interaction::Resize(size) => terminal.resize(size),
        Interaction::Scroll(lines) => terminal.scroll(lines),
        Interaction::Select { kind, at, side } => terminal.select(kind, at, side),
        Interaction::ExtendSelection { at, side } => terminal.extend_selection(at, side),
        Interaction::FinishSelection => {
            return terminal
                .selection_text()
                .map_or_else(Task::none, |text| copy(text, true));
        }
        Interaction::PastePrimary => return clipboard::read_primary().map(Message::Paste),
        Interaction::OpenLink(cell) => {
            if let Some(url) = terminal.link_at(cell) {
                open_link(&url);
            }
        }
        Interaction::Shortcut(shortcut) => return run_terminal_shortcut(terminal, shortcut),
        Interaction::ContextMenu(_) => {}
    }
    Task::none()
}

fn run_terminal_shortcut(terminal: &Terminal, shortcut: Shortcut) -> Task<Message> {
    match shortcut {
        Shortcut::Copy => {
            return terminal
                .selection_text()
                .map_or_else(Task::none, |text| copy(text, false));
        }
        Shortcut::Paste => return clipboard::read().map(Message::Paste),
        Shortcut::PastePrimary => return clipboard::read_primary().map(Message::Paste),
        Shortcut::ScrollPageUp => terminal.scroll_page(true),
        Shortcut::ScrollPageDown => terminal.scroll_page(false),
        Shortcut::ScrollToTop => terminal.scroll_to_top(),
        Shortcut::ScrollToBottom => terminal.scroll_to_bottom(),
        Shortcut::NewTab
        | Shortcut::CloseTab
        | Shortcut::NextTab
        | Shortcut::PreviousTab
        | Shortcut::MoveTabLeft
        | Shortcut::MoveTabRight
        | Shortcut::RenameTab
        | Shortcut::Search
        | Shortcut::GoToTab(_) => {}
    }
    Task::none()
}

fn open_link(url: &str) {
    if !links::is_openable(url) {
        tracing::warn!(url, "refusing to open a link with an unsupported scheme");
        return;
    }
    if let Err(error) = open::that_detached(url) {
        tracing::error!(url, %error, "could not open link");
    }
}

fn copy(text: String, primary: bool) -> Task<Message> {
    if primary {
        clipboard::write_primary(text)
    } else {
        clipboard::write(text)
    }
}

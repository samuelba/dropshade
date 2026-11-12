//! How the application reacts to messages.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dropshade_term::{Interaction, Notice, Shortcut, TermEvent, Terminal, links};
use iced::Task;
use iced::widget::{Id, operation};

use super::commands::{self, Finished};
use super::zoom::Zoom;
use super::{
    AboutMessage, App, Menu, Message, PendingClose, Rename, SettingsMessage, TerminalMenu, session,
};
use crate::clipboard::{Clipboard, Selection};
use crate::ipc::client::Visibility;
use crate::notifications::{self, DEFAULT_ACTION, Notification};
use crate::tabs::{Attention, TabId, TabMessage, bar};

impl App {
    fn set_drop_target(&mut self, target: Option<crate::dnd::DropTarget>) -> Task<Message> {
        self.drop_target = self.drop_target.take().or(target);
        Task::none()
    }

    fn paste_dropped_paths(&self, paths: &[std::path::PathBuf]) -> Task<Message> {
        if let Some(tab) = self.tabs.active() {
            let quoted: String = paths
                .iter()
                .map(|path| dropshade_term::input::shell_quoted_path(&path.to_string_lossy()))
                .collect();
            tab.content.terminal.paste(&quoted);
        }
        Task::none()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Term(id, event) => self.terminal_event(id, event),
            Message::Interaction(Interaction::ContextMenu(position)) => {
                self.terminal_menu = Some(TerminalMenu::at(position));
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
            Message::FilesDropped(paths) => self.paste_dropped_paths(&paths),
            Message::DropTarget(target) => self.set_drop_target(target),
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
            Message::About(message) => self.about_message(message),
            Message::ReleaseChecked(result) => self.release_checked(result),
            Message::WindowResized(size) => {
                self.window_size = size;
                Task::none()
            }
            Message::Escape | Message::Enter if self.about => {
                self.about_message(AboutMessage::Close)
            }
            Message::Escape if self.settings_panel.is_some() => {
                self.settings_message(SettingsMessage::Close)
            }
            Message::Escape => {
                self.close_search();
                self.terminal_menu = None;
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
            Message::Portal(event) => self.portal_event(event),
            Message::ScreensChanged(screens) => {
                self.screens = Some(screens);
                self.publish_window_settings()
            }
            Message::Ipc(command) => self.command(command),
            Message::IpcReady(Ok(connection)) => {
                self.ipc = Some(connection);
                Task::none()
            }
            Message::IpcReady(Err(error)) => {
                tracing::warn!(%error, "D-Bus service unavailable; the CLI and the extension cannot reach this instance");
                Task::none()
            }
            Message::Enter | Message::Done | Message::Layer(_) => Task::none(),
            Message::SaveSession => {
                self.save_session();
                Task::none()
            }
            Message::Terminate => {
                self.save_session();
                iced::exit()
            }
            Message::WindowOpened(id, size) => self.window_opened(id, size),
            Message::WindowClosed(id) => {
                self.window_closed(id);
                Task::none()
            }
            Message::CloseRequested => self.visibility(Visibility::Hide),
            Message::WindowFocused(focused) => self.window_focused(focused),
            Message::Slide => self.slide_step(),
            Message::PollCommands => self.poll_commands(),
            Message::ShowTab(id) => self.show_tab(id),
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
            Some(Notice::Copy { text, primary }) => self.clipboard.write(selection(primary), text),
            Some(Notice::Bell) => {
                tab.content.bell = true;
                if self.config.notifications.bell_indicator {
                    self.tabs.notice(id, Attention::Bell);
                }
                Task::none()
            }
            Some(Notice::Output) => {
                if let Some(command) = tab.content.pending_command.take() {
                    tab.content
                        .terminal
                        .write(format!("{command}\r").into_bytes());
                }
                if self.config.notifications.activity_indicator
                    && Instant::now() >= tab.content.quiet_until
                {
                    self.tabs.notice(id, Attention::Activity);
                }
                Task::none()
            }
            Some(Notice::Redraw) | None => Task::none(),
        }
    }

    fn show_tab(&mut self, id: TabId) -> Task<Message> {
        self.tabs.activate(id);
        self.visibility(Visibility::Show)
    }

    /// Looks which tabs' foreground commands finished, and announces the long ones while the
    /// window is out of sight or out of focus.
    fn poll_commands(&mut self) -> Task<Message> {
        let min = Duration::from_secs(self.config.notifications.command_min_duration.into());
        let now = Instant::now();
        let unattended = !self.focused;
        let mut announcements = Vec::new();
        for tab in self.tabs.iter_mut() {
            let current = tab.content.terminal.foreground_process();
            let (running, finished) = commands::step(tab.content.running.take(), current, now, min);
            tab.content.running = running;
            if let Some(finished) = finished.filter(|_| unattended) {
                announcements.push(announce_finished(tab.id, tab.label().to_owned(), finished));
            }
        }
        Task::batch(announcements)
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
            return Task::batch([
                self.clipboard.write(Selection::Primary, text.clone()),
                self.clipboard.write(Selection::Clipboard, text),
            ]);
        }
        apply(&mut tab.content.terminal, self.clipboard, interaction)
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
            Shortcut::ZoomIn => return self.set_zoom(self.zoom.zoom_in()),
            Shortcut::ZoomOut => return self.set_zoom(self.zoom.zoom_out()),
            Shortcut::ZoomReset => return self.set_zoom(Zoom::default()),
            Shortcut::NextTab => self.tabs.cycle(1),
            Shortcut::PreviousTab => self.tabs.cycle(-1),
            Shortcut::MoveTabLeft => self.tabs.move_active(-1),
            Shortcut::MoveTabRight => self.tabs.move_active(1),
            Shortcut::GoToTab(index) => {
                self.tabs.activate_index(usize::from(index));
            }
            terminal_shortcut => {
                if let Some(tab) = self.tabs.active() {
                    return run_terminal_shortcut(
                        &tab.content.terminal,
                        self.clipboard,
                        terminal_shortcut,
                    );
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

fn apply(terminal: &mut Terminal, clipboard: Clipboard, interaction: Interaction) -> Task<Message> {
    match interaction {
        Interaction::Input(bytes) => {
            terminal.scroll_to_bottom();
            terminal.write(bytes);
        }
        Interaction::Resize(size) => terminal.resize(size),
        Interaction::Scroll(lines) => terminal.scroll(lines),
        Interaction::ScrollTo(offset) => terminal.scroll_to(offset),
        Interaction::Select { kind, at, side } => terminal.select(kind, at, side),
        Interaction::ExtendSelection { at, side } => terminal.extend_selection(at, side),
        Interaction::FinishSelection => {
            return terminal
                .selection_text()
                .map_or_else(Task::none, |text| clipboard.write(Selection::Primary, text));
        }
        Interaction::PastePrimary => return paste(clipboard, Selection::Primary),
        Interaction::OpenLink(cell) => {
            if let Some(url) = terminal.link_at(cell) {
                open_link(&url);
            }
        }
        Interaction::Shortcut(shortcut) => {
            return run_terminal_shortcut(terminal, clipboard, shortcut);
        }
        Interaction::ContextMenu(_) => {}
    }
    Task::none()
}

fn run_terminal_shortcut(
    terminal: &Terminal,
    clipboard: Clipboard,
    shortcut: Shortcut,
) -> Task<Message> {
    match shortcut {
        Shortcut::Copy => {
            return terminal.selection_text().map_or_else(Task::none, |text| {
                clipboard.write(Selection::Clipboard, text)
            });
        }
        Shortcut::Paste => return paste(clipboard, Selection::Clipboard),
        Shortcut::PastePrimary => return paste(clipboard, Selection::Primary),
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
        | Shortcut::ZoomIn
        | Shortcut::ZoomOut
        | Shortcut::ZoomReset
        | Shortcut::GoToTab(_) => {}
    }
    Task::none()
}

/// A notification that `finished` ended in the tab labelled `label`; clicking it shows the tab.
fn announce_finished(id: TabId, label: String, finished: Finished) -> Task<Message> {
    Task::perform(
        async move {
            let summary = format!("{} finished", finished.name);
            let body = format!("in {label} after {}", commands::describe(finished.elapsed));
            let notification = Notification {
                summary: &summary,
                body: &body,
                actions: &[DEFAULT_ACTION, "Show"],
            };
            notifications::show_and_wait(notification).await
        },
        move |result| match result {
            Ok(Some(_)) => Message::ShowTab(id),
            Ok(None) => Message::Done,
            Err(error) => {
                tracing::warn!(%error, "cannot show the command notification");
                Message::Done
            }
        },
    )
}

pub(super) fn open_link(url: &str) {
    if !links::is_openable(url) {
        tracing::warn!(url, "refusing to open a link with an unsupported scheme");
        return;
    }
    if let Err(error) = open::that_detached(url) {
        tracing::error!(url, %error, "could not open link");
    }
}

fn paste(clipboard: Clipboard, selection: Selection) -> Task<Message> {
    clipboard.read(selection).map(Message::Paste)
}

fn selection(primary: bool) -> Selection {
    if primary {
        Selection::Primary
    } else {
        Selection::Clipboard
    }
}

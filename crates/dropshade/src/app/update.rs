//! How the application reacts to messages.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dropshade_term::mouse::CellPosition;
use dropshade_term::{
    Interaction, Notice, SearchDirection, Shortcut, TermEvent, Terminal, ViAction, links,
};
use iced::widget::{Id, operation};
use iced::{Point, Task};

use super::commands::{self, Finished};
use super::split::sync_title;
use super::view::bar_origin;
use super::zoom::Zoom;
use super::{
    AboutMessage, App, CloseTarget, Menu, Message, PaneKey, PendingClose, Rename, SettingsMessage,
    TerminalMenu, session,
};
use crate::clipboard::{Clipboard, Selection};
use crate::ipc::client::Visibility;
use crate::notifications::{self, DEFAULT_ACTION, Notification};
use crate::tabs::{Attention, Panes, TabId, TabMessage, bar};

impl App {
    fn set_drop_target(&mut self, target: Option<crate::dnd::DropTarget>) -> Task<Message> {
        self.drop_target = self.drop_target.take().or(target);
        Task::none()
    }

    fn paste_dropped_paths(&self, paths: &[std::path::PathBuf]) -> Task<Message> {
        if let Some(session) = self.focused_session() {
            let quoted: String = paths
                .iter()
                .map(|path| dropshade_term::input::shell_quoted_path(&path.to_string_lossy()))
                .collect();
            session.terminal.paste(&quoted);
        }
        Task::none()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Term(key, event) => self.terminal_event(key, event),
            Message::Interaction(key, Interaction::ContextMenu { position, cell }) => {
                // The menu's actions act on the focused pane, so it becomes the clicked one.
                self.focus_pane(key);
                self.terminal_menu = Some(TerminalMenu::at(position, Some(cell)));
                Task::none()
            }
            Message::Interaction(_, Interaction::Shortcut(shortcut)) => self.shortcut(shortcut),
            Message::Interaction(key, interaction) => self.interaction(key, interaction),
            Message::Paste(key, text) => {
                if let (Some(session), Some(text)) = (self.session(key), text) {
                    session.terminal.paste(&text);
                }
                Task::none()
            }
            Message::FilesDropped(paths) => self.paste_dropped_paths(&paths),
            Message::DropTarget(target) => self.set_drop_target(target),
            Message::Blink => {
                if let Some(session) = self.focused_session_mut() {
                    session.cursor_on = !session.cursor_on;
                }
                Task::none()
            }
            Message::BellOff => {
                self.sessions_mut().for_each(|session| session.bell = false);
                Task::none()
            }
            Message::Tab(message) => self.tab_message(message),
            Message::Pane(tab, message) => self.pane_message(tab, message),
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
            Message::ShowPane(key) => self.show_pane(key),
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
                let panes = Panes::new(session);
                let pane = panes.focused();
                let tab = self.tabs.insert(panes);
                self.error = None;
                let key = PaneKey { tab, pane };
                Task::run(events, move |event| Message::Term(key, event))
            }
            Err(error) => {
                tracing::error!(%error, "could not start a shell");
                self.error = Some(error.to_string());
                Task::none()
            }
        }
    }

    fn terminal_event(&mut self, key: PaneKey, event: TermEvent) -> Task<Message> {
        let palette = self.settings.style.palette;
        let Some(tab) = self.tabs.get_mut(key.tab) else {
            return Task::none();
        };
        let focused = tab.content.focused() == key.pane;
        let Some(session) = tab.content.get_mut(key.pane) else {
            return Task::none();
        };
        match session.terminal.handle_event(event, &palette) {
            Some(Notice::Exited(_)) => self.close_pane(key),
            Some(Notice::TitleChanged) => {
                if focused {
                    sync_title(tab);
                }
                Task::none()
            }
            Some(Notice::Copy { text, primary }) => self.clipboard.write(selection(primary), text),
            Some(Notice::Bell) => {
                session.bell = self.config.notifications.visual_bell;
                if self.config.notifications.bell_indicator {
                    self.tabs.notice(key.tab, Attention::Bell);
                }
                Task::none()
            }
            Some(Notice::Output) => {
                // In vi mode the view stays on what is being selected.
                if self.settings.scroll.on_output && !session.terminal.vi_mode() {
                    session.terminal.scroll_to_bottom();
                }
                if let Some(command) = session.pending_command.take() {
                    session.terminal.write(format!("{command}\r").into_bytes());
                }
                if self.config.notifications.activity_indicator
                    && Instant::now() >= session.quiet_until
                {
                    self.tabs.notice(key.tab, Attention::Activity);
                }
                Task::none()
            }
            Some(Notice::Redraw) | None => Task::none(),
        }
    }

    fn show_pane(&mut self, key: PaneKey) -> Task<Message> {
        self.focus_pane(key);
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
            let label = tab.label().to_owned();
            for (pane, session) in tab.content.iter_mut() {
                let current = session.terminal.foreground_process();
                let (running, finished) = commands::step(session.running.take(), current, now, min);
                session.running = running;
                if let Some(finished) = finished.filter(|_| unattended) {
                    let key = PaneKey { tab: tab.id, pane };
                    announcements.push(announce_finished(key, label.clone(), finished));
                }
            }
        }
        Task::batch(announcements)
    }

    fn interaction(&mut self, key: PaneKey, interaction: Interaction) -> Task<Message> {
        if let Interaction::Vi(action) = interaction {
            return self.vi_action(key, action);
        }
        let (copy_on_select, on_keystroke) = (
            self.settings.copy_on_select,
            self.settings.scroll.on_keystroke,
        );
        let clipboard = self.clipboard;
        let Some(session) = self.session_mut(key) else {
            return Task::none();
        };
        if matches!(interaction, Interaction::Input(_)) {
            session.cursor_on = true;
        }
        if matches!(interaction, Interaction::FinishSelection) && copy_on_select {
            let Some(text) = session.terminal.selection_text() else {
                return Task::none();
            };
            return Task::batch([
                clipboard.write(Selection::Primary, text.clone()),
                clipboard.write(Selection::Clipboard, text),
            ]);
        }
        if matches!(interaction, Interaction::Input(_)) && on_keystroke {
            session.terminal.scroll_to_bottom();
        }
        apply(key, &mut session.terminal, clipboard, interaction)
    }

    /// A key pressed in vi mode.
    fn vi_action(&mut self, key: PaneKey, action: ViAction) -> Task<Message> {
        let clipboard = self.clipboard;
        let Some(session) = self.session_mut(key) else {
            return Task::none();
        };
        session.cursor_on = true;
        let terminal = &mut session.terminal;
        match action {
            ViAction::Motion(motion) => terminal.vi_motion(motion),
            ViAction::Select(kind) => terminal.vi_select(kind),
            ViAction::Scroll(scroll) => terminal.vi_scroll(scroll),
            ViAction::Top => terminal.vi_goto_top(),
            ViAction::Bottom => terminal.vi_goto_bottom(),
            ViAction::Escape if terminal.vi_selection().is_some() => terminal.clear_selection(),
            ViAction::Escape | ViAction::Exit => terminal.toggle_vi_mode(),
            ViAction::Yank => {
                let text = terminal.selection_text();
                terminal.toggle_vi_mode();
                return text.map_or_else(Task::none, |text| {
                    Task::batch([
                        clipboard.write(Selection::Primary, text.clone()),
                        clipboard.write(Selection::Clipboard, text),
                    ])
                });
            }
            ViAction::Search(direction) => {
                terminal.start_vi_search(direction);
                return self.open_search();
            }
            ViAction::SearchNext { reverse } => {
                terminal.vi_search_next(reverse);
            }
        }
        Task::none()
    }

    pub(super) fn shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        match shortcut {
            Shortcut::NewTab => {
                let directory = self
                    .focused_session()
                    .and_then(|session| session.terminal.working_directory());
                return self.open_tab(directory);
            }
            Shortcut::CloseTab => {
                return self
                    .tabs
                    .active_id()
                    .map_or_else(Task::none, |id| self.request_close(CloseTarget::Tab(id)));
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
            Shortcut::ToggleMaximize => return self.toggle_maximize(),
            Shortcut::SelectCommandOutput => return self.select_command_output(None),
            Shortcut::OpenSettings if self.settings_panel.is_none() => {
                return self.open_settings();
            }
            Shortcut::OpenSettings => {}
            Shortcut::ToggleViMode => {
                if let Some(session) = self.focused_session_mut() {
                    session.terminal.toggle_vi_mode();
                    session.cursor_on = true;
                }
            }
            Shortcut::SplitRight
            | Shortcut::SplitDown
            | Shortcut::ClosePane
            | Shortcut::FocusPane(_)
            | Shortcut::ResizePane(_)
            | Shortcut::TogglePaneZoom => return self.pane_shortcut(shortcut),
            Shortcut::NextTab => self.tabs.cycle(1),
            Shortcut::PreviousTab => self.tabs.cycle(-1),
            Shortcut::MoveTabLeft => self.tabs.move_active(-1),
            Shortcut::MoveTabRight => self.tabs.move_active(1),
            Shortcut::GoToTab(index) => {
                self.tabs.activate_index(usize::from(index));
            }
            terminal_shortcut => {
                if let Some(key) = self.focused_key()
                    && let Some(session) = self.session(key)
                {
                    return run_terminal_shortcut(
                        key,
                        &session.terminal,
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
            TabMessage::RequestClose(id) => return self.request_close(CloseTarget::Tab(id)),
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
            TabMessage::OpenBarMenu => {
                self.renaming = None;
                self.menu = None;
                let origin = bar_origin(self.settings.bar, self.window_size);
                let position =
                    Point::new(origin.x + self.bar_pointer.x, origin.y + self.bar_pointer.y);
                self.terminal_menu = Some(TerminalMenu::at(position, None));
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

    /// Selects a command's output (the one at `at`, or the newest) as the primary selection,
    /// and copies it too when copy-on-select is on.
    pub(super) fn select_command_output(&mut self, at: Option<CellPosition>) -> Task<Message> {
        let Some(session) = self.focused_session() else {
            return Task::none();
        };
        let terminal = &session.terminal;
        if !terminal.select_command_output(at) {
            return Task::none();
        }
        let Some(text) = terminal.selection_text() else {
            return Task::none();
        };
        if self.settings.copy_on_select {
            return Task::batch([
                self.clipboard.write(Selection::Primary, text.clone()),
                self.clipboard.write(Selection::Clipboard, text),
            ]);
        }
        self.clipboard.write(Selection::Primary, text)
    }

    /// Closes a tab or pane, asking first when something runs in it.
    pub(super) fn request_close(&mut self, target: CloseTarget) -> Task<Message> {
        self.menu = None;
        let Some(tab) = self.tabs.get(target.tab()) else {
            return Task::none();
        };
        let running: Vec<RunningCommand> = tab
            .content
            .iter()
            .filter(|(pane, _)| match target {
                CloseTarget::Tab(_) => true,
                CloseTarget::Pane(key) => *pane == key.pane,
            })
            .filter_map(|(_, session)| running_command(&session.terminal))
            .collect();
        let others = running.len().saturating_sub(1);
        let Some(first) = running.into_iter().next() else {
            return self.close(target);
        };
        self.tabs.activate(target.tab());
        self.pending_close = Some(PendingClose {
            target,
            process: first.name,
            others,
        });
        Task::none()
    }

    fn confirm_close(&mut self) -> Task<Message> {
        match self.pending_close.take() {
            Some(pending) => self.close(pending.target),
            None => Task::none(),
        }
    }

    fn close(&mut self, target: CloseTarget) -> Task<Message> {
        match target {
            CloseTarget::Tab(id) => self.close_tab(id),
            CloseTarget::Pane(key) => self.close_pane(key),
        }
    }

    /// Closes a pane, or its tab when it is the last one.
    pub(super) fn close_pane(&mut self, key: PaneKey) -> Task<Message> {
        let Some(tab) = self.tabs.get_mut(key.tab) else {
            return Task::none();
        };
        if !tab.content.is_split() {
            return self.close_tab(key.tab);
        }
        let was_focused = tab.content.focused() == key.pane;
        if tab.content.close(key.pane).is_some() && was_focused {
            self.focus_changed(key.tab);
        }
        if self
            .pending_close
            .as_ref()
            .is_some_and(|pending| pending.target == CloseTarget::Pane(key))
        {
            self.pending_close = None;
        }
        Task::none()
    }

    fn close_tab(&mut self, id: TabId) -> Task<Message> {
        self.tabs.close(id);
        if self.renaming.as_ref().is_some_and(|rename| rename.id == id) {
            self.renaming = None;
        }
        if self
            .pending_close
            .as_ref()
            .is_some_and(|pending| pending.target.tab() == id)
        {
            self.pending_close = None;
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

/// Something closing the terminal would interrupt.
struct RunningCommand {
    /// The program's name, when it is known.
    name: Option<String>,
}

/// What runs in the terminal, if anything. With shell integration the shell's marks tell (which
/// also covers ssh or a nested shell sitting at a prompt); otherwise any foreground program
/// other than the shell counts.
fn running_command(terminal: &Terminal) -> Option<RunningCommand> {
    let foreground = || terminal.foreground_process().map(|process| process.name);
    match terminal.command_running() {
        Some(false) => None,
        Some(true) => Some(RunningCommand { name: foreground() }),
        None => foreground().map(|name| RunningCommand { name: Some(name) }),
    }
}

fn apply(
    key: PaneKey,
    terminal: &mut Terminal,
    clipboard: Clipboard,
    interaction: Interaction,
) -> Task<Message> {
    match interaction {
        Interaction::Input(bytes) => terminal.write(bytes),
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
        Interaction::PastePrimary => return paste(key, clipboard, Selection::Primary),
        Interaction::OpenLink(cell) => {
            if let Some(url) = terminal.link_at(cell) {
                open_link(&url);
            }
        }
        Interaction::Shortcut(shortcut) => {
            return run_terminal_shortcut(key, terminal, clipboard, shortcut);
        }
        // Vi mode keys are handled before this, in `vi_action`.
        Interaction::ContextMenu { .. } | Interaction::Vi(_) => {}
    }
    Task::none()
}

fn run_terminal_shortcut(
    key: PaneKey,
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
        Shortcut::Paste => return paste(key, clipboard, Selection::Clipboard),
        Shortcut::PastePrimary => return paste(key, clipboard, Selection::Primary),
        Shortcut::ScrollPageUp => terminal.scroll_page(true),
        Shortcut::ScrollPageDown => terminal.scroll_page(false),
        Shortcut::ScrollToTop => terminal.scroll_to_top(),
        Shortcut::ScrollToBottom => terminal.scroll_to_bottom(),
        Shortcut::PreviousPrompt => {
            terminal.jump_to_prompt(SearchDirection::Older);
        }
        Shortcut::NextPrompt => {
            terminal.jump_to_prompt(SearchDirection::Newer);
        }
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
        | Shortcut::ToggleMaximize
        | Shortcut::SelectCommandOutput
        | Shortcut::OpenSettings
        | Shortcut::ToggleViMode
        | Shortcut::GoToTab(_)
        | Shortcut::SplitRight
        | Shortcut::SplitDown
        | Shortcut::ClosePane
        | Shortcut::FocusPane(_)
        | Shortcut::ResizePane(_)
        | Shortcut::TogglePaneZoom => {}
    }
    Task::none()
}

/// A notification that `finished` ended in the tab labelled `label`; clicking it shows the pane.
fn announce_finished(key: PaneKey, label: String, finished: Finished) -> Task<Message> {
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
            Ok(Some(_)) => Message::ShowPane(key),
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

fn paste(key: PaneKey, clipboard: Clipboard, selection: Selection) -> Task<Message> {
    clipboard
        .read(selection)
        .map(move |text| Message::Paste(key, text))
}

fn selection(primary: bool) -> Selection {
    if primary {
        Selection::Primary
    } else {
        Selection::Clipboard
    }
}

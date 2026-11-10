//! Showing and hiding the window. With the GNOME Shell extension, it owns visibility and these
//! requests are forwarded to it; without it, hiding closes the window while the tabs keep
//! running, and showing opens a new one.

use iced::futures::channel::mpsc;
use iced::window::settings::PlatformSpecific;
use iced::{Task, window};

use super::{App, INITIAL_SIZE, Message, WindowSettings};
use crate::ipc::client::{self, Visibility};
use crate::ipc::{Command, service};

pub const APP_ID: &str = "dev.samuelba.Dropshade";

pub fn window_settings(managed: bool) -> window::Settings {
    window::Settings {
        size: INITIAL_SIZE,
        transparent: true,
        decorations: !managed,
        exit_on_close_request: false,
        platform_specific: PlatformSpecific {
            application_id: APP_ID.to_owned(),
            ..PlatformSpecific::default()
        },
        ..window::Settings::default()
    }
}

impl App {
    pub(super) fn open_window(&mut self) -> Task<Message> {
        let (id, open) = window::open(window_settings(self.managed));
        self.window = Some(id);
        open.discard()
    }

    pub(super) fn start_service(&self) -> Task<Message> {
        let (sender, commands) = mpsc::unbounded();
        let window = WindowSettings::from(&self.config.window);
        let start = Task::perform(service::start(window, sender), |result| {
            Message::IpcReady(result.map_err(|error| error.to_string()))
        });
        Task::batch([start, Task::run(commands, Message::Ipc)])
    }

    pub(super) fn command(&mut self, command: Command) -> Task<Message> {
        match command {
            Command::Toggle => self.visibility(Visibility::Toggle),
            Command::Show => self.visibility(Visibility::Show),
            Command::Hide => self.visibility(Visibility::Hide),
            Command::NewTab(directory) => {
                let directory = directory.or_else(|| {
                    self.tabs
                        .active()
                        .and_then(|tab| tab.content.terminal.working_directory())
                });
                let tab = self.open_tab(directory);
                Task::batch([tab, self.visibility(Visibility::Show)])
            }
            Command::SaveLayout(name) => {
                self.save_layout(&name);
                Task::none()
            }
            Command::LoadLayout(name) => {
                let tabs = self.load_layout(&name);
                Task::batch([tabs, self.visibility(Visibility::Show)])
            }
            Command::OpenSettings => {
                let settings = self.open_settings();
                Task::batch([settings, self.visibility(Visibility::Show)])
            }
        }
    }

    pub(super) fn visibility(&mut self, action: Visibility) -> Task<Message> {
        if self.managed {
            return self.forward_to_shell(action);
        }
        // Without the extension GNOME may not focus a window opened outside a user action, so
        // toggling follows whether the window is open, not whether it has the focus.
        match action {
            Visibility::Toggle if self.window.is_some() => self.hide(),
            Visibility::Toggle | Visibility::Show => self.show(),
            Visibility::Hide => self.hide(),
        }
    }

    /// Sends the current window settings to the extension, which applies them on its next show.
    pub(super) fn publish_window_settings(&self) -> Task<Message> {
        let Some(connection) = self.ipc.clone() else {
            return Task::none();
        };
        let window = WindowSettings::from(&self.config.window);
        Task::perform(
            async move { service::publish(&connection, window).await },
            |result| {
                if let Err(error) = result {
                    tracing::warn!(%error, "could not publish window settings");
                }
                Message::Done
            },
        )
    }

    fn forward_to_shell(&self, action: Visibility) -> Task<Message> {
        let Some(connection) = self.ipc.clone() else {
            tracing::warn!("cannot reach the GNOME Shell extension: no D-Bus connection");
            return Task::none();
        };
        Task::perform(
            async move { client::shell(&connection, action).await },
            |result| {
                if let Err(error) = result {
                    tracing::warn!(%error, "the GNOME Shell extension did not respond");
                }
                Message::Done
            },
        )
    }

    fn show(&mut self) -> Task<Message> {
        match self.window {
            Some(id) => window::gain_focus(id),
            None => self.open_window(),
        }
    }

    fn hide(&mut self) -> Task<Message> {
        self.focused = false;
        self.window.take().map_or_else(Task::none, window::close)
    }
}

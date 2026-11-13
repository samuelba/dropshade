//! Showing and hiding the window. With the GNOME Shell extension, it owns visibility and these
//! requests are forwarded to it. On compositors with layer shell, Dropshade slides its own
//! drop-down in and out. Otherwise hiding closes the window while the tabs keep running, and
//! showing opens a new one.

use std::time::Instant;

use iced::futures::channel::mpsc;
use iced::window::settings::PlatformSpecific;
use iced::{Task, window};

use super::dropdown::Dropdown;
use super::{App, Host, INITIAL_SIZE, Message, WindowSettings};
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
        if self.host == Host::Layer {
            // Screens come and go between shows, so the list is resolved again each time.
            self.screens = crate::wayland::screens();
            let (dropdown, open) = Dropdown::open(&self.config.window, self.screens.as_deref());
            self.dropdown = Some(dropdown);
            return open;
        }
        let (id, open) = window::open(window_settings(self.host == Host::Shell));
        self.window = Some(id);
        open.discard()
    }

    pub(super) fn start_service(&self) -> Task<Message> {
        let (sender, commands) = mpsc::unbounded();
        let window = WindowSettings::new(&self.config.window, self.screens.as_deref());
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
                    self.focused_session()
                        .and_then(|session| session.terminal.working_directory())
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
            Command::OpenAbout => {
                let about = self.open_about();
                Task::batch([about, self.visibility(Visibility::Show)])
            }
        }
    }

    pub(super) fn visibility(&mut self, action: Visibility) -> Task<Message> {
        match self.host {
            Host::Shell => self.forward_to_shell(action),
            Host::Layer => self.slide(action),
            Host::Window => self.window_visibility(action),
        }
    }

    /// Fills the work area, or returns to the configured size. Hiding ends it.
    pub(super) fn toggle_maximize(&mut self) -> Task<Message> {
        match self.host {
            Host::Layer => self.dropdown.as_mut().map_or_else(Task::none, |dropdown| {
                dropdown.toggle_maximize(Instant::now())
            }),
            Host::Shell => self.forward_maximize_to_shell(),
            Host::Window => self.window.map_or_else(Task::none, window::toggle_maximize),
        }
    }

    fn forward_maximize_to_shell(&self) -> Task<Message> {
        let Some(connection) = self.ipc.clone() else {
            tracing::warn!("cannot reach the GNOME Shell extension: no D-Bus connection");
            return Task::none();
        };
        Task::perform(
            async move { client::shell_toggle_maximize(&connection).await },
            |result| {
                if let Err(error) = result {
                    tracing::warn!(
                        %error,
                        "the GNOME Shell extension cannot maximize; update it with \
                         `dropshade gnome-extension install`"
                    );
                }
                Message::Done
            },
        )
    }

    /// Toggling follows whether the drop-down is shown or on its way in, not the focus: after a
    /// click into another window the hotkey hides it rather than focusing it again.
    fn slide(&mut self, action: Visibility) -> Task<Message> {
        let now = Instant::now();
        let showing = self.dropdown.as_ref().is_some_and(Dropdown::showing);
        match (action, self.dropdown.as_mut()) {
            (Visibility::Toggle, Some(dropdown)) if showing => dropdown.hide(now),
            (Visibility::Hide, Some(dropdown)) => dropdown.hide(now),
            (Visibility::Toggle | Visibility::Show, Some(dropdown)) => dropdown.show(now),
            (Visibility::Toggle | Visibility::Show, None) => return self.open_window(),
            (Visibility::Hide, None) => {}
        }
        Task::none()
    }

    /// Advances the drop-down's slide, and closes it once it has slid out.
    pub(super) fn slide_step(&mut self) -> Task<Message> {
        let Some(dropdown) = self.dropdown.as_mut() else {
            return Task::none();
        };
        let (step, hidden) = dropdown.tick(Instant::now());
        if !hidden {
            return step;
        }
        self.focused = false;
        self.dropdown
            .take()
            .map_or_else(Task::none, |dropdown| window::close(dropdown.id))
    }

    /// A window reported its first size. A window that opens at its final size gets no resize
    /// event, so this is the only size it reports. For the drop-down's surface, this is the
    /// work area it was measuring.
    pub(super) fn window_opened(&mut self, id: window::Id, size: iced::Size) -> Task<Message> {
        self.window_size = size;
        let placed = match self.dropdown.as_mut() {
            Some(dropdown) if dropdown.id == id => dropdown.placed(size, Instant::now()),
            _ => Task::none(),
        };
        // iced_layershell ignores window::run; its connection was handed over at boot.
        let drop_target = if self.drop_target.is_none() && self.host != Host::Layer {
            window::run(id, crate::dnd::DropTarget::of_window).map(Message::DropTarget)
        } else {
            Task::none()
        };
        Task::batch([placed, drop_target])
    }

    pub(super) fn window_closed(&mut self, id: window::Id) {
        if self.window == Some(id) {
            self.window = None;
            self.focused = false;
        }
        if self
            .dropdown
            .as_ref()
            .is_some_and(|dropdown| dropdown.id == id)
        {
            self.dropdown = None;
            self.focused = false;
        }
    }

    pub(super) fn window_focused(&mut self, focused: bool) -> Task<Message> {
        self.focused = focused;
        // The extension does this on GNOME; a layer surface has to do it itself.
        if !focused && self.host == Host::Layer && self.config.window.hide_on_unfocus {
            return self.visibility(Visibility::Hide);
        }
        Task::none()
    }

    fn window_visibility(&mut self, action: Visibility) -> Task<Message> {
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
        let window = WindowSettings::new(&self.config.window, self.screens.as_deref());
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

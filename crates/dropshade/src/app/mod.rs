//! The Dropshade application: tabs of terminals plus the chrome around them.

mod session;
mod update;
mod view;
mod watch;

use std::path::PathBuf;
use std::time::Duration;

use dropshade_config::Config;
use dropshade_term::{Interaction, TermEvent};
use iced::keyboard::{self, key::Named};
use iced::theme::Mode;
use iced::{Event, Point, Size, Subscription, Task, event, mouse, system, time, window};

use crate::settings::{self, Settings};
use crate::tabs::{TabId, TabMessage, Tabs};
use session::Session;

pub const APP_NAME: &str = "Dropshade";
pub const INITIAL_SIZE: Size = Size::new(1100.0, 640.0);
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
const BELL_DURATION: Duration = Duration::from_millis(120);

#[derive(Debug)]
pub struct App {
    tabs: Tabs<Session>,
    config: Config,
    config_path: Option<PathBuf>,
    /// Why the config file could not be used; the last good settings stay active meanwhile.
    config_error: Option<String>,
    prefers_light: bool,
    settings: Settings,
    hovered: Option<TabId>,
    renaming: Option<Rename>,
    menu: Option<Menu>,
    pending_close: Option<PendingClose>,
    dragging: Option<TabId>,
    bar_pointer: Point,
    window: Size,
    error: Option<String>,
}

#[derive(Debug)]
struct Rename {
    id: TabId,
    draft: String,
}

#[derive(Debug, Clone, Copy)]
struct Menu {
    id: TabId,
    position: Point,
}

#[derive(Debug)]
struct PendingClose {
    id: TabId,
    process: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Term(TabId, TermEvent),
    Interaction(Interaction),
    Paste(Option<String>),
    Blink,
    BellOff,
    Tab(TabMessage),
    WindowResized(Size),
    Escape,
    Enter,
    MouseReleased,
    ConfigChanged,
    SystemTheme(Mode),
}

impl App {
    pub fn boot(config_path: Option<PathBuf>) -> (Self, Task<Message>) {
        let (config, config_error) = load(config_path.as_deref());
        let settings = settings::build(&config, false, dropshade_config::themes_dir().as_deref());
        let mut app = Self {
            tabs: Tabs::default(),
            config,
            config_path,
            config_error,
            prefers_light: false,
            settings,
            hovered: None,
            renaming: None,
            menu: None,
            pending_close: None,
            dragging: None,
            bar_pointer: Point::ORIGIN,
            window: INITIAL_SIZE,
            error: None,
        };
        let first_tab = app.open_tab(app.settings.working_directory.clone());
        (
            app,
            Task::batch([first_tab, system::theme().map(Message::SystemTheme)]),
        )
    }

    pub fn title(&self) -> String {
        self.tabs
            .active()
            .map_or_else(|| APP_NAME.to_owned(), |tab| tab.label().to_owned())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let blink = self
            .tabs
            .active()
            .filter(|tab| tab.content.terminal.cursor_blinks())
            .map_or_else(Subscription::none, |_| {
                time::every(BLINK_INTERVAL).map(|_| Message::Blink)
            });
        let bell = if self.tabs.iter().any(|tab| tab.content.bell) {
            time::every(BELL_DURATION).map(|_| Message::BellOff)
        } else {
            Subscription::none()
        };
        let overlay_keys = if self.overlay_open() {
            event::listen_with(overlay_key)
        } else {
            Subscription::none()
        };
        let drag_end = if self.dragging.is_some() {
            event::listen_with(mouse_released)
        } else {
            Subscription::none()
        };
        let config = self
            .config_path
            .clone()
            .map_or_else(Subscription::none, watch::config_changes);
        Subscription::batch([
            blink,
            bell,
            overlay_keys,
            drag_end,
            config,
            system::theme_changes().map(Message::SystemTheme),
            window::resize_events().map(|(_, size)| Message::WindowResized(size)),
        ])
    }

    fn overlay_open(&self) -> bool {
        self.renaming.is_some() || self.menu.is_some() || self.pending_close.is_some()
    }

    /// Re-reads the config file. A broken file keeps the current settings and shows why.
    fn reload_config(&mut self) {
        let Some(path) = self.config_path.as_deref() else {
            return;
        };
        match dropshade_config::load(path) {
            Ok(config) => {
                self.config = config;
                self.config_error = None;
                self.apply_settings();
            }
            Err(error) => {
                tracing::warn!(%error, "keeping the previous configuration");
                self.config_error = Some(error.to_string());
            }
        }
    }

    fn apply_settings(&mut self) {
        let themes = dropshade_config::themes_dir();
        self.settings = settings::build(&self.config, self.prefers_light, themes.as_deref());
        for tab in self.tabs.iter() {
            tab.content
                .terminal
                .update_options(self.settings.scrollback_lines, self.settings.cursor_style);
        }
    }
}

fn load(path: Option<&std::path::Path>) -> (Config, Option<String>) {
    match path.map(dropshade_config::load) {
        Some(Ok(config)) => (config, None),
        Some(Err(error)) => {
            tracing::warn!(%error, "using the default configuration");
            (Config::default(), Some(error.to_string()))
        }
        None => (Config::default(), None),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "signature required by event::listen_with"
)]
fn overlay_key(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            ..
        }) => match named {
            Named::Escape => Some(Message::Escape),
            Named::Enter => Some(Message::Enter),
            _ => None,
        },
        _ => None,
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "signature required by event::listen_with"
)]
fn mouse_released(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    matches!(
        event,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    )
    .then_some(Message::MouseReleased)
}

//! The Dropshade application: tabs of terminals plus the chrome around them.

mod about;
mod commands;
mod dropdown;
mod menu;
mod panel;
mod persist;
mod search;
mod session;
mod split;
mod update;
mod view;
mod visibility;
mod watch;
mod zoom;

use std::path::PathBuf;
use std::time::Duration;

use dropshade_config::Config;
use dropshade_term::{Interaction, TermEvent};
use iced::keyboard::{self, key::Named};
use iced::theme::Mode;
use iced::{Event, Point, Size, Subscription, Task, event, mouse, system, time, window};
use iced_layershell::actions::LayerShellCustomActionWithId;
use ipc::WindowSettings;

use crate::clipboard::Clipboard;
use crate::dnd::{self, DropTarget};
use crate::hotkey;
use crate::ipc::{self, Command};
use crate::screens::{self, Screen};
use crate::settings::{self, Settings};
use crate::tabs::{Panes, TabId, TabMessage, Tabs};
pub use about::AboutMessage;
pub use dropdown::NAMESPACE as LAYER_NAMESPACE;
pub use menu::MenuAction;
use menu::TerminalMenu;
pub use panel::SettingsMessage;
pub use persist::Startup;
pub use search::SearchMessage;
use session::Session;
pub use split::{PaneKey, PaneMessage};
pub use visibility::APP_ID;
use zoom::Zoom;

pub const APP_NAME: &str = "Dropshade";
pub const INITIAL_SIZE: Size = Size::new(1100.0, 640.0);
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
const BELL_DURATION: Duration = Duration::from_millis(120);
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(3);
const SLIDE_INTERVAL: Duration = Duration::from_millis(8);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Who shows, hides and places the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// The GNOME Shell extension, which started this process.
    Shell,
    /// Dropshade itself, as a layer-shell surface the compositor keeps on top.
    Layer,
    /// Nobody: an ordinary window that hiding closes.
    Window,
}

#[derive(Debug)]
pub struct App {
    tabs: Tabs<Panes<Session>>,
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
    search: Option<search::SearchBar>,
    terminal_menu: Option<TerminalMenu>,
    settings_panel: Option<panel::SettingsPanel>,
    about: bool,
    /// A newer release than this build, once the check found one.
    release: Option<crate::release::Release>,
    /// The font zoom of the terminals, on top of the configured size.
    zoom: Zoom,
    dragging: Option<TabId>,
    bar_pointer: Point,
    window_size: Size,
    error: Option<String>,
    /// The window, while it is open; without the extension, hiding closes it.
    window: Option<window::Id>,
    /// The layer-shell surface, while it is shown or sliding.
    dropdown: Option<dropdown::Dropdown>,
    focused: bool,
    host: Host,
    clipboard: Clipboard,
    /// The connected screens, to pick from `window.monitor`; `None` until known.
    screens: Option<Vec<Screen>>,
    hotkey: HotkeyBinding,
    ipc: Option<zbus::Connection>,
    session_path: Option<PathBuf>,
    /// The last snapshot written, to skip saves when nothing changed.
    saved: Option<dropshade_session::Snapshot>,
    /// Where file drops arrive; winit's window shares its connection once it opens.
    drop_target: Option<DropTarget>,
}

/// Binding the hotkey outside GNOME.
#[derive(Debug, Default)]
struct HotkeyBinding {
    /// Where it is bound, and how that went.
    status: Option<(hotkey::Binder, hotkey::Status)>,
    /// Counts requests to bind it through the portal; a new one asks again.
    portal_requests: u32,
    /// The desktop portal turned out to have no global shortcuts.
    portal_missing: bool,
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
    target: CloseTarget,
    /// The running program, when its name is known.
    process: Option<String>,
    /// How many other panes of the tab also run something.
    others: usize,
}

/// What a close asks to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseTarget {
    Tab(TabId),
    Pane(PaneKey),
}

impl CloseTarget {
    fn tab(self) -> TabId {
        match self {
            Self::Tab(id) => id,
            Self::Pane(key) => key.tab,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Term(PaneKey, TermEvent),
    Interaction(PaneKey, Interaction),
    Paste(PaneKey, Option<String>),
    FilesDropped(Vec<PathBuf>),
    DropTarget(Option<DropTarget>),
    Blink,
    BellOff,
    Tab(TabMessage),
    Pane(TabId, PaneMessage),
    Search(SearchMessage),
    Menu(MenuAction),
    Settings(SettingsMessage),
    About(AboutMessage),
    ReleaseChecked(Result<Option<crate::release::Release>, String>),
    WindowResized(Size),
    Escape,
    Enter,
    MouseReleased,
    ConfigChanged,
    SystemTheme(Mode),
    Ipc(Command),
    /// The desktop portal bound the hotkey or reported a press.
    Portal(hotkey::PortalEvent),
    /// GNOME's compositor reported the connected screens.
    ScreensChanged(Vec<Screen>),
    IpcReady(Result<zbus::Connection, String>),
    WindowOpened(window::Id, Size),
    WindowClosed(window::Id),
    CloseRequested,
    WindowFocused(bool),
    SaveSession,
    Terminate,
    /// A step of the drop-down's slide.
    Slide,
    /// Time to look which commands finished.
    PollCommands,
    /// Show the window on this pane, e.g. after its notification was clicked.
    ShowPane(PaneKey),
    /// A request for the layer-shell runner; the plain window runner never sees one.
    Layer(LayerShellCustomActionWithId),
    Done,
}

impl TryInto<LayerShellCustomActionWithId> for Message {
    type Error = Self;

    fn try_into(self) -> Result<LayerShellCustomActionWithId, Self> {
        match self {
            Self::Layer(action) => Ok(action),
            message => Err(message),
        }
    }
}

impl App {
    pub fn boot(
        config_path: Option<PathBuf>,
        host: Host,
        clipboard: Clipboard,
        startup: Startup,
        drop_target: Option<DropTarget>,
    ) -> (Self, Task<Message>) {
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
            search: None,
            terminal_menu: None,
            settings_panel: None,
            about: false,
            release: None,
            zoom: Zoom::default(),
            dragging: None,
            bar_pointer: Point::ORIGIN,
            window_size: INITIAL_SIZE,
            error: None,
            window: None,
            dropdown: None,
            focused: false,
            host,
            clipboard,
            screens: None,
            hotkey: HotkeyBinding::default(),
            ipc: None,
            session_path: dropshade_session::session_path(),
            saved: None,
            drop_target,
        };
        if app.binds_in_cosmic()
            && let Some(status) = hotkey::set_up_cosmic(&app.config.window.hotkey)
        {
            app.hotkey.status = Some((hotkey::Binder::Cosmic, status));
        }
        let first_tab = app.startup_tabs(startup);
        let open = app.open_window();
        let service = app.start_service();
        let theme = system::theme().map(Message::SystemTheme);
        let release = app.check_release();
        (app, Task::batch([first_tab, open, service, theme, release]))
    }

    pub fn theme(&self, _window: window::Id) -> iced::Theme {
        self.settings
            .colors
            .iced_theme(&self.settings.style.palette)
    }

    pub fn title(&self, _window: window::Id) -> String {
        self.tabs
            .active()
            .map_or_else(|| APP_NAME.to_owned(), |tab| tab.label().to_owned())
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let blink = self
            .focused_session()
            .filter(|session| session.terminal.cursor_blinks())
            .map_or_else(Subscription::none, |_| {
                time::every(BLINK_INTERVAL).map(|_| Message::Blink)
            });
        let bell = if self.sessions().any(|session| session.bell) {
            time::every(BELL_DURATION).map(|_| Message::BellOff)
        } else {
            Subscription::none()
        };
        let commands = if self.config.notifications.command_finished {
            time::every(COMMAND_POLL_INTERVAL).map(|_| Message::PollCommands)
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
        let autosave = if self.config.session.restore {
            time::every(AUTOSAVE_INTERVAL).map(|_| Message::SaveSession)
        } else {
            Subscription::none()
        };
        let slide = if self
            .dropdown
            .as_ref()
            .is_some_and(dropdown::Dropdown::animating)
        {
            time::every(SLIDE_INTERVAL).map(|_| Message::Slide)
        } else {
            Subscription::none()
        };
        // On GNOME the extension places the window, from the screen this process publishes.
        let screens = if self.host == Host::Shell {
            Subscription::run(screens::changes).map(Message::ScreensChanged)
        } else {
            Subscription::none()
        };
        let portal = if self.binds_through_portal() {
            let request = (
                self.config.window.hotkey.trim().to_owned(),
                self.hotkey.portal_requests,
            );
            Subscription::run_with(request, hotkey::portal_events).map(Message::Portal)
        } else {
            Subscription::none()
        };
        let drops = self
            .drop_target
            .clone()
            .map_or_else(Subscription::none, |target| {
                Subscription::run_with(target, dnd::drops).map(Message::FilesDropped)
            });
        Subscription::batch([
            drops,
            autosave,
            screens,
            portal,
            slide,
            Subscription::run(termination_signals),
            blink,
            bell,
            commands,
            overlay_keys,
            drag_end,
            config,
            system::theme_changes().map(Message::SystemTheme),
            window::resize_events().map(|(_, size)| Message::WindowResized(size)),
            window::close_requests().map(|_| Message::CloseRequested),
            window::close_events().map(Message::WindowClosed),
            window::events().filter_map(|(id, event)| match event {
                window::Event::Opened { size, .. } => Some(Message::WindowOpened(id, size)),
                window::Event::Focused => Some(Message::WindowFocused(true)),
                window::Event::Unfocused => Some(Message::WindowFocused(false)),
                window::Event::FileDropped(path) => Some(Message::FilesDropped(vec![path])),
                _ => None,
            }),
        ])
    }

    fn overlay_open(&self) -> bool {
        self.renaming.is_some()
            || self.menu.is_some()
            || self.pending_close.is_some()
            || self.search.is_some()
            || self.terminal_menu.is_some()
            || self.settings_panel.is_some()
            || self.about
    }

    /// Re-reads the config file. A broken file keeps the current settings and shows why.
    fn reload_config(&mut self) -> Task<Message> {
        let Some(path) = self.config_path.as_deref() else {
            return Task::none();
        };
        match dropshade_config::load(path) {
            Ok(config) => {
                let previous = std::mem::replace(&mut self.config, config);
                self.config_error = None;
                self.apply_settings();
                if let Some(panel) = self.settings_panel.as_mut() {
                    panel.config_reloaded();
                }
                if previous.window.hotkey.trim() != self.config.window.hotkey.trim() {
                    let hotkey = self.config.window.hotkey.clone();
                    self.bind_hotkey(&hotkey);
                }
                return self.publish_window_settings();
            }
            Err(error) => {
                tracing::warn!(%error, "keeping the previous configuration");
                self.config_error = Some(error.to_string());
            }
        }
        Task::none()
    }

    /// Whether Dropshade sets the hotkey in COSMIC's shortcuts.
    fn binds_in_cosmic(&self) -> bool {
        self.host == Host::Layer && hotkey::on_cosmic()
    }

    /// Whether Dropshade asks the desktop portal for the hotkey: on layer-shell desktops other
    /// than COSMIC. Where the portal has no global shortcuts this just finds out and stops.
    fn binds_through_portal(&self) -> bool {
        self.host == Host::Layer && !hotkey::on_cosmic()
    }

    /// Whether the settings panel offers the hotkey on a layer-shell desktop.
    fn hotkey_settable(&self) -> bool {
        self.binds_in_cosmic() || (self.binds_through_portal() && !self.hotkey.portal_missing)
    }

    /// Sets `hotkey` as the desktop's shortcut for Dropshade, where Dropshade does that. The
    /// portal subscription follows the configured hotkey; this asks it again for the same one.
    fn bind_hotkey(&mut self, hotkey: &str) {
        if self.binds_in_cosmic() {
            self.hotkey.status = Some((hotkey::Binder::Cosmic, hotkey::bind_cosmic(hotkey)));
        } else if self.binds_through_portal() {
            self.hotkey.portal_requests = self.hotkey.portal_requests.wrapping_add(1);
        }
    }

    fn portal_event(&mut self, event: hotkey::PortalEvent) -> Task<Message> {
        match event {
            hotkey::PortalEvent::Toggle => return self.command(Command::Toggle),
            hotkey::PortalEvent::Status(status) => {
                self.hotkey.status = Some((hotkey::Binder::Portal, status));
            }
            hotkey::PortalEvent::Unavailable => self.hotkey.portal_missing = true,
        }
        Task::none()
    }

    fn apply_settings(&mut self) {
        let themes = dropshade_config::themes_dir();
        let settings = settings::build(&self.config, self.prefers_light, themes.as_deref());
        self.settings = if self.zoom.is_normal() {
            settings
        } else {
            settings.zoomed(self.zoom.scale(), self.config.font.line_height)
        };
        let (scrollback, cursor_style, cell) = (
            self.settings.scrollback_lines,
            self.settings.cursor_style,
            self.settings.style.cell,
        );
        for session in self.sessions_mut() {
            let terminal = &mut session.terminal;
            terminal.update_options(scrollback, cursor_style);
            terminal.set_cell(cell);
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

/// SIGTERM (logout), SIGHUP and SIGINT: save the session before quitting.
fn termination_signals() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::SinkExt;
    use tokio::signal::unix::{SignalKind, signal};

    iced::stream::channel(
        1,
        async |mut output: iced::futures::channel::mpsc::Sender<Message>| {
            let signals = (
                signal(SignalKind::terminate()),
                signal(SignalKind::hangup()),
                signal(SignalKind::interrupt()),
            );
            let (Ok(mut terminate), Ok(mut hangup), Ok(mut interrupt)) = signals else {
                tracing::warn!(
                    "cannot listen for termination signals; the session is saved periodically"
                );
                return;
            };
            tokio::select! {
                _ = terminate.recv() => {}
                _ = hangup.recv() => {}
                _ = interrupt.recv() => {}
            }
            if output.send(Message::Terminate).await.is_err() {
                tracing::debug!("app already shutting down");
            }
        },
    )
}

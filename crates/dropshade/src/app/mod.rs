//! The Dropshade application: tabs of terminals plus the chrome around them.

mod session;
mod update;
mod view;

use std::time::Duration;

use dropshade_term::{InputConfig, Interaction, Palette, TermEvent, ViewStyle, metrics};
use iced::keyboard::{self, key::Named};
use iced::{Event, Font, Point, Size, Subscription, Task, event, mouse, time, window};

use crate::tabs::{BarPosition, TabId, TabMessage, Tabs};
use crate::theme::UiColors;
use session::Session;

pub const APP_NAME: &str = "Dropshade";
pub const INITIAL_SIZE: Size = Size::new(1100.0, 640.0);
const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT: f32 = 1.25;
const PADDING: f32 = 10.0;
const BACKGROUND_OPACITY: f32 = 0.94;
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
const BELL_DURATION: Duration = Duration::from_millis(120);
/// Temporary until the config file exists (Phase 3): top, bottom, left or right.
const TAB_BAR_ENV: &str = "DROPSHADE_TAB_BAR";

#[derive(Debug)]
pub struct App {
    tabs: Tabs<Session>,
    style: ViewStyle,
    input: InputConfig,
    colors: UiColors,
    bar: BarPosition,
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
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let font = Font::MONOSPACE;
        let palette = Palette::DUSK;
        let style = ViewStyle {
            palette,
            font,
            font_size: FONT_SIZE,
            cell: metrics::cell_size(font, FONT_SIZE, LINE_HEIGHT),
            padding: PADDING,
            background_opacity: BACKGROUND_OPACITY,
        };
        let mut app = Self {
            tabs: Tabs::default(),
            colors: UiColors::from_palette(&palette, BACKGROUND_OPACITY),
            style,
            input: InputConfig::default(),
            bar: tab_bar_position(),
            hovered: None,
            renaming: None,
            menu: None,
            pending_close: None,
            dragging: None,
            bar_pointer: Point::ORIGIN,
            window: INITIAL_SIZE,
            error: None,
        };
        let task = app.open_tab(None);
        (app, task)
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
        let resize = window::resize_events().map(|(_, size)| Message::WindowResized(size));
        Subscription::batch([blink, bell, overlay_keys, drag_end, resize])
    }

    fn overlay_open(&self) -> bool {
        self.renaming.is_some() || self.menu.is_some() || self.pending_close.is_some()
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

fn tab_bar_position() -> BarPosition {
    let Ok(value) = std::env::var(TAB_BAR_ENV) else {
        return BarPosition::default();
    };
    value.parse().unwrap_or_else(|error: String| {
        tracing::warn!(%error, "ignoring {TAB_BAR_ENV}");
        BarPosition::default()
    })
}

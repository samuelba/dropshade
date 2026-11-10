//! Dropshade: a drop-down terminal for Linux.

use dropshade_term::{
    InputConfig, Interaction, Notice, Palette, Shortcut, SpawnOptions, TermEvent, Terminal,
    TerminalView, TerminalWidget, ViewStyle, links, metrics,
};
use iced::clipboard;
use iced::widget::{container, text};
use iced::window::settings::PlatformSpecific;
use iced::{Color, Element, Fill, Font, Size, Subscription, Task, Theme, time, window};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "io.github.samuelba.Dropshade";
const APP_NAME: &str = "Dropshade";
const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT: f32 = 1.25;
const PADDING: f32 = 10.0;
const BACKGROUND_OPACITY: f32 = 0.94;
const SHELL_LAUNCHED_ENV: &str = "DROPSHADE_SHELL_LAUNCHED";
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
const BELL_DURATION: Duration = Duration::from_millis(120);

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    dropshade_term::setup_env();
    let managed = is_shell_managed(std::env::var(SHELL_LAUNCHED_ENV).ok().as_deref());

    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        // FIFO presentation stalls under Mutter: a commit carrying a wp_fifo_v1 barrier can wait
        // forever for a repaint, freezing the drop-down until it is hidden and shown again.
        // Without vsync, wgpu picks mailbox or immediate; winit still paces redraws with frame
        // callbacks, and ICED_PRESENT_MODE overrides this choice.
        .settings(iced::Settings {
            vsync: false,
            ..iced::Settings::default()
        })
        .window(window::Settings {
            size: Size::new(1100.0, 640.0),
            transparent: true,
            decorations: !managed,
            platform_specific: PlatformSpecific {
                application_id: APP_ID.to_owned(),
                ..PlatformSpecific::default()
            },
            ..window::Settings::default()
        })
        .style(|_, theme: &Theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .run()
}

#[derive(Debug)]
enum App {
    Running(Box<Session>),
    Failed(String),
}

#[derive(Debug)]
struct Session {
    terminal: Terminal,
    style: ViewStyle,
    input: InputConfig,
    cursor_on: bool,
    bell: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Term(TermEvent),
    Interaction(Interaction),
    Paste(Option<String>),
    Blink,
    BellOff,
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let font = Font::MONOSPACE;
        let cell = metrics::cell_size(font, FONT_SIZE, LINE_HEIGHT);
        let style = ViewStyle {
            palette: Palette::DUSK,
            font,
            font_size: FONT_SIZE,
            cell,
            padding: PADDING,
            background_opacity: BACKGROUND_OPACITY,
        };
        match Terminal::spawn(SpawnOptions {
            cell,
            ..SpawnOptions::default()
        }) {
            Ok((terminal, events)) => {
                let session = Session {
                    terminal,
                    style,
                    input: InputConfig::default(),
                    cursor_on: true,
                    bell: false,
                };
                (
                    Self::Running(Box::new(session)),
                    Task::run(events, Message::Term),
                )
            }
            Err(error) => {
                tracing::error!(%error, "could not start the shell");
                (Self::Failed(error.to_string()), Task::none())
            }
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Running(session) => session.terminal.title().unwrap_or(APP_NAME).to_owned(),
            Self::Failed(_) => APP_NAME.to_owned(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let Self::Running(session) = self else {
            return Task::none();
        };
        match message {
            Message::Term(event) => {
                match session.terminal.handle_event(event, &session.style.palette) {
                    Some(Notice::Exited(_)) => iced::exit(),
                    Some(Notice::Copy { text, primary }) => copy(text, primary),
                    Some(Notice::Bell) => {
                        session.bell = true;
                        Task::none()
                    }
                    _ => Task::none(),
                }
            }
            Message::Interaction(interaction) => {
                if matches!(interaction, Interaction::Input(_)) {
                    session.cursor_on = true;
                }
                apply(&mut session.terminal, interaction)
            }
            Message::Blink => {
                session.cursor_on = !session.cursor_on;
                Task::none()
            }
            Message::BellOff => {
                session.bell = false;
                Task::none()
            }
            Message::Paste(Some(text)) => {
                session.terminal.paste(&text);
                Task::none()
            }
            Message::Paste(None) => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let Self::Running(session) = self else {
            return Subscription::none();
        };
        let blink = if session.terminal.cursor_blinks() {
            time::every(BLINK_INTERVAL).map(|_| Message::Blink)
        } else {
            Subscription::none()
        };
        let bell = if session.bell {
            time::every(BELL_DURATION).map(|_| Message::BellOff)
        } else {
            Subscription::none()
        };
        Subscription::batch([blink, bell])
    }

    fn view(&self) -> Element<'_, Message> {
        match self {
            Self::Running(session) => {
                let view = TerminalView::new(
                    &session.terminal,
                    &session.style,
                    &session.input,
                    Message::Interaction,
                )
                .cursor_visible(session.cursor_on || !session.terminal.cursor_blinks())
                .bell(session.bell);
                TerminalWidget::new(view).into()
            }
            Self::Failed(error) => container(text(format!(
                "Dropshade could not start a shell.\n\n{error}"
            )))
            .center(Fill)
            .into(),
        }
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
        Interaction::Shortcut(shortcut) => return run_shortcut(terminal, shortcut),
    }
    Task::none()
}

fn run_shortcut(terminal: &Terminal, shortcut: Shortcut) -> Task<Message> {
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

/// Whether the GNOME Shell extension started the app and therefore owns window placement.
fn is_shell_managed(env_value: Option<&str>) -> bool {
    env_value == Some("1")
}

#[cfg(test)]
mod tests {
    use super::is_shell_managed;

    #[test]
    fn only_the_extension_marker_enables_managed_mode() {
        assert!(is_shell_managed(Some("1")));
        assert!(!is_shell_managed(Some("0")));
        assert!(!is_shell_managed(Some("")));
        assert!(!is_shell_managed(None));
    }
}

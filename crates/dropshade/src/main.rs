//! Dropshade: a drop-down terminal for Linux.

use dropshade_term::{
    InputConfig, Interaction, Notice, Palette, SpawnOptions, TermEvent, Terminal, TerminalView,
    ViewStyle, metrics,
};
use iced::widget::{canvas, container, text};
use iced::window::settings::PlatformSpecific;
use iced::{Color, Element, Fill, Font, Size, Task, Theme, window};
use tracing_subscriber::EnvFilter;

const APP_ID: &str = "io.github.samuelba.Dropshade";
const APP_NAME: &str = "Dropshade";
const FONT_SIZE: f32 = 15.0;
const LINE_HEIGHT: f32 = 1.25;
const PADDING: f32 = 10.0;
const BACKGROUND_OPACITY: f32 = 0.94;
const SHELL_LAUNCHED_ENV: &str = "DROPSHADE_SHELL_LAUNCHED";

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    dropshade_term::setup_env();
    let managed = is_shell_managed(std::env::var(SHELL_LAUNCHED_ENV).ok().as_deref());

    iced::application(App::boot, App::update, App::view)
        .title(App::title)
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
    Running(Session),
    Failed(String),
}

#[derive(Debug)]
struct Session {
    terminal: Terminal,
    style: ViewStyle,
    input: InputConfig,
}

#[derive(Debug, Clone)]
enum Message {
    Term(TermEvent),
    Interaction(Interaction),
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
                };
                (Self::Running(session), Task::run(events, Message::Term))
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
                    _ => Task::none(),
                }
            }
            Message::Interaction(interaction) => {
                apply(&mut session.terminal, interaction);
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self {
            Self::Running(session) => canvas(TerminalView::new(
                &session.terminal,
                &session.style,
                &session.input,
                Message::Interaction,
            ))
            .width(Fill)
            .height(Fill)
            .into(),
            Self::Failed(error) => container(text(format!(
                "Dropshade could not start a shell.\n\n{error}"
            )))
            .center(Fill)
            .into(),
        }
    }
}

fn apply(terminal: &mut Terminal, interaction: Interaction) {
    match interaction {
        Interaction::Input(bytes) => {
            terminal.scroll_to_bottom();
            terminal.write(bytes);
        }
        Interaction::Resize(size) => terminal.resize(size),
        Interaction::Scroll(lines) => terminal.scroll(lines),
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

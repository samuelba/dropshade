//! The About dialog: icon, name, version and where to find the project.

mod icon;

use iced::widget::{Space, button, canvas, column, container, opaque, row, text};
use iced::{Center, Element, Fill, Font, Task, font};

use super::update::open_link;
use super::{APP_NAME, App, Message};

const DIALOG_WIDTH: f32 = 380.0;
const ICON_SIZE: f32 = 96.0;
const TEXT_SIZE: f32 = 13.0;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

#[derive(Debug, Clone, Copy)]
pub enum AboutMessage {
    OpenRepository,
    Close,
}

impl App {
    pub(super) fn open_about(&mut self) -> Task<Message> {
        self.about = true;
        Task::none()
    }

    pub(super) fn about_message(&mut self, message: AboutMessage) -> Task<Message> {
        match message {
            AboutMessage::OpenRepository => open_link(REPOSITORY),
            AboutMessage::Close => self.about = false,
        }
        Task::none()
    }

    pub(super) fn about_layer(&self) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let bold = Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        };
        let heading = column![
            text(APP_NAME).size(24).font(bold),
            text(format!("Version {VERSION}"))
                .size(TEXT_SIZE)
                .color(colors.muted),
        ]
        .spacing(2)
        .align_x(Center);
        let repository = button(text(display_url(REPOSITORY)).size(TEXT_SIZE))
            .padding([6, 12])
            .style(move |_, status| colors.ghost_button(colors.accent, status))
            .on_press(Message::About(AboutMessage::OpenRepository));
        let credits = text(format!("{LICENSE} license · by {}", author_names(AUTHORS)))
            .size(12)
            .color(colors.muted);
        let close = button(text("Close").size(TEXT_SIZE))
            .padding([7, 16])
            .style(move |_, status| colors.accent_button(status))
            .on_press(Message::About(AboutMessage::Close));
        let card = container(
            column![
                canvas(icon::Icon).width(ICON_SIZE).height(ICON_SIZE),
                heading,
                text(DESCRIPTION).size(TEXT_SIZE),
                column![repository, credits].spacing(2).align_x(Center),
                row![Space::new().width(Fill), close, Space::new().width(Fill)],
            ]
            .spacing(14)
            .align_x(Center),
        )
        .padding(24)
        .width(DIALOG_WIDTH)
        .style(move |_| colors.surface_style());
        opaque(
            container(card)
                .center(Fill)
                .style(move |_| colors.backdrop_style()),
        )
    }
}

/// A URL without its scheme, for showing.
fn display_url(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

/// The names of the authors, without their email addresses.
fn author_names(authors: &str) -> String {
    authors
        .split(':')
        .map(|author| author.split('<').next().unwrap_or(author).trim())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_are_shown_without_their_email() {
        assert_eq!(
            author_names("Samuel Bachmann <samuel@example.com>"),
            "Samuel Bachmann"
        );
    }

    #[test]
    fn several_authors_are_listed() {
        assert_eq!(
            author_names("Ada <ada@example.com>:Grace <grace@example.com>"),
            "Ada, Grace"
        );
    }

    #[test]
    fn the_cargo_author_has_a_name() {
        assert_ne!(author_names(AUTHORS), "");
        assert!(!author_names(AUTHORS).contains('@'));
    }

    #[test]
    fn urls_are_shown_without_the_scheme() {
        assert_eq!(display_url("https://github.com/a/b"), "github.com/a/b");
        assert_eq!(display_url("github.com/a/b"), "github.com/a/b");
    }
}

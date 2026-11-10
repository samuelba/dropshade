//! Layout: tab bar plus the active terminal, with the context menu and close dialog on top.

use dropshade_term::colors::to_iced;
use dropshade_term::{TerminalView, TerminalWidget};
use iced::widget::{Space, button, column, container, mouse_area, opaque, pin, row, stack, text};
use iced::{Element, Fill, Font, Point, Size, font};

use super::{App, Menu, Message, PendingClose};
use crate::tabs::{BarPosition, BarState, TabMessage, bar};

const MENU_WIDTH: f32 = 168.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const MENU_ITEMS: f32 = 4.0;
const MENU_PADDING: f32 = 4.0;
const DIALOG_WIDTH: f32 = 380.0;
const TEXT_SIZE: f32 = 13.0;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let mut layers = vec![self.body()];
        if let Some(menu) = self.menu {
            layers.push(self.menu_layer(menu));
        }
        if let Some(pending) = &self.pending_close {
            layers.push(self.close_dialog(pending));
        }
        stack(layers).width(Fill).height(Fill).into()
    }

    fn body(&self) -> Element<'_, Message> {
        let state = BarState {
            position: self.settings.bar,
            hovered: self.hovered,
            renaming: self
                .renaming
                .as_ref()
                .map(|rename| (rename.id, rename.draft.as_str())),
            colors: self.settings.colors,
            width: self.window.width,
            tab_count: self.tabs.len(),
        };
        let tab_bar = bar::view(&self.tabs, state).map(Message::Tab);
        let terminal = match self.banner() {
            Some(banner) => column![banner, self.terminal()].into(),
            None => self.terminal(),
        };
        match self.settings.bar {
            BarPosition::Top => column![tab_bar, terminal].into(),
            BarPosition::Bottom => column![terminal, tab_bar].into(),
            BarPosition::Left => row![tab_bar, terminal].into(),
            BarPosition::Right => row![terminal, tab_bar].into(),
        }
    }

    fn terminal(&self) -> Element<'_, Message> {
        if let Some(tab) = self.tabs.active() {
            let session = &tab.content;
            let view = TerminalView::new(
                &session.terminal,
                &self.settings.style,
                &self.settings.input,
                Message::Interaction,
            )
            .cursor_visible(session.cursor_on || !session.terminal.cursor_blinks())
            .bell(session.bell)
            .accepts_input(!self.overlay_open());
            return TerminalWidget::new(view).into();
        }
        let message = self.error.as_deref().map_or_else(String::new, |error| {
            format!("Dropshade could not start a shell.\n\n{error}")
        });
        container(text(message).color(self.settings.colors.text))
            .center(Fill)
            .into()
    }

    /// A strip above the terminal explaining configuration problems.
    fn banner(&self) -> Option<Element<'_, Message>> {
        let palette = &self.settings.style.palette;
        let (message, color) = match (&self.config_error, self.settings.warnings.as_slice()) {
            (Some(error), _) => (
                format!("Configuration error, still using the previous settings: {error}"),
                palette.normal[1],
            ),
            (None, []) => return None,
            (None, warnings) => (warnings.join("\n"), palette.normal[3]),
        };
        let colors = self.settings.colors;
        Some(
            container(text(message).size(TEXT_SIZE).color(to_iced(color, 1.0)))
                .padding([6, 12])
                .width(Fill)
                .style(move |_| container::Style::default().background(colors.surface))
                .into(),
        )
    }

    fn menu_layer(&self, menu: Menu) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let index = self.tabs.index_of(menu.id).unwrap_or_default();
        let last = self.tabs.len().saturating_sub(1);
        let item = |label: &'static str, message: Option<TabMessage>| {
            let enabled = message.is_some();
            let color = if enabled { colors.text } else { colors.muted };
            button(text(label).size(TEXT_SIZE))
                .width(Fill)
                .height(MENU_ITEM_HEIGHT)
                .padding([7, 10])
                .style(move |_, status| colors.ghost_button(color, status))
                .on_press_maybe(message.map(Message::Tab))
        };
        let items = column![
            item("Rename", Some(TabMessage::StartRename(menu.id))),
            item(
                "Move left",
                (index > 0).then_some(TabMessage::MoveLeft(menu.id))
            ),
            item(
                "Move right",
                (index < last).then_some(TabMessage::MoveRight(menu.id))
            ),
            item("Close", Some(TabMessage::RequestClose(menu.id))),
        ];
        let card = container(items)
            .padding(MENU_PADDING)
            .width(MENU_WIDTH)
            .style(move |_| colors.surface_style());
        let position = menu_position(self.settings.bar, menu.position, self.window);
        let dismiss = mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Tab(TabMessage::CloseMenu))
            .on_right_press(Message::Tab(TabMessage::CloseMenu));
        stack![dismiss, pin(card).position(position)].into()
    }

    fn close_dialog(&self, pending: &PendingClose) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let bold = Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        };
        let label = self
            .tabs
            .get(pending.id)
            .map_or("this tab", |tab| tab.label());
        let actions = row![
            Space::new().width(Fill),
            button(text("Cancel").size(TEXT_SIZE))
                .padding([7, 14])
                .style(move |_, status| colors.ghost_button(colors.text, status))
                .on_press(Message::Tab(TabMessage::CancelClose)),
            button(text("Close tab").size(TEXT_SIZE))
                .padding([7, 14])
                .style(move |_, status| colors.accent_button(status))
                .on_press(Message::Tab(TabMessage::ConfirmClose)),
        ]
        .spacing(8);
        let card = container(
            column![
                text(format!("Close “{label}”?")).size(16).font(bold),
                text(format!(
                    "“{}” is still running. Closing the tab stops it.",
                    pending.process
                ))
                .size(TEXT_SIZE)
                .color(colors.muted),
                actions,
            ]
            .spacing(14),
        )
        .padding(20)
        .width(DIALOG_WIDTH)
        .style(move |_| colors.surface_style());
        opaque(
            container(card)
                .center(Fill)
                .style(move |_| colors.backdrop_style()),
        )
    }
}

/// Where the tab context menu opens: at the pointer, translated from bar to window coordinates
/// and kept inside the window.
fn menu_position(bar: BarPosition, pointer: Point, window: Size) -> Point {
    let origin = match bar {
        BarPosition::Top | BarPosition::Left => Point::ORIGIN,
        BarPosition::Bottom => Point::new(0.0, window.height - bar::HORIZONTAL_HEIGHT),
        BarPosition::Right => Point::new(window.width - bar::VERTICAL_WIDTH, 0.0),
    };
    let height = MENU_ITEMS * MENU_ITEM_HEIGHT + 2.0 * MENU_PADDING;
    Point::new(
        (origin.x + pointer.x).clamp(0.0, (window.width - MENU_WIDTH).max(0.0)),
        (origin.y + pointer.y).clamp(0.0, (window.height - height).max(0.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1000.0, 600.0);

    #[test]
    fn top_bar_menus_open_at_the_pointer() {
        let at = menu_position(BarPosition::Top, Point::new(120.0, 20.0), WINDOW);
        assert_eq!(at, Point::new(120.0, 20.0));
    }

    #[test]
    fn bottom_bar_menus_stay_inside_the_window() {
        let at = menu_position(BarPosition::Bottom, Point::new(50.0, 20.0), WINDOW);
        let height = MENU_ITEMS * MENU_ITEM_HEIGHT + 2.0 * MENU_PADDING;
        assert_eq!(at, Point::new(50.0, WINDOW.height - height));
    }

    #[test]
    fn right_bar_menus_are_shifted_into_view() {
        let at = menu_position(BarPosition::Right, Point::new(150.0, 40.0), WINDOW);
        assert_eq!(at, Point::new(WINDOW.width - MENU_WIDTH, 40.0));
    }

    #[test]
    fn tiny_windows_do_not_produce_negative_positions() {
        let at = menu_position(
            BarPosition::Top,
            Point::new(10.0, 10.0),
            Size::new(50.0, 50.0),
        );
        assert_eq!(at, Point::ORIGIN);
    }
}

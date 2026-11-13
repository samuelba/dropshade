//! Layout: tab bar plus the active terminal, with the context menu and close dialog on top.

use dropshade_term::colors::to_iced;
use dropshade_term::{TerminalView, TerminalWidget};
use iced::widget::canvas::{self, Canvas};
use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::{Space, button, column, container, mouse_area, opaque, pin, row, stack, text};
use iced::{Background, Border, Color, Element, Fill, Font, Point, Size, font, mouse};

use super::session::Session;
use super::{App, CloseTarget, Host, Menu, Message, PaneKey, PaneMessage, PendingClose};
use crate::tabs::{BarPosition, BarState, TabMessage, bar};
use crate::theme::UiColors;

const MENU_WIDTH: f32 = 168.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const MENU_ITEMS: f32 = 4.0;
const MENU_PADDING: f32 = 4.0;
const DIALOG_WIDTH: f32 = 380.0;
const TEXT_SIZE: f32 = 13.0;
/// How far beside a divider it can still be grabbed.
const DIVIDER_LEEWAY: f32 = 4.0;
const DIVIDER_WIDTH: f32 = 2.0;

impl App {
    /// Shows `content` over the terminal, keeping the pointer and clicks from reaching it.
    pub(super) fn modal<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        opaque(mouse_area(content).interaction(arrow(self.host)))
    }

    pub fn view(&self, window: iced::window::Id) -> Element<'_, Message> {
        if self
            .dropdown
            .as_ref()
            .is_some_and(|dropdown| dropdown.id == window && dropdown.measuring())
        {
            return Space::new().into();
        }
        let mut layers = vec![self.body()];
        if let Some(menu) = self.menu {
            layers.push(self.menu_layer(menu));
        }
        if let Some(menu) = self.terminal_menu {
            layers.push(self.terminal_menu_layer(menu));
        }
        if let Some(panel) = &self.settings_panel {
            layers.push(self.settings_layer(panel));
        }
        if self.about {
            layers.push(self.about_layer());
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
            width: self.window_size.width,
            sizing: self.settings.tab_sizing,
        };
        let tab_bar = bar::view(&self.tabs, state).map(Message::Tab);
        let terminal: Element<'_, Message> = iced::widget::Column::new()
            .push(self.banner())
            .push(self.search_bar())
            .push(self.terminal())
            .into();
        match self.settings.bar {
            BarPosition::Top => column![tab_bar, terminal].into(),
            BarPosition::Bottom => column![terminal, tab_bar].into(),
            BarPosition::Left => row![tab_bar, terminal].into(),
            BarPosition::Right => row![terminal, tab_bar].into(),
        }
    }

    fn terminal(&self) -> Element<'_, Message> {
        if let Some(tab) = self.tabs.active() {
            let id = tab.id;
            let focused = tab.content.focused();
            let split = tab.content.is_split();
            let colors = self.settings.colors;
            let style = &self.settings.style;
            let min_size = 3.0 * style.cell.height + 2.0 * style.padding;
            let grid = PaneGrid::new(tab.content.grid(), move |pane, session, zoomed| {
                let key = PaneKey { tab: id, pane };
                let view = self
                    .pane(key, session, pane == focused, split)
                    .zoomed(zoomed);
                pane_grid::Content::new(TerminalWidget::new(view))
            })
            .spacing(if split { DIVIDER_WIDTH } else { 0.0 })
            .min_size(min_size)
            .on_click(move |pane| Message::Pane(id, PaneMessage::Focus(pane)))
            .on_resize(DIVIDER_LEEWAY, move |event| {
                Message::Pane(id, PaneMessage::Resized(event))
            })
            .style(move |_| divider_style(colors));
            if !split || tab.content.zoomed() {
                return grid.into();
            }
            // Painted behind the panes, so only the gaps between them show it: a background
            // under the whole grid would show through translucent terminals.
            let dividers = Canvas::new(Dividers {
                layout: tab.content.layout().clone(),
                min_size,
                color: colors.border,
            })
            .width(Fill)
            .height(Fill);
            return stack![dividers, grid].into();
        }
        let message = self.error.as_deref().map_or_else(String::new, |error| {
            format!("Dropshade could not start a shell.\n\n{error}")
        });
        container(text(message).color(self.settings.colors.text))
            .center(Fill)
            .into()
    }

    fn pane<'a>(
        &'a self,
        key: PaneKey,
        session: &'a Session,
        focused: bool,
        split: bool,
    ) -> TerminalView<'a, Message> {
        let dim = if focused || !split {
            0.0
        } else {
            self.settings.unfocused_dim
        };
        TerminalView::new(
            &session.terminal,
            &self.settings.style,
            &self.settings.input,
            move |interaction| Message::Interaction(key, interaction),
        )
        .cursor_visible(session.cursor_on || !session.terminal.cursor_blinks())
        .bell(session.bell)
        .accepts_input(!self.overlay_open())
        .active(focused)
        .split(split)
        .dim(dim)
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
        let position = menu_position(self.settings.bar, menu.position, self.window_size);
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
        let pane = matches!(pending.target, CloseTarget::Pane(_));
        let (noun, confirm) = if pane {
            ("pane", "Close pane")
        } else {
            ("tab", "Close tab")
        };
        let title = if pane {
            "Close this pane?".to_owned()
        } else {
            let label = self
                .tabs
                .get(pending.target.tab())
                .map_or("this tab", |tab| tab.label());
            format!("Close “{label}”?")
        };
        let others = match pending.others {
            0 => String::new(),
            1 => " and 1 more command are".to_owned(),
            n => format!(" and {n} more commands are"),
        };
        let what = match (&pending.process, others.is_empty()) {
            (Some(process), true) => format!("“{process}” is"),
            (Some(process), false) => format!("“{process}”{others}"),
            (None, true) => "A command is".to_owned(),
            (None, false) => format!("A command{others}"),
        };
        let body = format!("{what} still running. Closing the {noun} stops it.");
        let actions = row![
            Space::new().width(Fill),
            button(text("Cancel").size(TEXT_SIZE))
                .padding([7, 14])
                .style(move |_, status| colors.ghost_button(colors.text, status))
                .on_press(Message::Tab(TabMessage::CancelClose)),
            button(text(confirm).size(TEXT_SIZE))
                .padding([7, 14])
                .style(move |_, status| colors.accent_button(status))
                .on_press(Message::Tab(TabMessage::ConfirmClose)),
        ]
        .spacing(8);
        let card = container(
            column![
                text(title).size(16).font(bold),
                text(body).size(TEXT_SIZE).color(colors.muted),
                actions,
            ]
            .spacing(14),
        )
        .padding(20)
        .width(DIALOG_WIDTH)
        .style(move |_| colors.surface_style());
        self.modal(
            container(card)
                .center(Fill)
                .style(move |_| colors.backdrop_style()),
        )
    }
}

/// Draws the dividers of a split tab into the gaps between its panes.
#[derive(Debug)]
struct Dividers {
    layout: pane_grid::Node,
    min_size: f32,
    color: Color,
}

impl canvas::Program<Message> for Dividers {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let splits = self
            .layout
            .split_regions(DIVIDER_WIDTH, self.min_size, bounds.size());
        for (axis, region, ratio) in splits.values() {
            let line = axis.split_line_bounds(*region, *ratio, DIVIDER_WIDTH);
            frame.fill_rectangle(line.position(), line.size(), self.color);
        }
        vec![frame.into_geometry()]
    }
}

/// The dividers light up in the accent color while hovered or dragged.
fn divider_style(colors: UiColors) -> pane_grid::Style {
    let line = pane_grid::Line {
        color: colors.accent,
        width: DIVIDER_WIDTH,
    };
    pane_grid::Style {
        hovered_region: pane_grid::Highlight {
            background: Background::Color(Color {
                a: 0.2,
                ..colors.accent
            }),
            border: Border::default(),
        },
        picked_split: line,
        hovered_split: line,
    }
}

/// The plain pointer over a modal. `iced_layershell` 0.19 shows `Idle` as the busy cursor, and the
/// arrow for shapes it has no mapping for, such as `ContextMenu`.
fn arrow(host: Host) -> mouse::Interaction {
    match host {
        Host::Layer => mouse::Interaction::ContextMenu,
        Host::Shell | Host::Window => mouse::Interaction::Idle,
    }
}

/// Where the tab context menu opens: at the pointer, translated from bar to window coordinates
/// and kept inside the window.
/// The tab bar's top-left corner in the window.
pub(super) fn bar_origin(bar: BarPosition, window: Size) -> Point {
    match bar {
        BarPosition::Top | BarPosition::Left => Point::ORIGIN,
        BarPosition::Bottom => Point::new(0.0, window.height - bar::HORIZONTAL_HEIGHT),
        BarPosition::Right => Point::new(window.width - bar::VERTICAL_WIDTH, 0.0),
    }
}

fn menu_position(bar: BarPosition, pointer: Point, window: Size) -> Point {
    let origin = bar_origin(bar, window);
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
    fn modals_never_ask_layer_shell_for_the_busy_cursor() {
        assert_ne!(arrow(Host::Layer), mouse::Interaction::Idle);
        assert_ne!(
            arrow(Host::Layer),
            mouse::Interaction::None,
            "would show the terminal's"
        );
        assert_eq!(arrow(Host::Window), mouse::Interaction::Idle);
    }

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
    fn bar_origin_is_the_corner_the_bar_starts_at() {
        assert_eq!(bar_origin(BarPosition::Top, WINDOW), Point::ORIGIN);
        assert_eq!(bar_origin(BarPosition::Left, WINDOW), Point::ORIGIN);
        assert_eq!(
            bar_origin(BarPosition::Bottom, WINDOW),
            Point::new(0.0, WINDOW.height - bar::HORIZONTAL_HEIGHT)
        );
        assert_eq!(
            bar_origin(BarPosition::Right, WINDOW),
            Point::new(WINDOW.width - bar::VERTICAL_WIDTH, 0.0)
        );
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

//! The terminal's right-click menu, with a submenu for zoom.

use iced::widget::{Button, Space, button, column, container, mouse_area, pin, row, stack, text};
use iced::{Element, Fill, Point, Size, Task};

use super::zoom::Zoom;
use super::{App, Message};
use crate::theme::UiColors;

const MENU_WIDTH: f32 = 190.0;
const SUBMENU_WIDTH: f32 = 150.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const CARD_PADDING: f32 = 4.0;
const TEXT_SIZE: f32 = 13.0;
/// Items before the one that opens the zoom submenu, and the separators between them.
const ITEMS_BEFORE_ZOOM: f32 = 4.0;
const SEPARATORS_BEFORE_ZOOM: f32 = 2.0;
const ITEMS: f32 = 9.0;
const SEPARATORS: f32 = 4.0;
const SUBMENU_ITEMS: f32 = 3.0;

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    Copy,
    Paste,
    NewTab,
    Find,
    ShowZoomMenu,
    HideZoomMenu,
    ZoomIn,
    ZoomOut,
    ZoomNormal,
    Reset,
    ResetAndClear,
    Settings,
    About,
    Dismiss,
}

/// The open context menu: where it is, and whether its zoom submenu is showing.
#[derive(Debug, Clone, Copy)]
pub(super) struct TerminalMenu {
    pub position: Point,
    pub zoom_open: bool,
}

impl TerminalMenu {
    pub fn at(position: Point) -> Self {
        Self {
            position,
            zoom_open: false,
        }
    }
}

impl App {
    pub(super) fn menu_action(&mut self, action: MenuAction) -> Task<Message> {
        if let MenuAction::ShowZoomMenu | MenuAction::HideZoomMenu = action {
            if let Some(menu) = self.terminal_menu.as_mut() {
                menu.zoom_open = matches!(action, MenuAction::ShowZoomMenu);
            }
            return Task::none();
        }
        self.terminal_menu = None;
        let shortcut = match action {
            MenuAction::Copy => dropshade_term::Shortcut::Copy,
            MenuAction::Paste => dropshade_term::Shortcut::Paste,
            MenuAction::NewTab => dropshade_term::Shortcut::NewTab,
            MenuAction::Find => dropshade_term::Shortcut::Search,
            MenuAction::ZoomIn => return self.set_zoom(self.zoom.zoom_in()),
            MenuAction::ZoomOut => return self.set_zoom(self.zoom.zoom_out()),
            MenuAction::ZoomNormal => return self.set_zoom(Zoom::default()),
            MenuAction::Reset => return self.reset_terminal(false),
            MenuAction::ResetAndClear => return self.reset_terminal(true),
            MenuAction::Settings => return self.open_settings(),
            MenuAction::About => return self.open_about(),
            MenuAction::ShowZoomMenu | MenuAction::HideZoomMenu | MenuAction::Dismiss => {
                return Task::none();
            }
        };
        self.update(Message::Interaction(dropshade_term::Interaction::Shortcut(
            shortcut,
        )))
    }

    fn set_zoom(&mut self, zoom: Zoom) -> Task<Message> {
        if zoom != self.zoom {
            self.zoom = zoom;
            self.apply_settings();
        }
        Task::none()
    }

    fn reset_terminal(&mut self, clear: bool) -> Task<Message> {
        if let Some(tab) = self.tabs.active_mut() {
            if clear {
                tab.content.terminal.reset_and_clear();
            } else {
                tab.content.terminal.reset();
            }
        }
        Task::none()
    }

    pub(super) fn terminal_menu_layer(&self, menu: TerminalMenu) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let has_selection = self
            .tabs
            .active()
            .is_some_and(|tab| tab.content.terminal.selection_text().is_some());
        let item = |label: &'static str, action: Option<MenuAction>| {
            mouse_area(item_button(colors, label, action))
                .on_enter(Message::Menu(MenuAction::HideZoomMenu))
        };
        let items = column![
            item("Copy", has_selection.then_some(MenuAction::Copy)),
            item("Paste", Some(MenuAction::Paste)),
            separator(colors),
            item("New Tab", Some(MenuAction::NewTab)),
            item("Find…", Some(MenuAction::Find)),
            separator(colors),
            zoom_item(colors, menu.zoom_open),
            separator(colors),
            item("Reset", Some(MenuAction::Reset)),
            item("Reset and Clean", Some(MenuAction::ResetAndClear)),
            separator(colors),
            item("Settings…", Some(MenuAction::Settings)),
            item("About Dropshade", Some(MenuAction::About)),
        ];
        let anchor = menu_position(menu.position, self.window_size);
        let card = container(items)
            .padding(CARD_PADDING)
            .width(MENU_WIDTH)
            .style(move |_| colors.surface_style());
        let dismiss = mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Menu(MenuAction::Dismiss))
            .on_right_press(Message::Menu(MenuAction::Dismiss));
        let mut layers = vec![dismiss.into(), pin(card).position(anchor).into()];
        if menu.zoom_open {
            let position = submenu_position(anchor, self.window_size);
            layers.push(pin(self.zoom_submenu()).position(position).into());
        }
        stack(layers).into()
    }

    fn zoom_submenu(&self) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let zoom = self.zoom;
        let items = column![
            item_button(
                colors,
                "Zoom In",
                zoom.can_zoom_in().then_some(MenuAction::ZoomIn)
            ),
            item_button(
                colors,
                "Zoom Out",
                zoom.can_zoom_out().then_some(MenuAction::ZoomOut)
            ),
            item_button(
                colors,
                "Normal Size",
                (!zoom.is_normal()).then_some(MenuAction::ZoomNormal)
            ),
        ];
        container(items)
            .padding(CARD_PADDING)
            .width(SUBMENU_WIDTH)
            .style(move |_| colors.surface_style())
            .into()
    }
}

fn item_button(
    colors: UiColors,
    label: &'static str,
    action: Option<MenuAction>,
) -> Button<'static, Message> {
    let color = if action.is_some() {
        colors.text
    } else {
        colors.muted
    };
    button(text(label).size(TEXT_SIZE))
        .width(Fill)
        .height(MENU_ITEM_HEIGHT)
        .padding([7, 10])
        .style(move |_, status| colors.ghost_button(color, status))
        .on_press_maybe(action.map(Message::Menu))
}

/// The entry that opens the zoom submenu, highlighted while the submenu is open.
fn zoom_item(colors: UiColors, open: bool) -> Element<'static, Message> {
    let label = row![
        text("Zoom").size(TEXT_SIZE),
        Space::new().width(Fill),
        text("›").size(TEXT_SIZE),
    ];
    let button = button(label)
        .width(Fill)
        .height(MENU_ITEM_HEIGHT)
        .padding([7, 10])
        .style(move |_, status| {
            let status = if open {
                button::Status::Hovered
            } else {
                status
            };
            colors.ghost_button(colors.text, status)
        })
        .on_press(Message::Menu(MenuAction::ShowZoomMenu));
    mouse_area(button)
        .on_enter(Message::Menu(MenuAction::ShowZoomMenu))
        .into()
}

fn separator(colors: UiColors) -> Element<'static, Message> {
    container(
        container(Space::new())
            .width(Fill)
            .height(1)
            .style(move |_| container::Style::default().background(colors.border)),
    )
    .padding([4, 6])
    .height(SEPARATOR_HEIGHT)
    .into()
}

/// Where the menu opens: at the pointer, kept inside the window.
fn menu_position(pointer: Point, window: Size) -> Point {
    let height = ITEMS * MENU_ITEM_HEIGHT + SEPARATORS * SEPARATOR_HEIGHT + 2.0 * CARD_PADDING;
    Point::new(
        pointer.x.clamp(0.0, (window.width - MENU_WIDTH).max(0.0)),
        pointer.y.clamp(0.0, (window.height - height).max(0.0)),
    )
}

/// Where the zoom submenu opens: beside the menu, level with the zoom item, on the left when
/// there is no room on the right.
fn submenu_position(menu: Point, window: Size) -> Point {
    let height = SUBMENU_ITEMS * MENU_ITEM_HEIGHT + 2.0 * CARD_PADDING;
    let level = menu.y
        + CARD_PADDING
        + ITEMS_BEFORE_ZOOM * MENU_ITEM_HEIGHT
        + SEPARATORS_BEFORE_ZOOM * SEPARATOR_HEIGHT;
    let right = menu.x + MENU_WIDTH;
    let x = if right + SUBMENU_WIDTH <= window.width {
        right
    } else {
        (menu.x - SUBMENU_WIDTH).max(0.0)
    };
    Point::new(x, level.min((window.height - height).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1000.0, 600.0);

    #[test]
    fn the_menu_opens_at_the_pointer() {
        let at = menu_position(Point::new(120.0, 80.0), WINDOW);
        assert_eq!(at, Point::new(120.0, 80.0));
    }

    #[test]
    fn the_menu_stays_inside_the_window() {
        let at = menu_position(Point::new(995.0, 595.0), WINDOW);
        let height = ITEMS * MENU_ITEM_HEIGHT + SEPARATORS * SEPARATOR_HEIGHT + 2.0 * CARD_PADDING;
        assert_eq!(
            at,
            Point::new(WINDOW.width - MENU_WIDTH, WINDOW.height - height)
        );
    }

    #[test]
    fn tiny_windows_do_not_produce_negative_positions() {
        let at = menu_position(Point::new(10.0, 10.0), Size::new(50.0, 50.0));
        assert_eq!(at, Point::ORIGIN);
    }

    #[test]
    fn the_submenu_opens_to_the_right_level_with_the_zoom_item() {
        let at = submenu_position(Point::new(100.0, 50.0), WINDOW);
        let zoom_item = 50.0
            + CARD_PADDING
            + ITEMS_BEFORE_ZOOM * MENU_ITEM_HEIGHT
            + SEPARATORS_BEFORE_ZOOM * SEPARATOR_HEIGHT;
        assert_eq!(at, Point::new(100.0 + MENU_WIDTH, zoom_item));
    }

    #[test]
    fn the_submenu_flips_to_the_left_at_the_right_edge() {
        let menu = Point::new(WINDOW.width - MENU_WIDTH, 50.0);
        let at = submenu_position(menu, WINDOW);
        assert_eq!(at.x, menu.x - SUBMENU_WIDTH);
    }

    #[test]
    fn the_submenu_stays_inside_the_window_at_the_bottom() {
        let menu = menu_position(Point::new(100.0, 595.0), WINDOW);
        let at = submenu_position(menu, WINDOW);
        let height = SUBMENU_ITEMS * MENU_ITEM_HEIGHT + 2.0 * CARD_PADDING;
        assert!(at.y + height <= WINDOW.height);
    }
}

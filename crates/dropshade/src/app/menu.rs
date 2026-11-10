//! The terminal's right-click menu.

use iced::widget::{Space, button, column, container, mouse_area, pin, stack, text};
use iced::{Element, Fill, Point, Task};

use super::{App, Message};

const MENU_WIDTH: f32 = 180.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const TEXT_SIZE: f32 = 13.0;

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    Copy,
    Paste,
    NewTab,
    Find,
    Settings,
    Dismiss,
}

impl App {
    pub(super) fn menu_action(&mut self, action: MenuAction) -> Task<Message> {
        self.terminal_menu = None;
        let shortcut = match action {
            MenuAction::Copy => dropshade_term::Shortcut::Copy,
            MenuAction::Paste => dropshade_term::Shortcut::Paste,
            MenuAction::NewTab => dropshade_term::Shortcut::NewTab,
            MenuAction::Find => dropshade_term::Shortcut::Search,
            MenuAction::Settings => return self.open_settings(),
            MenuAction::Dismiss => return Task::none(),
        };
        self.update(Message::Interaction(dropshade_term::Interaction::Shortcut(
            shortcut,
        )))
    }

    pub(super) fn terminal_menu_layer(&self, position: Point) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let has_selection = self
            .tabs
            .active()
            .is_some_and(|tab| tab.content.terminal.selection_text().is_some());
        let item = |label: &'static str, action: Option<MenuAction>| {
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
        };
        let items = column![
            item("Copy", has_selection.then_some(MenuAction::Copy)),
            item("Paste", Some(MenuAction::Paste)),
            item("New Tab", Some(MenuAction::NewTab)),
            item("Find…", Some(MenuAction::Find)),
            item("Settings…", Some(MenuAction::Settings)),
        ];
        #[allow(clippy::cast_precision_loss)]
        let height = 5.0 * MENU_ITEM_HEIGHT + 8.0;
        let anchor = Point::new(
            position
                .x
                .clamp(0.0, (self.window_size.width - MENU_WIDTH).max(0.0)),
            position
                .y
                .clamp(0.0, (self.window_size.height - height).max(0.0)),
        );
        let card = container(items)
            .padding(4)
            .width(MENU_WIDTH)
            .style(move |_| colors.surface_style());
        let dismiss = mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Menu(MenuAction::Dismiss))
            .on_right_press(Message::Menu(MenuAction::Dismiss));
        stack![dismiss, pin(card).position(anchor)].into()
    }
}

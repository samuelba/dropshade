//! The tab bar: one entry per tab with rename-in-place, close buttons and drag reordering.

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Row, Space, button, column, container, mouse_area, row, text, text_input,
};
use iced::{Color, Element, Fill, Length, Padding, Point};

use super::model::{Tab, TabId, Tabs};
use crate::theme::UiColors;

pub const RENAME_INPUT: &str = "tab-rename";
pub const HORIZONTAL_HEIGHT: f32 = 38.0;
pub const VERTICAL_WIDTH: f32 = 200.0;
const MAX_TAB_WIDTH: f32 = 240.0;
const MIN_TAB_WIDTH: f32 = 72.0;
const NEW_TAB_BUTTON_WIDTH: f32 = 32.0;
const LABEL_SIZE: f32 = 13.0;
const ACCENT_THICKNESS: f32 = 2.0;
const SPACING: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl BarPosition {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

#[derive(Debug, Clone)]
pub enum TabMessage {
    New,
    Pressed(TabId),
    Entered(TabId),
    Exited(TabId),
    Released,
    RequestClose(TabId),
    StartRename(TabId),
    RenameInput(String),
    CommitRename,
    OpenMenu(TabId),
    CloseMenu,
    MoveLeft(TabId),
    MoveRight(TabId),
    /// Pointer position over the bar, relative to the bar; anchors the context menu.
    PointerMoved(Point),
    ConfirmClose,
    CancelClose,
}

/// Transient bar state owned by the application.
#[derive(Debug, Clone, Copy)]
pub struct BarState<'a> {
    pub position: BarPosition,
    pub hovered: Option<TabId>,
    pub renaming: Option<(TabId, &'a str)>,
    pub colors: UiColors,
    /// Width available to a horizontal bar, used to size the tabs.
    pub width: f32,
    pub tab_count: usize,
}

/// Width of each tab in a horizontal bar: tabs share the space, between a readable minimum and a
/// comfortable maximum.
#[allow(clippy::cast_precision_loss)]
pub fn tab_width(bar_width: f32, count: usize) -> f32 {
    let count = count.max(1) as f32;
    let available = bar_width - 2.0 * SPACING - NEW_TAB_BUTTON_WIDTH - count * SPACING;
    (available / count).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
}

pub fn view<'a, T>(tabs: &'a Tabs<T>, state: BarState<'a>) -> Element<'a, TabMessage> {
    let active = tabs.active_id();
    let entries = tabs
        .iter()
        .map(|tab| entry(tab, Some(tab.id) == active, state));
    let new_tab = icon_button("+", state.colors.muted, state.colors).on_press(TabMessage::New);

    let content: Element<'a, TabMessage> = if state.position.is_horizontal() {
        Row::with_children(entries)
            .push(new_tab)
            .spacing(SPACING)
            .padding(SPACING)
            .height(Length::Fixed(HORIZONTAL_HEIGHT))
            .align_y(iced::Center)
            .into()
    } else {
        Column::with_children(entries)
            .push(new_tab)
            .spacing(SPACING)
            .padding(SPACING)
            .width(Length::Fixed(VERTICAL_WIDTH))
            .into()
    };

    let bar = container(content).style(move |_| state.colors.bar_style());
    let bar = if state.position.is_horizontal() {
        bar.width(Fill)
    } else {
        bar.height(Fill)
    };
    mouse_area(bar).on_move(TabMessage::PointerMoved).into()
}

fn entry<'a, T>(tab: &'a Tab<T>, active: bool, state: BarState<'a>) -> Element<'a, TabMessage> {
    let colors = state.colors;
    let hovered = state.hovered == Some(tab.id);
    let label: Element<'a, TabMessage> = match state.renaming {
        Some((id, draft)) if id == tab.id => text_input("Tab name", draft)
            .id(Id::new(RENAME_INPUT))
            .on_input(TabMessage::RenameInput)
            .on_submit(TabMessage::CommitRename)
            .size(LABEL_SIZE)
            .padding([2, 6])
            .width(Fill)
            .into(),
        _ => text(tab.label())
            .size(LABEL_SIZE)
            .wrapping(Wrapping::None)
            .width(Fill)
            .into(),
    };
    let close_color = if active || hovered {
        colors.muted
    } else {
        Color::TRANSPARENT
    };
    let close = icon_button("×", close_color, colors).on_press(TabMessage::RequestClose(tab.id));
    let body = row![label, close].spacing(SPACING).align_y(iced::Center);

    let indicator = container(Space::new()).style(move |_| {
        container::Style::default().background(if active {
            colors.accent
        } else {
            Color::TRANSPARENT
        })
    });
    let content: Element<'a, TabMessage> = if state.position.is_horizontal() {
        column![
            container(body)
                .padding(Padding {
                    top: 4.0,
                    right: 6.0,
                    bottom: 2.0,
                    left: 10.0
                })
                .height(Fill)
                .center_y(Fill),
            indicator.width(Fill).height(ACCENT_THICKNESS),
        ]
        .into()
    } else {
        row![
            indicator.width(ACCENT_THICKNESS).height(Fill),
            container(body)
                .padding(Padding {
                    top: 6.0,
                    right: 6.0,
                    bottom: 6.0,
                    left: 8.0
                })
                .width(Fill),
        ]
        .height(Length::Shrink)
        .into()
    };

    let tab_box = container(content)
        .clip(true)
        .style(move |_| colors.tab_style(active, hovered));
    let tab_box = if state.position.is_horizontal() {
        tab_box
            .width(tab_width(state.width, state.tab_count))
            .height(Fill)
    } else {
        tab_box.width(Fill)
    };

    mouse_area(tab_box)
        .on_press(TabMessage::Pressed(tab.id))
        .on_release(TabMessage::Released)
        .on_double_click(TabMessage::StartRename(tab.id))
        .on_middle_press(TabMessage::RequestClose(tab.id))
        .on_right_press(TabMessage::OpenMenu(tab.id))
        .on_enter(TabMessage::Entered(tab.id))
        .on_exit(TabMessage::Exited(tab.id))
        .into()
}

fn icon_button(label: &str, color: Color, colors: UiColors) -> button::Button<'_, TabMessage> {
    button(text(label).size(LABEL_SIZE + 3.0).center())
        .padding([0, 7])
        .style(move |_, status| colors.ghost_button(color, status))
}

#[cfg(test)]
mod tests {

    #[test]
    fn tabs_share_the_bar_within_limits() {
        assert!((super::tab_width(1600.0, 2) - super::MAX_TAB_WIDTH).abs() < f32::EPSILON);
        let crowded = super::tab_width(800.0, 6);
        assert!(crowded > super::MIN_TAB_WIDTH && crowded < super::MAX_TAB_WIDTH);
        assert!((super::tab_width(300.0, 40) - super::MIN_TAB_WIDTH).abs() < f32::EPSILON);
        assert!(super::tab_width(1600.0, 0) <= super::MAX_TAB_WIDTH);
    }
}

//! The tab bar: one entry per tab with rename-in-place, close buttons and drag reordering.

use dropshade_term::metrics;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Row, Space, button, column, container, mouse_area, row, text, text_input,
};
use iced::{Color, Element, Fill, Font, Length, Padding, Point};

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
const CLOSE_LABEL: &str = "×";
const CLOSE_PADDING: f32 = 7.0;
const LABEL_PADDING_LEFT: f32 = 10.0;
const LABEL_PADDING_RIGHT: f32 = 6.0;
const RENAME_PLACEHOLDER: &str = "Tab name";
const RENAME_PADDING: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

/// How wide the tabs of a horizontal bar are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabSizing {
    /// Tabs share the bar equally.
    #[default]
    Fixed,
    /// Each tab is as wide as its label, up to the same maximum.
    Fit,
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
    pub sizing: TabSizing,
}

/// Width of each tab in a horizontal bar: tabs share the space, between a readable minimum and a
/// comfortable maximum.
#[allow(clippy::cast_precision_loss)]
pub fn tab_width(bar_width: f32, count: usize) -> f32 {
    (available_width(bar_width, count) / count.max(1) as f32).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
}

/// Widths for tabs that want `natural` widths: each gets what it wants within the limits, and
/// when the bar is too narrow the space is shared so short tabs keep their size and long tabs
/// split the rest equally.
#[allow(clippy::cast_precision_loss)]
pub fn fitted_widths(bar_width: f32, natural: &[f32]) -> Vec<f32> {
    let wanted: Vec<f32> = natural
        .iter()
        .map(|width| width.clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH))
        .collect();
    let mut order: Vec<usize> = (0..wanted.len()).collect();
    order.sort_by(|&a, &b| wanted[a].total_cmp(&wanted[b]));
    let mut remaining = available_width(bar_width, wanted.len());
    let mut widths = vec![0.0; wanted.len()];
    for (placed, &index) in order.iter().enumerate() {
        let share = remaining / (wanted.len() - placed) as f32;
        let width = wanted[index].min(share).max(MIN_TAB_WIDTH);
        widths[index] = width;
        remaining -= width;
    }
    widths
}

#[allow(clippy::cast_precision_loss)]
fn available_width(bar_width: f32, count: usize) -> f32 {
    let count = count.max(1) as f32;
    bar_width - 2.0 * SPACING - NEW_TAB_BUTTON_WIDTH - count * SPACING
}

/// Width a horizontal tab needs to show `label` in full, close button included.
fn natural_width(label: &str) -> f32 {
    let text = metrics::text_width(label, Font::DEFAULT, LABEL_SIZE);
    let close = metrics::text_width(CLOSE_LABEL, Font::DEFAULT, LABEL_SIZE + 3.0);
    (LABEL_PADDING_LEFT + text + SPACING + close + 2.0 * CLOSE_PADDING + LABEL_PADDING_RIGHT).ceil()
        + 1.0
}

fn tab_widths<T>(tabs: &Tabs<T>, state: BarState<'_>) -> Vec<f32> {
    match state.sizing {
        TabSizing::Fixed => vec![tab_width(state.width, tabs.len()); tabs.len()],
        TabSizing::Fit => {
            let natural: Vec<f32> = tabs
                .iter()
                .map(|tab| match state.renaming {
                    Some((id, draft)) if id == tab.id => {
                        let shown = if draft.is_empty() {
                            RENAME_PLACEHOLDER
                        } else {
                            draft
                        };
                        natural_width(shown) + 2.0 * RENAME_PADDING
                    }
                    _ => natural_width(tab.label()),
                })
                .collect();
            fitted_widths(state.width, &natural)
        }
    }
}

pub fn view<'a, T>(tabs: &'a Tabs<T>, state: BarState<'a>) -> Element<'a, TabMessage> {
    let active = tabs.active_id();
    let widths = if state.position.is_horizontal() {
        tab_widths(tabs, state)
    } else {
        vec![VERTICAL_WIDTH; tabs.len()]
    };
    let entries = tabs
        .iter()
        .zip(widths)
        .map(|(tab, width)| entry(tab, Some(tab.id) == active, width, state));
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

fn entry<'a, T>(
    tab: &'a Tab<T>,
    active: bool,
    width: f32,
    state: BarState<'a>,
) -> Element<'a, TabMessage> {
    let colors = state.colors;
    let hovered = state.hovered == Some(tab.id);
    let label: Element<'a, TabMessage> = match state.renaming {
        Some((id, draft)) if id == tab.id => text_input("Tab name", draft)
            .id(Id::new(RENAME_INPUT))
            .on_input(TabMessage::RenameInput)
            .on_submit(TabMessage::CommitRename)
            .size(LABEL_SIZE)
            .padding([2.0, RENAME_PADDING])
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
    let close =
        icon_button(CLOSE_LABEL, close_color, colors).on_press(TabMessage::RequestClose(tab.id));
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
                    right: LABEL_PADDING_RIGHT,
                    bottom: 2.0,
                    left: LABEL_PADDING_LEFT
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
        tab_box.width(width).height(Fill)
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
        .padding([0.0, CLOSE_PADDING])
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

    #[test]
    fn fitted_tabs_follow_their_labels_within_limits() {
        let widths = super::fitted_widths(1600.0, &[100.0, 30.0, 900.0]);
        assert_eq!(
            widths,
            vec![100.0, super::MIN_TAB_WIDTH, super::MAX_TAB_WIDTH]
        );
    }

    #[test]
    fn crowded_fitted_tabs_keep_short_ones_and_share_the_rest() {
        let natural = [80.0, 200.0, 200.0];
        let widths = super::fitted_widths(400.0, &natural);
        let available = super::available_width(400.0, natural.len());
        assert!((widths[0] - 80.0).abs() < f32::EPSILON);
        assert!((widths[1] - widths[2]).abs() < f32::EPSILON);
        assert!((widths.iter().sum::<f32>() - available).abs() < 0.01);
    }

    #[test]
    fn longer_labels_need_wider_tabs() {
        assert!(super::natural_width("a much longer tab name") > super::natural_width("sh"));
    }
}

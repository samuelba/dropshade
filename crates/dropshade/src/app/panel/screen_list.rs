//! The settings panel's ordered list of screens for `window.monitor`: the first connected one is
//! used, so one list covers every desk the computer is plugged into.

use dropshade_config::edit::SettingValue;
use dropshade_config::{Monitor, Monitors};
use iced::widget::{Column, button, column, pick_list, row, text};
use iced::{Element, Fill, Length};

use super::SettingsMessage;
use super::view::{Ctx, LABEL_WIDTH, TEXT_SIZE, hint, settings};
use crate::app::{App, Message};
use crate::screens::{self, Screen, ScreenChoice};
use crate::theme::UiColors;

/// A change to the list.
#[derive(Debug, Clone)]
pub enum ScreenEdit {
    Add(ScreenChoice),
    Remove(usize),
    /// Swaps the entry with the one above it.
    Raise(usize),
}

/// The list after `edit`, or `None` when it would be empty.
pub fn edited(current: &Monitors, edit: ScreenEdit) -> Option<Monitors> {
    let mut entries = current.entries().to_vec();
    match edit {
        ScreenEdit::Add(choice) => entries.push(choice.monitor),
        ScreenEdit::Remove(index) if index < entries.len() => {
            entries.remove(index);
        }
        ScreenEdit::Raise(index) if index > 0 && index < entries.len() => {
            entries.swap(index - 1, index);
        }
        ScreenEdit::Remove(_) | ScreenEdit::Raise(_) => {}
    }
    Monitors::new(entries)
}

impl App {
    pub(super) fn edit_screens(&self, edit: ScreenEdit) -> Result<(), String> {
        let Some(monitors) = edited(&self.config.window.monitor, edit) else {
            return self.remove("window", "monitor");
        };
        let mut names: Vec<String> = monitors
            .entries()
            .iter()
            .map(|monitor| monitor.as_str().to_owned())
            .collect();
        let value = if names.len() == 1 {
            SettingValue::Text(names.remove(0))
        } else {
            SettingValue::List(names)
        };
        self.write("window", "monitor", value)
    }
}

/// The position of the entry that applies with these screens connected.
fn in_use(monitors: &Monitors, screens: &[Screen]) -> Option<usize> {
    monitors.entries().iter().position(|entry| match entry {
        Monitor::Pointer | Monitor::Primary => true,
        Monitor::Named(_) => screens::find(entry, screens).is_some(),
    })
}

/// Where a named entry is right now, such as "on DP-4", or that it is not connected.
fn whereabouts(entry: &Monitor, screens: &[Screen]) -> String {
    match (entry, screens::find(entry, screens)) {
        (Monitor::Pointer | Monitor::Primary, _) => String::new(),
        (Monitor::Named(_), None) => "not connected".to_owned(),
        (Monitor::Named(name), Some(screen)) if name.eq_ignore_ascii_case(&screen.connector) => {
            screen.product.clone()
        }
        (Monitor::Named(_), Some(screen)) => format!("on {}", screen.connector),
    }
}

/// "on DP-4 · in use" and the like; empty while the screens are unknown.
fn status(entry: &Monitor, screens: Option<&[Screen]>, active: bool) -> String {
    let Some(screens) = screens else {
        return String::new();
    };
    let place = whereabouts(entry, screens);
    match (active, place.is_empty()) {
        (true, true) => "in use".to_owned(),
        (true, false) => format!("{place} · in use"),
        (false, _) => place,
    }
}

pub(super) fn screen_list(ctx: Ctx<'_>) -> Element<'static, Message> {
    let colors = ctx.colors;
    let monitors = &ctx.config.window.monitor;
    let screens = ctx.panel.screens.as_deref();
    let active = screens.and_then(|screens| in_use(monitors, screens));
    let count = monitors.entries().len();
    let rows = monitors.entries().iter().enumerate().map(|(index, entry)| {
        let status = status(entry, screens, active == Some(index));
        entry_row(index, count, entry, status, colors)
    });
    let add = pick_list(
        screens::choices(screens.unwrap_or_default(), monitors),
        None::<ScreenChoice>,
        |choice| settings(SettingsMessage::EditScreens(ScreenEdit::Add(choice))),
    )
    .placeholder("Add a screen…")
    .text_size(TEXT_SIZE);
    let note = if screens.is_some() && active.is_none() {
        "None of these is connected, so the screen under the mouse pointer is used."
    } else {
        "The first connected screen is used, so one list covers home, office and laptop. \
         Pointer and primary always apply, so put them last."
    };
    row![
        text("Screen")
            .size(TEXT_SIZE)
            .width(Length::Fixed(LABEL_WIDTH)),
        column![
            Column::with_children(rows).spacing(4),
            add,
            hint(note, colors)
        ]
        .spacing(8)
        .width(Fill),
    ]
    .spacing(10)
    .into()
}

fn entry_row(
    index: usize,
    count: usize,
    entry: &Monitor,
    status: String,
    colors: UiColors,
) -> Element<'static, Message> {
    let edit = |edit| settings(SettingsMessage::EditScreens(edit));
    let icon = |label: &'static str, message: Option<Message>| {
        button(text(label).size(TEXT_SIZE))
            .padding([3, 8])
            .style(move |_, status| colors.ghost_button(colors.text, status))
            .on_press_maybe(message)
    };
    let raise = (index > 0).then(|| edit(ScreenEdit::Raise(index)));
    let lower = (index + 1 < count).then(|| edit(ScreenEdit::Raise(index + 1)));
    row![
        text(format!("{}. {}", index + 1, screens::label(entry)))
            .size(TEXT_SIZE)
            .width(Fill),
        text(status).size(TEXT_SIZE).color(colors.muted),
        icon("↑", raise),
        icon("↓", lower),
        icon("×", Some(edit(ScreenEdit::Remove(index)))),
    ]
    .spacing(6)
    .align_y(iced::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitors(names: &[&str]) -> Monitors {
        Monitors::new(names.iter().map(|name| Monitor::from(*name)).collect()).expect("entries")
    }

    fn screen(connector: &str, product: &str) -> Screen {
        Screen {
            connector: connector.into(),
            product: product.into(),
            primary: false,
        }
    }

    fn choice(name: &str) -> ScreenChoice {
        screens::choices(&[screen(name, "")], &Monitors::default())
            .into_iter()
            .find(|choice| choice.monitor == Monitor::Named(name.into()))
            .expect("offered")
    }

    #[test]
    fn screens_are_added_at_the_end() {
        let list = edited(&monitors(&["DP-1"]), ScreenEdit::Add(choice("eDP-1")));
        assert_eq!(list, Some(monitors(&["DP-1", "eDP-1"])));
    }

    #[test]
    fn raising_swaps_with_the_entry_above() {
        let list = monitors(&["DP-1", "DP-2", "pointer"]);
        assert_eq!(
            edited(&list, ScreenEdit::Raise(2)),
            Some(monitors(&["DP-1", "pointer", "DP-2"]))
        );
        assert_eq!(edited(&list, ScreenEdit::Raise(0)), Some(list.clone()));
        assert_eq!(edited(&list, ScreenEdit::Raise(9)), Some(list));
    }

    #[test]
    fn removing_the_last_entry_empties_the_list() {
        assert_eq!(edited(&monitors(&["DP-1"]), ScreenEdit::Remove(0)), None);
        assert_eq!(
            edited(&monitors(&["DP-1", "DP-2"]), ScreenEdit::Remove(5)),
            Some(monitors(&["DP-1", "DP-2"]))
        );
    }

    #[test]
    fn the_entry_in_use_is_the_first_connected_one() {
        let list = monitors(&["LEN P27h-10", "DELL U2719DC", "pointer"]);
        let home = [screen("DP-1", "DELL U2719DC")];
        assert_eq!(in_use(&list, &home), Some(1));
        assert_eq!(in_use(&monitors(&["DP-9"]), &home), None);
    }

    #[test]
    fn entries_say_where_their_screen_is() {
        let screens = [screen("DP-4", "LEN P27h-10")];
        assert_eq!(
            whereabouts(&Monitor::from("LEN P27h-10"), &screens),
            "on DP-4"
        );
        assert_eq!(whereabouts(&Monitor::from("DP-4"), &screens), "LEN P27h-10");
        assert_eq!(
            whereabouts(&Monitor::from("DP-1"), &screens),
            "not connected"
        );
        assert_eq!(whereabouts(&Monitor::Pointer, &screens), "");
    }

    #[test]
    fn the_status_marks_the_entry_in_use() {
        let screens = [screen("DP-4", "LEN P27h-10")];
        let len = Monitor::from("LEN P27h-10");
        assert_eq!(status(&len, Some(&screens), true), "on DP-4 · in use");
        assert_eq!(status(&Monitor::Pointer, Some(&screens), true), "in use");
        assert_eq!(status(&len, None, true), "");
    }
}

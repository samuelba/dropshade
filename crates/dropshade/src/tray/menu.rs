//! The tray icon's menu, served as `com.canonical.dbusmenu`. It never changes, so the revision
//! stays at 1 and the menu sends no update signals.

#![allow(clippy::used_underscore_binding)]

use std::collections::HashMap;

use iced::futures::channel::mpsc::UnboundedSender;
use serde::{Deserialize, Serialize};
use zbus::interface;
use zbus::{OwnedVariant, Type, Variant};

use crate::ipc::Command;

pub const PATH: &str = "/MenuBar";

const ROOT: i32 = 0;
const REVISION: u32 = 1;

/// One menu entry and, for the root, the entries under it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, Variant, OwnedVariant)]
pub struct Layout {
    id: i32,
    properties: HashMap<String, OwnedVariant>,
    children: Vec<OwnedVariant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Toggle,
    Separator,
    Settings,
    About,
}

const ENTRIES: [Entry; 4] = [
    Entry::Toggle,
    Entry::Separator,
    Entry::Settings,
    Entry::About,
];

impl Entry {
    fn from_id(id: i32) -> Option<Self> {
        let index = usize::try_from(id).ok()?.checked_sub(1)?;
        ENTRIES.get(index).copied()
    }

    fn id(self) -> i32 {
        let index = ENTRIES.iter().position(|entry| *entry == self).unwrap_or(0);
        i32::try_from(index).unwrap_or(0) + 1
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Self::Toggle => Some("Show/Hide Dropshade"),
            Self::Separator => None,
            Self::Settings => Some("Settings…"),
            Self::About => Some("About Dropshade"),
        }
    }

    fn command(self) -> Option<Command> {
        match self {
            Self::Toggle => Some(Command::Toggle),
            Self::Separator => None,
            Self::Settings => Some(Command::OpenSettings),
            Self::About => Some(Command::OpenAbout),
        }
    }

    fn properties(self) -> HashMap<String, OwnedVariant> {
        match self.label() {
            Some(label) => HashMap::from([property("label", label)]),
            None => HashMap::from([property("type", "separator")]),
        }
    }
}

fn property<'a>(name: &str, value: impl Into<Variant<'a>>) -> (String, OwnedVariant) {
    let value =
        OwnedVariant::try_from(value.into()).expect("plain values have no file descriptors");
    (name.to_owned(), value)
}

fn root_properties() -> HashMap<String, OwnedVariant> {
    HashMap::from([property("children-display", "submenu")])
}

/// The properties of entry `id`, or `None` for an id the menu does not have.
fn properties_of(id: i32) -> Option<HashMap<String, OwnedVariant>> {
    if id == ROOT {
        return Some(root_properties());
    }
    Entry::from_id(id).map(Entry::properties)
}

/// The layout from `parent` down, with its children when `depth` allows any.
pub fn layout(parent: i32, depth: i32) -> Option<Layout> {
    let properties = properties_of(parent)?;
    let children = if parent == ROOT && depth != 0 {
        ENTRIES
            .iter()
            .map(|entry| {
                let child = Layout {
                    id: entry.id(),
                    properties: entry.properties(),
                    children: Vec::new(),
                };
                OwnedVariant::try_from(Variant::from(child)).expect("layouts hold plain values")
            })
            .collect()
    } else {
        Vec::new()
    };
    Some(Layout {
        id: parent,
        properties,
        children,
    })
}

/// What clicking entry `id` asks the app to do.
pub fn command_for(id: i32, event: &str) -> Option<Command> {
    (event == "clicked")
        .then(|| Entry::from_id(id))
        .flatten()
        .and_then(Entry::command)
}

fn unknown(id: i32) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(format!("no menu entry {id}"))
}

#[derive(Debug)]
pub struct Menu {
    commands: UnboundedSender<Command>,
}

impl Menu {
    pub fn new(commands: UnboundedSender<Command>) -> Self {
        Self { commands }
    }

    fn dispatch(&self, id: i32, event: &str) {
        if let Some(command) = command_for(id, event) {
            let _ = self.commands.unbounded_send(command);
        }
    }
}

#[allow(clippy::unused_self)]
#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, Layout)> {
        let layout = layout(parent_id, recursion_depth).ok_or_else(|| unknown(parent_id))?;
        Ok((REVISION, layout))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedVariant>)> {
        let ids = if ids.is_empty() {
            std::iter::once(ROOT)
                .chain(ENTRIES.iter().map(|entry| entry.id()))
                .collect()
        } else {
            ids
        };
        ids.into_iter()
            .filter_map(|id| properties_of(id).map(|properties| (id, properties)))
            .collect()
    }

    fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<OwnedVariant> {
        properties_of(id)
            .ok_or_else(|| unknown(id))?
            .remove(name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name}")))
    }

    fn event(&self, id: i32, event_id: &str, _data: Variant<'_>, _timestamp: u32) {
        self.dispatch(id, event_id);
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedVariant, u32)>) -> Vec<i32> {
        let mut missing = Vec::new();
        for (id, event, _data, _timestamp) in events {
            if properties_of(id).is_none() {
                missing.push(id);
            }
            self.dispatch(id, &event);
        }
        missing
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &'static str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &'static str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use iced::futures::channel::mpsc;

    use super::*;

    fn label(properties: &HashMap<String, OwnedVariant>) -> Option<String> {
        properties
            .get("label")
            .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
    }

    #[test]
    fn the_root_lists_toggle_separator_settings_and_about() {
        let root = layout(ROOT, -1).expect("root");
        let children: Vec<Layout> = root
            .children
            .into_iter()
            .map(|child| Layout::try_from(child).expect("a layout"))
            .collect();
        let labels: Vec<Option<String>> = children
            .iter()
            .map(|child| label(&child.properties))
            .collect();
        assert_eq!(
            labels,
            [
                Some("Show/Hide Dropshade".to_owned()),
                None,
                Some("Settings…".to_owned()),
                Some("About Dropshade".to_owned()),
            ]
        );
        let ids: Vec<i32> = children.iter().map(|child| child.id).collect();
        assert_eq!(ids, [1, 2, 3, 4]);
        assert!(children[1].properties.contains_key("type"));
    }

    #[test]
    fn a_depth_of_zero_leaves_out_the_children() {
        assert_eq!(layout(ROOT, 0).expect("root").children, []);
    }

    #[test]
    fn unknown_entries_have_no_layout() {
        assert!(layout(42, -1).is_none());
        assert!(layout(-1, -1).is_none());
    }

    #[test]
    fn clicks_map_to_commands() {
        assert_eq!(command_for(1, "clicked"), Some(Command::Toggle));
        assert_eq!(command_for(3, "clicked"), Some(Command::OpenSettings));
        assert_eq!(command_for(4, "clicked"), Some(Command::OpenAbout));
    }

    #[test]
    fn other_events_and_entries_do_nothing() {
        assert_eq!(command_for(2, "clicked"), None);
        assert_eq!(command_for(1, "hovered"), None);
        assert_eq!(command_for(ROOT, "clicked"), None);
        assert_eq!(command_for(9, "clicked"), None);
    }

    #[test]
    fn a_click_is_forwarded_to_the_app() {
        let (commands, mut received) = mpsc::unbounded();
        let menu = Menu::new(commands);
        let missing = menu.event_group(vec![(3, "clicked".into(), OwnedVariant::from(0_i32), 0)]);
        assert_eq!(missing, Vec::<i32>::new());
        assert_eq!(received.try_recv().expect("queued"), Command::OpenSettings);
    }
}

//! The tray icon itself, served as `org.kde.StatusNotifierItem`.

#![allow(clippy::used_underscore_binding)]

use iced::futures::channel::mpsc::UnboundedSender;
use zbus::ObjectPath;
use zbus::interface;

use crate::ipc::Command;

pub const PATH: &str = "/StatusNotifierItem";

const ICON_NAME: &str = "dev.samuelba.Dropshade-tray";
const TITLE: &str = "Dropshade";

type Pixmap = (i32, i32, Vec<u8>);

#[derive(Debug)]
pub struct Item {
    commands: UnboundedSender<Command>,
}

impl Item {
    pub fn new(commands: UnboundedSender<Command>) -> Self {
        Self { commands }
    }

    fn toggle(&self) {
        let _ = self.commands.unbounded_send(Command::Toggle);
    }
}

#[allow(clippy::unused_self)]
#[interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    fn activate(&self, _x: i32, _y: i32) {
        self.toggle();
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        self.toggle();
    }

    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: &str) {}

    #[zbus(property)]
    fn category(&self) -> &'static str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &'static str {
        "dropshade"
    }

    #[zbus(property)]
    fn title(&self) -> &'static str {
        TITLE
    }

    #[zbus(property)]
    fn status(&self) -> &'static str {
        "Active"
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> &'static str {
        ICON_NAME
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        (String::new(), Vec::new(), TITLE.to_owned(), String::new())
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str_unchecked(super::menu::PATH)
    }
}

#[cfg(test)]
mod tests {
    use iced::futures::channel::mpsc;

    use super::*;

    #[test]
    fn a_click_toggles_the_terminal() {
        let (commands, mut received) = mpsc::unbounded();
        let item = Item::new(commands);
        item.activate(0, 0);
        item.secondary_activate(0, 0);
        assert_eq!(received.try_recv().expect("queued"), Command::Toggle);
        assert_eq!(received.try_recv().expect("queued"), Command::Toggle);
    }
}

//! D-Bus: the app's own service, and clients for it and for the GNOME Shell extension.

pub mod client;
pub mod service;

use std::path::PathBuf;

use crate::screens::{self, Screen};

pub const BUS_NAME: &str = "dev.samuelba.Dropshade";
pub const OBJECT_PATH: &str = "/dev/samuelba/Dropshade";
pub const SHELL_BUS_NAME: &str = "dev.samuelba.Dropshade.Shell";

/// What other processes (the CLI, a second instance) can ask the running app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show,
    Hide,
    NewTab(Option<PathBuf>),
    SaveLayout(String),
    LoadLayout(String),
    OpenSettings,
    OpenAbout,
}

/// The drop-down window settings the extension reads from the app.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSettings {
    pub hotkey: String,
    pub width: f64,
    pub height: f64,
    pub monitor: String,
    pub animation_duration: u32,
    pub hide_on_unfocus: bool,
}

impl WindowSettings {
    /// The configured settings, with the monitor list resolved against the connected `screens`
    /// to one entry the extension understands: "pointer", "primary" or a connector.
    pub fn new(window: &dropshade_config::Window, screens: Option<&[Screen]>) -> Self {
        Self {
            hotkey: window.hotkey.trim().to_owned(),
            width: window.width,
            height: window.height,
            monitor: screens::resolve(&window.monitor, screens)
                .as_str()
                .to_owned(),
            animation_duration: window.animation_duration,
            hide_on_unfocus: window.hide_on_unfocus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use dropshade_config::{Monitor, Monitors};

    #[test]
    fn window_settings_come_from_the_config() {
        let config = dropshade_config::Window {
            hotkey: "  <Super>grave ".into(),
            monitor: Monitors::from(Monitor::Named("DP-1".into())),
            ..dropshade_config::Window::default()
        };
        let settings = WindowSettings::new(&config, None);
        assert_eq!(settings.hotkey, "<Super>grave");
        assert_eq!(settings.monitor, "DP-1");
        assert!((settings.height - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn the_extension_gets_the_connector_of_the_preferred_screen() {
        let config = dropshade_config::Window {
            monitor: Monitors::new(vec![Monitor::Named("LEN P27h-10".into()), Monitor::Pointer])
                .expect("entries"),
            ..dropshade_config::Window::default()
        };
        let office = [Screen {
            connector: "DP-4".into(),
            product: "LEN P27h-10".into(),
            primary: false,
        }];
        assert_eq!(WindowSettings::new(&config, Some(&office)).monitor, "DP-4");
        assert_eq!(WindowSettings::new(&config, Some(&[])).monitor, "pointer");
    }
}

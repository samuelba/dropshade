//! D-Bus: the app's own service, and clients for it and for the GNOME Shell extension.

pub mod client;
pub mod service;

use std::path::PathBuf;

pub const BUS_NAME: &str = "io.github.samuelba.Dropshade";
pub const OBJECT_PATH: &str = "/io/github/samuelba/Dropshade";
pub const SHELL_BUS_NAME: &str = "io.github.samuelba.Dropshade.Shell";

/// What other processes (the CLI, a second instance) can ask the running app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show,
    Hide,
    NewTab(Option<PathBuf>),
    SaveLayout(String),
    LoadLayout(String),
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

impl From<&dropshade_config::Window> for WindowSettings {
    fn from(window: &dropshade_config::Window) -> Self {
        Self {
            hotkey: window.hotkey.trim().to_owned(),
            width: window.width,
            height: window.height,
            monitor: window.monitor.as_str().to_owned(),
            animation_duration: window.animation_duration,
            hide_on_unfocus: window.hide_on_unfocus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_settings_come_from_the_config() {
        let config = dropshade_config::Window {
            hotkey: "  <Super>grave ".into(),
            monitor: dropshade_config::Monitor::Connector("DP-1".into()),
            ..dropshade_config::Window::default()
        };
        let settings = WindowSettings::from(&config);
        assert_eq!(settings.hotkey, "<Super>grave");
        assert_eq!(settings.monitor, "DP-1");
        assert!((settings.height - 0.5).abs() < f64::EPSILON);
    }
}

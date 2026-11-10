//! Dropshade: a drop-down terminal for Linux.

mod app;
mod tabs;
mod theme;

use iced::window::settings::PlatformSpecific;
use iced::{Color, Theme, window};
use tracing_subscriber::EnvFilter;

use app::{App, INITIAL_SIZE};

const APP_ID: &str = "io.github.samuelba.Dropshade";
const SHELL_LAUNCHED_ENV: &str = "DROPSHADE_SHELL_LAUNCHED";

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    dropshade_term::setup_env();
    let managed = is_shell_managed(std::env::var(SHELL_LAUNCHED_ENV).ok().as_deref());

    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        // FIFO presentation stalls under Mutter: a commit carrying a wp_fifo_v1 barrier can wait
        // forever for a repaint, freezing the drop-down until it is hidden and shown again.
        // Without vsync, wgpu picks mailbox or immediate; winit still paces redraws with frame
        // callbacks, and ICED_PRESENT_MODE overrides this choice.
        .settings(iced::Settings {
            vsync: false,
            ..iced::Settings::default()
        })
        .window(window::Settings {
            size: INITIAL_SIZE,
            transparent: true,
            decorations: !managed,
            platform_specific: PlatformSpecific {
                application_id: APP_ID.to_owned(),
                ..PlatformSpecific::default()
            },
            ..window::Settings::default()
        })
        .style(|_, theme: &Theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .run()
}

/// Whether the GNOME Shell extension started the app and therefore owns window placement.
fn is_shell_managed(env_value: Option<&str>) -> bool {
    env_value == Some("1")
}

#[cfg(test)]
mod tests {
    use super::is_shell_managed;

    #[test]
    fn only_the_extension_marker_enables_managed_mode() {
        assert!(is_shell_managed(Some("1")));
        assert!(!is_shell_managed(Some("0")));
        assert!(!is_shell_managed(Some("")));
        assert!(!is_shell_managed(None));
    }
}

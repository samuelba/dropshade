//! The app's D-Bus service (`dev.samuelba.Dropshade`): commands for the running instance and
//! the window settings the GNOME Shell extension applies.

use std::path::PathBuf;

use iced::futures::channel::mpsc::UnboundedSender;
use zbus::{Connection, connection, fdo, interface};

use super::{BUS_NAME, Command, OBJECT_PATH, WindowSettings};

#[derive(Debug)]
pub struct Service {
    commands: UnboundedSender<Command>,
    window: WindowSettings,
}

impl Service {
    fn send(&self, command: Command) -> fdo::Result<()> {
        self.commands
            .unbounded_send(command)
            .map_err(|_| fdo::Error::Failed("Dropshade is shutting down".into()))
    }
}

#[interface(name = "dev.samuelba.Dropshade")]
impl Service {
    fn toggle(&self) -> fdo::Result<()> {
        self.send(Command::Toggle)
    }

    fn show(&self) -> fdo::Result<()> {
        self.send(Command::Show)
    }

    fn hide(&self) -> fdo::Result<()> {
        self.send(Command::Hide)
    }

    /// Opens a tab in `cwd`, or in the active tab's directory when `cwd` is empty.
    fn new_tab(&self, cwd: &str) -> fdo::Result<()> {
        let cwd = (!cwd.is_empty()).then(|| PathBuf::from(cwd));
        self.send(Command::NewTab(cwd))
    }

    /// Saves the open tabs as `~/.config/dropshade/layouts/<name>.toml`.
    fn save_layout(&self, name: &str) -> fdo::Result<()> {
        self.send(Command::SaveLayout(name.to_owned()))
    }

    /// Opens the tabs of a saved layout next to the current ones.
    fn load_layout(&self, name: &str) -> fdo::Result<()> {
        self.send(Command::LoadLayout(name.to_owned()))
    }

    /// Shows the window with the settings panel open.
    fn open_settings(&self) -> fdo::Result<()> {
        self.send(Command::OpenSettings)
    }

    /// Shows the window with the About dialog open.
    fn open_about(&self) -> fdo::Result<()> {
        self.send(Command::OpenAbout)
    }

    #[zbus(property)]
    fn hotkey(&self) -> String {
        self.window.hotkey.clone()
    }

    #[zbus(property)]
    fn width(&self) -> f64 {
        self.window.width
    }

    #[zbus(property)]
    fn height(&self) -> f64 {
        self.window.height
    }

    #[zbus(property)]
    fn align(&self) -> String {
        self.window.align.clone()
    }

    #[zbus(property)]
    fn monitor(&self) -> String {
        self.window.monitor.clone()
    }

    #[zbus(property)]
    fn animation_duration(&self) -> u32 {
        self.window.animation_duration
    }

    #[zbus(property)]
    fn hide_on_unfocus(&self) -> bool {
        self.window.hide_on_unfocus
    }
}

/// Connects to the session bus, exports the service and claims the well-known name.
pub async fn start(
    window: WindowSettings,
    commands: UnboundedSender<Command>,
) -> zbus::Result<Connection> {
    connection::Builder::session()
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, Service { commands, window })
        .build()
        .await
}

/// Updates the published window settings and notifies listeners of the values that changed.
pub async fn publish(connection: &Connection, window: WindowSettings) -> zbus::Result<()> {
    let interface = connection
        .object_server()
        .interface::<_, Service>(OBJECT_PATH)
        .await?;
    let previous = std::mem::replace(&mut interface.get_mut().await.window, window.clone());
    let service = interface.get().await;
    let emitter = interface.signal_emitter();
    if previous.hotkey != window.hotkey {
        service.hotkey_changed(emitter).await?;
    }
    if previous.width.to_bits() != window.width.to_bits() {
        service.width_changed(emitter).await?;
    }
    if previous.height.to_bits() != window.height.to_bits() {
        service.height_changed(emitter).await?;
    }
    if previous.align != window.align {
        service.align_changed(emitter).await?;
    }
    if previous.monitor != window.monitor {
        service.monitor_changed(emitter).await?;
    }
    if previous.animation_duration != window.animation_duration {
        service.animation_duration_changed(emitter).await?;
    }
    if previous.hide_on_unfocus != window.hide_on_unfocus {
        service.hide_on_unfocus_changed(emitter).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use iced::futures::channel::mpsc;

    use super::*;

    #[test]
    fn open_settings_is_forwarded_to_the_app() {
        let (commands, mut received) = mpsc::unbounded();
        let window = WindowSettings::new(&dropshade_config::Window::default(), None);
        let service = Service { commands, window };
        service.open_settings().expect("send");
        assert_eq!(received.try_recv().expect("queued"), Command::OpenSettings);
    }

    #[test]
    fn open_about_is_forwarded_to_the_app() {
        let (commands, mut received) = mpsc::unbounded();
        let window = WindowSettings::new(&dropshade_config::Window::default(), None);
        let service = Service { commands, window };
        service.open_about().expect("send");
        assert_eq!(received.try_recv().expect("queued"), Command::OpenAbout);
    }
}

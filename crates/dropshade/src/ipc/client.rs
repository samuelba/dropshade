//! Clients for the running app and for the GNOME Shell extension.

use zbus::{Connection, fdo, proxy};

use super::{BUS_NAME, SHELL_BUS_NAME};

#[proxy(
    interface = "io.github.samuelba.Dropshade",
    default_service = "io.github.samuelba.Dropshade",
    default_path = "/io/github/samuelba/Dropshade"
)]
pub trait App {
    fn toggle(&self) -> zbus::Result<()>;
    fn show(&self) -> zbus::Result<()>;
    fn hide(&self) -> zbus::Result<()>;
    fn new_tab(&self, cwd: &str) -> zbus::Result<()>;
}

#[proxy(
    interface = "io.github.samuelba.Dropshade.Shell",
    default_service = "io.github.samuelba.Dropshade.Shell",
    default_path = "/io/github/samuelba/Dropshade/Shell"
)]
pub trait Shell {
    fn toggle(&self) -> zbus::Result<()>;
    fn show(&self) -> zbus::Result<()>;
    fn hide(&self) -> zbus::Result<()>;
}

/// Who is already on the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presence {
    pub app: bool,
    pub shell: bool,
}

pub async fn presence(connection: &Connection) -> zbus::Result<Presence> {
    let bus = fdo::DBusProxy::new(connection).await?;
    let owned = async |name: &str| -> zbus::Result<bool> {
        Ok(bus.name_has_owner(name.try_into()?).await?)
    };
    Ok(Presence {
        app: owned(BUS_NAME).await?,
        shell: owned(SHELL_BUS_NAME).await?,
    })
}

/// The shell action to forward, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Toggle,
    Show,
    Hide,
}

pub async fn shell(connection: &Connection, action: Visibility) -> zbus::Result<()> {
    let shell = ShellProxy::new(connection).await?;
    match action {
        Visibility::Toggle => shell.toggle().await,
        Visibility::Show => shell.show().await,
        Visibility::Hide => shell.hide().await,
    }
}

pub async fn app(connection: &Connection, action: Visibility) -> zbus::Result<()> {
    let app = AppProxy::new(connection).await?;
    match action {
        Visibility::Toggle => app.toggle().await,
        Visibility::Show => app.show().await,
        Visibility::Hide => app.hide().await,
    }
}

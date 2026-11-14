//! A tray icon on desktops that host `StatusNotifierItem`s: COSMIC's Notifications Tray, KDE
//! Plasma's System Tray, and GNOME with the `AppIndicator` extension. It lives on a connection of
//! its own, so ending the subscription closes it and the panel drops the icon.

mod item;
mod menu;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use zbus::{Connection, fdo, proxy};

use crate::ipc::Command;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";

#[proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
trait Watcher {
    fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;
}

/// The commands picked from the tray icon, while it is shown.
pub fn commands() -> impl Stream<Item = Command> {
    iced::stream::channel(8, async |mut output: mpsc::Sender<Command>| {
        if let Err(error) = run(&mut output).await {
            tracing::warn!(%error, "cannot show the tray icon");
        }
    })
}

async fn run(output: &mut mpsc::Sender<Command>) -> zbus::Result<()> {
    let (sender, mut received) = mpsc::unbounded();
    let connection = zbus::connection::Builder::session()
        .serve_at(item::PATH, item::Item::new(sender.clone()))
        .serve_at(menu::PATH, menu::Menu::new(sender))
        .build()
        .await?;
    let bus = fdo::DBusProxy::new(&connection).await?;
    let mut watchers = bus
        .receive_name_owner_changed_with_args(&[(0, WATCHER)])
        .await?;
    register(&connection).await;
    loop {
        tokio::select! {
            command = received.next() => {
                let Some(command) = command else { return Ok(()) };
                if output.send(command).await.is_err() {
                    return Ok(());
                }
            }
            changed = watchers.next() => {
                let Some(changed) = changed else { return Ok(()) };
                if changed.args().is_ok_and(|args| args.new_owner().is_some()) {
                    register(&connection).await;
                }
            }
        }
    }
}

/// Hands the icon to the panel's watcher, if one runs; a panel started later announces itself.
async fn register(connection: &Connection) {
    let Some(name) = connection.unique_name() else {
        return;
    };
    let result = async {
        WatcherProxy::new(connection)
            .await?
            .register_status_notifier_item(name.as_str())
            .await
    }
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, "no tray to show the icon in yet");
    }
}

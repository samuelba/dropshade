//! A desktop notification announcing a release, through `org.freedesktop.Notifications`.

use std::collections::HashMap;

use iced::futures::StreamExt;
use zbus::{Connection, Variant, proxy};

use super::Release;
use crate::app::APP_NAME;

const APP_ID: &str = "dev.samuelba.Dropshade";
const DOWNLOAD_ACTION: &str = "download";
/// Clicking the notification itself.
const DEFAULT_ACTION: &str = "default";
/// Let the notification server decide how long it stays.
const SERVER_TIMEOUT: i32 = -1;

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[expect(clippy::too_many_arguments, reason = "the D-Bus method's signature")]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Variant<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
}

/// Shows the notification and waits for it; `true` when the user asked to download.
pub async fn announce(release: Release) -> bool {
    match show_and_wait(&release).await {
        Ok(download) => download,
        Err(error) => {
            tracing::warn!(%error, "cannot show the update notification");
            false
        }
    }
}

async fn show_and_wait(release: &Release) -> zbus::Result<bool> {
    let connection = Connection::session().await?;
    let notifications = NotificationsProxy::new(&connection).await?;
    let mut invoked = notifications.receive_action_invoked().await?;
    let mut closed = notifications.receive_notification_closed().await?;
    let hints = HashMap::from([("desktop-entry", Variant::from(APP_ID))]);
    let id = notifications
        .notify(
            APP_NAME,
            0,
            APP_ID,
            &format!("{APP_NAME} {} is available", release.version),
            &format!(
                "You are running {}. Download the new version from GitHub.",
                env!("CARGO_PKG_VERSION")
            ),
            &[DEFAULT_ACTION, "Download", DOWNLOAD_ACTION, "Download"],
            hints,
            SERVER_TIMEOUT,
        )
        .await?;
    loop {
        tokio::select! {
            Some(signal) = invoked.next() => {
                let args = signal.args()?;
                if args.id == id {
                    return Ok(matches!(args.action_key.as_str(), DEFAULT_ACTION | DOWNLOAD_ACTION));
                }
            }
            Some(signal) = closed.next() => {
                if signal.args()?.id == id {
                    return Ok(false);
                }
            }
            else => return Ok(false),
        }
    }
}

//! Desktop notifications through `org.freedesktop.Notifications`.

use std::collections::HashMap;

use iced::futures::StreamExt;
use zbus::{Connection, Variant, proxy};

use crate::app::{APP_ID, APP_NAME};

/// Clicking the notification itself.
pub const DEFAULT_ACTION: &str = "default";
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

/// What a notification shows. `actions` alternates keys and labels, as the D-Bus method wants.
#[derive(Debug)]
pub struct Notification<'a> {
    pub summary: &'a str,
    pub body: &'a str,
    pub actions: &'a [&'a str],
}

/// Shows the notification and waits until it is dismissed or an action is chosen; returns the
/// chosen action's key.
pub async fn show_and_wait(notification: Notification<'_>) -> zbus::Result<Option<String>> {
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
            notification.summary,
            notification.body,
            notification.actions,
            hints,
            SERVER_TIMEOUT,
        )
        .await?;
    loop {
        tokio::select! {
            Some(signal) = invoked.next() => {
                let args = signal.args()?;
                if args.id == id {
                    return Ok(Some(args.action_key));
                }
            }
            Some(signal) = closed.next() => {
                if signal.args()?.id == id {
                    return Ok(None);
                }
            }
            else => return Ok(None),
        }
    }
}

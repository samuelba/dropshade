//! The desktop portal's global shortcuts (`org.freedesktop.portal.GlobalShortcuts`), offered by
//! KDE Plasma, Hyprland and GNOME 48 and later. Dropshade asks for one shortcut, `toggle`, with
//! `window.hotkey` as the preferred keys; the desktop may ask the user to confirm it once and
//! keeps the final say over the keys. It only works while Dropshade runs.

use std::collections::HashMap;
use std::fmt;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use zbus::{Connection, ObjectPath, OwnedObjectPath, OwnedVariant, Variant, proxy};

use super::{Accelerator, Status};
use crate::app::APP_ID;

const SHORTCUT: &str = "toggle";
const DESCRIPTION: &str = "Show or hide Dropshade";
/// A request the user dismissed.
const CANCELLED: u32 = 1;

type Results = HashMap<String, OwnedVariant>;
type Shortcuts = Vec<(String, HashMap<String, OwnedVariant>)>;

#[proxy(
    interface = "org.freedesktop.portal.GlobalShortcuts",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait GlobalShortcuts {
    fn create_session(&self, options: HashMap<&str, Variant<'_>>) -> zbus::Result<OwnedObjectPath>;

    fn bind_shortcuts(
        &self,
        session_handle: &ObjectPath<'_>,
        shortcuts: &[(&str, HashMap<&str, Variant<'_>>)],
        parent_window: &str,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Since version 2.
    fn configure_shortcuts(
        &self,
        session_handle: &ObjectPath<'_>,
        parent_window: &str,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn activated(
        &self,
        session_handle: OwnedObjectPath,
        shortcut_id: String,
        timestamp: u64,
        options: Results,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn shortcuts_changed(
        &self,
        session_handle: OwnedObjectPath,
        shortcuts: Shortcuts,
    ) -> zbus::Result<()>;

    #[zbus(property, name = "version")]
    fn version(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop"
)]
trait Request {
    #[zbus(signal)]
    fn response(&self, response: u32, results: Results) -> zbus::Result<()>;
}

/// Lets an app outside a sandbox tell the portal who it is, so the desktop can remember its
/// shortcuts. Older portals do not have it.
#[proxy(
    interface = "org.freedesktop.host.portal.Registry",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Registry {
    fn register(&self, app_id: &str, options: HashMap<&str, Variant<'_>>) -> zbus::Result<()>;
}

/// What the portal reports.
#[derive(Debug, Clone)]
pub enum Event {
    Status(Status),
    /// The shortcut was pressed.
    Toggle,
    /// This desktop's portal has no global shortcuts.
    Unavailable,
    /// The desktop can show its own editor for the shortcut's keys.
    Configurable(Configure),
}

/// Asks the desktop to show its editor for the shortcut, which is where its keys are changed:
/// the portal only takes the preferred keys when the shortcut is first registered.
#[derive(Debug, Clone)]
pub struct Configure(mpsc::UnboundedSender<()>);

impl Configure {
    pub fn open(&self) {
        let _ = self.0.unbounded_send(());
    }
}

#[derive(Debug)]
enum Error {
    /// This desktop's portal has no global shortcuts.
    Unavailable,
    /// The user dismissed the portal's dialog.
    Declined,
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => f.write_str("the desktop portal has no global shortcuts"),
            Self::Declined => f.write_str("the shortcut was declined"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

/// Binds `hotkey` (GNOME accelerator syntax) through the portal and reports presses, for as long
/// as the stream is polled.
pub fn events(hotkey: &(String, u32)) -> impl Stream<Item = Event> + use<> {
    let hotkey = hotkey.0.clone();
    iced::stream::channel(4, async move |mut output: mpsc::Sender<Event>| {
        let status = match run(&hotkey, &mut output).await {
            Ok(()) => return,
            Err(Error::Unavailable) => {
                tracing::debug!("no GlobalShortcuts portal; the hotkey is up to the desktop");
                let _ = output.send(Event::Unavailable).await;
                return;
            }
            Err(Error::Declined) => Status::Unbound,
            Err(Error::Failed(message)) => {
                tracing::warn!(%message, "cannot bind the hotkey through the portal");
                Status::Failed(message)
            }
        };
        let _ = output.send(Event::Status(status)).await;
    })
}

async fn run(hotkey: &str, output: &mut mpsc::Sender<Event>) -> Result<(), Error> {
    let trigger = Accelerator::parse(hotkey)
        .map(trigger)
        .map_err(Error::Failed)?;
    let connection = Connection::session().await?;
    register(&connection).await;
    let portal = GlobalShortcutsProxy::new(&connection).await?;
    let version = portal.version().await.map_err(|_| Error::Unavailable)?;
    tracing::debug!(version, %trigger, "binding the hotkey through the portal");
    let mut activated = portal.receive_activated().await?;
    let mut changed = portal.receive_shortcuts_changed().await?;
    let session = create_session(&connection, &portal).await?;
    let keys = bind(&connection, &portal, &session, &trigger).await?;
    send(output, Event::Status(Status::Bound(keys))).await?;
    let (configure, mut configure_requests) = mpsc::unbounded();
    if version >= 2 {
        send(output, Event::Configurable(Configure(configure.clone()))).await?;
    }
    loop {
        tokio::select! {
            Some(()) = configure_requests.next() => {
                if let Err(error) = portal.configure_shortcuts(&session, "", HashMap::new()).await {
                    tracing::warn!(%error, "the desktop cannot show its shortcut settings");
                }
            }
            signal = activated.next() => {
                let Some(signal) = signal else { return Ok(()) };
                let args = signal.args()?;
                if *args.session_handle() == session && args.shortcut_id() == SHORTCUT {
                    send(output, Event::Toggle).await?;
                }
            }
            signal = changed.next() => {
                let Some(signal) = signal else { return Ok(()) };
                let args = signal.args()?;
                if *args.session_handle() == session
                    && let Some(keys) = trigger_description(args.shortcuts())
                {
                    send(output, Event::Status(Status::Bound(keys))).await?;
                }
            }
        }
    }
}

async fn send(output: &mut mpsc::Sender<Event>, event: Event) -> Result<(), Error> {
    output
        .send(event)
        .await
        .map_err(|_| Error::Failed("the app stopped listening".into()))
}

async fn register(connection: &Connection) {
    let result = async {
        RegistryProxy::new(connection)
            .await?
            .register(APP_ID, HashMap::new())
            .await
    }
    .await;
    if let Err(error) = result {
        tracing::debug!(%error, "the portal does not register host apps");
    }
}

async fn create_session(
    connection: &Connection,
    portal: &GlobalShortcutsProxy<'_>,
) -> Result<OwnedObjectPath, Error> {
    let token = token("session");
    let results = request(connection, |handle_token| {
        let options = HashMap::from([
            ("handle_token", Variant::from(handle_token)),
            ("session_handle_token", Variant::from(token.as_str())),
        ]);
        portal.create_session(options)
    })
    .await?;
    let handle = results
        .get("session_handle")
        .ok_or_else(|| Error::Failed("the portal did not open a session".into()))?;
    session_handle(handle)
}

/// Binds the shortcut and returns the keys the desktop shows for it.
async fn bind(
    connection: &Connection,
    portal: &GlobalShortcutsProxy<'_>,
    session: &OwnedObjectPath,
    trigger: &str,
) -> Result<String, Error> {
    let shortcuts = [(
        SHORTCUT,
        HashMap::from([
            ("description", Variant::from(DESCRIPTION)),
            ("preferred_trigger", Variant::from(trigger)),
        ]),
    )];
    let results = request(connection, |handle_token| {
        let options = HashMap::from([("handle_token", Variant::from(handle_token))]);
        portal.bind_shortcuts(session, &shortcuts, "", options)
    })
    .await?;
    let shortcuts = results
        .get("shortcuts")
        .and_then(|value| Shortcuts::try_from(value.clone()).ok())
        .unwrap_or_default();
    Ok(trigger_description(&shortcuts).unwrap_or_default())
}

/// Makes a portal request and waits for its response. The response is subscribed to before the
/// call, at the path the portal derives from the token, so it cannot be missed.
async fn request<F, C>(connection: &Connection, call: F) -> Result<Results, Error>
where
    F: FnOnce(String) -> C,
    C: Future<Output = zbus::Result<OwnedObjectPath>>,
{
    let token = token("request");
    let sender = connection
        .unique_name()
        .ok_or_else(|| Error::Failed("no D-Bus name".into()))?;
    let path = request_path(sender.as_str(), &token);
    let proxy = RequestProxy::builder(connection)
        .path(path.as_str())
        .build()
        .await?;
    let mut responses = proxy.receive_response().await?;
    let handle = call(token).await.map_err(|error| match error {
        zbus::Error::MethodError(..) | zbus::Error::FDO(..) => Error::Unavailable,
        other => Error::from(other),
    })?;
    if handle.as_str() != path {
        tracing::debug!(%handle, expected = %path, "the portal answers at another path");
        let proxy = RequestProxy::builder(connection)
            .path(handle)
            .build()
            .await?;
        responses = proxy.receive_response().await?;
    }
    let response = responses
        .next()
        .await
        .ok_or_else(|| Error::Failed("the portal did not answer".into()))?;
    let args = response.args()?;
    match *args.response() {
        0 => Ok(args.results().clone()),
        CANCELLED => Err(Error::Declined),
        _ => Err(Error::Failed("the portal request failed".into())),
    }
}

/// A token that is unique within this process and valid in an object path.
fn token(kind: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let count = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("dropshade_{kind}_{}_{count}", std::process::id())
}

/// Where the portal puts the request for `token` from the connection named `sender`.
fn request_path(sender: &str, token: &str) -> String {
    let sender = sender.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

/// Portals send the session handle as an object path, older ones as a string.
fn session_handle(value: &OwnedVariant) -> Result<OwnedObjectPath, Error> {
    OwnedObjectPath::try_from(value.clone())
        .or_else(|_| String::try_from(value.clone()).and_then(OwnedObjectPath::try_from))
        .map_err(|error| Error::Failed(format!("the session handle is invalid: {error}")))
}

fn trigger_description(shortcuts: &Shortcuts) -> Option<String> {
    shortcuts
        .iter()
        .find(|(id, _)| id == SHORTCUT)
        .and_then(|(_, properties)| properties.get("trigger_description"))
        .and_then(|value| String::try_from(value.clone()).ok())
}

/// The keys in the portal's shortcut syntax, such as "LOGO+grave" or "CTRL+ALT+t".
fn trigger(accelerator: Accelerator) -> String {
    let modifiers = accelerator.modifiers;
    let mut parts: Vec<String> = [
        (modifiers.ctrl, "CTRL"),
        (modifiers.alt, "ALT"),
        (modifiers.shift, "SHIFT"),
        (modifiers.logo, "LOGO"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_owned())
    .collect();
    parts.push(accelerator.key_name());
    parts.join("+")
}

#[cfg(test)]
mod tests;

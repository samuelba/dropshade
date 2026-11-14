use super::*;

fn keys(text: &str) -> Accelerator {
    Accelerator::parse(text).expect(text)
}

#[test]
fn triggers_use_the_portal_syntax() {
    assert_eq!(trigger(keys("F12")), "F12");
    assert_eq!(trigger(keys("<Super>grave")), "LOGO+grave");
    assert_eq!(trigger(keys("<Alt><Control>T")), "CTRL+ALT+t");
}

#[test]
fn requests_live_under_the_sender_and_token() {
    assert_eq!(
        request_path(":1.42", "dropshade_request_7_0"),
        "/org/freedesktop/portal/desktop/request/1_42/dropshade_request_7_0"
    );
}

#[test]
fn tokens_are_unique_and_valid_path_elements() {
    let (a, b) = (token("request"), token("request"));
    assert_ne!(a, b);
    assert!(
        a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "{a}"
    );
}

#[test]
fn session_handles_come_as_paths_or_strings() {
    let path = "/org/freedesktop/portal/desktop/session/1_42/dropshade";
    let as_path = Variant::from(ObjectPath::try_from(path).expect("path"));
    let as_string = Variant::from(path);
    let [as_path, as_string] =
        [as_path, as_string].map(|value| OwnedVariant::try_from(value).expect("owned"));
    for value in [as_path, as_string] {
        assert_eq!(session_handle(&value).expect("handle").as_str(), path);
    }
}

#[tokio::test]
#[ignore = "needs a desktop portal; COSMIC has no GlobalShortcuts"]
async fn a_portal_without_global_shortcuts_is_reported_as_unavailable() {
    let (mut output, _events) = mpsc::channel(4);
    let result = run("F12", &mut output).await;
    assert!(matches!(result, Err(Error::Unavailable)), "{result:?}");
}

/// A stand-in for xdg-desktop-portal's `GlobalShortcuts`, answering requests the way the real one
/// does: the method returns the request path and the response follows as a signal on it.
mod fake {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use zbus::message::Header;
    use zbus::{Connection, OwnedObjectPath, OwnedVariant, Variant, connection, interface};

    use super::super::request_path;

    pub const KEYS: &str = "Meta+`";
    const DESKTOP: &str = "/org/freedesktop/portal/desktop";
    const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";

    pub struct Portal {
        /// The response code for `BindShortcuts`: 0 to accept, 1 when the user declines.
        pub bind_response: u32,
        /// The shortcuts Dropshade asked for: id, description and preferred trigger.
        pub asked: Arc<Mutex<Vec<(String, String, String)>>>,
        /// The sessions whose shortcuts Dropshade asked the desktop to show for editing.
        pub configured: Arc<Mutex<Vec<String>>>,
    }

    fn text(options: &std::collections::HashMap<String, OwnedVariant>, key: &str) -> String {
        options
            .get(key)
            .and_then(|value| String::try_from(value.clone()).ok())
            .unwrap_or_default()
    }

    fn sender(header: &Header<'_>) -> String {
        header.sender().map(ToString::to_string).unwrap_or_default()
    }

    /// Answers at the request path after the method call has returned.
    fn respond(
        connection: &Connection,
        path: String,
        code: u32,
        key: &'static str,
        value: Variant<'static>,
    ) {
        let connection = connection.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let results = std::collections::HashMap::from([(key, value)]);
            let body = (code, results);
            connection
                .emit_signal(
                    None::<&str>,
                    path.as_str(),
                    "org.freedesktop.portal.Request",
                    "Response",
                    &body,
                )
                .await
                .expect("response");
        });
    }

    #[interface(name = "org.freedesktop.portal.GlobalShortcuts")]
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        reason = "the portal's D-Bus methods, as the interface macro wants them"
    )]
    impl Portal {
        #[zbus(property, name = "version")]
        fn version(&self) -> u32 {
            2
        }

        fn create_session(
            &self,
            options: std::collections::HashMap<String, OwnedVariant>,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
        ) -> OwnedObjectPath {
            let sender = sender(&header);
            let request = request_path(&sender, &text(&options, "handle_token"));
            let session = format!(
                "{DESKTOP}/session/{}/{}",
                sender.trim_start_matches(':').replace('.', "_"),
                text(&options, "session_handle_token")
            );
            respond(
                connection,
                request.clone(),
                0,
                "session_handle",
                Variant::from(session),
            );
            OwnedObjectPath::try_from(request).expect("path")
        }

        fn bind_shortcuts(
            &self,
            session_handle: OwnedObjectPath,
            shortcuts: Vec<(String, std::collections::HashMap<String, OwnedVariant>)>,
            parent_window: String,
            options: std::collections::HashMap<String, OwnedVariant>,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
        ) -> OwnedObjectPath {
            let asked = shortcuts.iter().map(|(id, properties)| {
                (
                    id.clone(),
                    text(properties, "description"),
                    text(properties, "preferred_trigger"),
                )
            });
            self.asked.lock().expect("lock").extend(asked);
            // Dropshade has no parent window to hand the portal's dialog.
            let _ = parent_window;
            let request = request_path(&sender(&header), &text(&options, "handle_token"));
            let bound = vec![(
                "toggle".to_owned(),
                std::collections::HashMap::from([(
                    "trigger_description".to_owned(),
                    Variant::from(KEYS),
                )]),
            )];
            let code = self.bind_response;
            respond(
                connection,
                request.clone(),
                code,
                "shortcuts",
                Variant::from(bound),
            );
            if self.bind_response == 0 {
                let connection = connection.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(60)).await;
                    let other = OwnedObjectPath::try_from(format!("{DESKTOP}/session/1_1/other"))
                        .expect("path");
                    for (session, id) in [
                        (&other, "toggle"),
                        (&session_handle, "something-else"),
                        (&session_handle, "toggle"),
                    ] {
                        let options = std::collections::HashMap::<&str, Variant<'_>>::new();
                        let body = (session, id, 0u64, options);
                        connection
                            .emit_signal(None::<&str>, DESKTOP, INTERFACE, "Activated", &body)
                            .await
                            .expect("activated");
                    }
                });
            }
            OwnedObjectPath::try_from(request).expect("path")
        }

        fn configure_shortcuts(
            &self,
            session_handle: OwnedObjectPath,
            parent_window: String,
            options: std::collections::HashMap<String, OwnedVariant>,
        ) {
            let _ = (parent_window, options);
            self.configured
                .lock()
                .expect("lock")
                .push(session_handle.to_string());
        }
    }

    /// Gives up the portal's name for the next fake.
    pub async fn stop(connection: Connection) {
        connection
            .release_name("org.freedesktop.portal.Desktop")
            .await
            .expect("release the name");
    }

    /// Serves the fake portal; refuses to run where a real portal owns the name.
    pub async fn serve(portal: Portal) -> Connection {
        let bus = Connection::session().await.expect("session bus");
        let dbus = zbus::fdo::DBusProxy::new(&bus).await.expect("bus");
        let taken = dbus
            .name_has_owner("org.freedesktop.portal.Desktop".try_into().expect("name"))
            .await
            .expect("ask the bus");
        assert!(
            !taken,
            "a real portal owns the name; run this test inside dbus-run-session"
        );
        connection::Builder::session()
            .name("org.freedesktop.portal.Desktop")
            .serve_at(DESKTOP, portal)
            .build()
            .await
            .expect("fake portal")
    }
}

/// One test, so the two fake portals do not compete for the portal's bus name.
#[tokio::test]
#[ignore = "needs a private bus: dbus-run-session -- cargo test fake_portal -- --ignored"]
async fn the_fake_portal_binds_reports_presses_and_declines() {
    binding_and_pressing().await;
    declining().await;
}

async fn binding_and_pressing() {
    let asked = std::sync::Arc::default();
    let configured = std::sync::Arc::default();
    let portal = fake::serve(fake::Portal {
        bind_response: 0,
        asked: std::sync::Arc::clone(&asked),
        configured: std::sync::Arc::clone(&configured),
    })
    .await;
    let (mut output, mut events) = mpsc::channel(8);
    let client = tokio::spawn(async move { run("<Super>grave", &mut output).await });
    let mut next = async || {
        tokio::time::timeout(std::time::Duration::from_secs(5), events.next())
            .await
            .expect("an event in time")
            .expect("open stream")
    };
    assert!(matches!(next().await, Event::Status(Status::Bound(keys)) if keys == fake::KEYS));
    let Event::Configurable(configure) = next().await else {
        panic!("a version 2 portal can show its shortcut editor");
    };
    assert!(matches!(next().await, Event::Toggle));
    configure.open();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let configured = configured.lock().expect("lock").clone();
    assert!(
        matches!(configured.as_slice(), [session] if session.contains("/dropshade_session_")),
        "{configured:?}"
    );
    assert_eq!(
        *asked.lock().expect("lock"),
        [(
            "toggle".to_owned(),
            DESCRIPTION.to_owned(),
            "LOGO+grave".to_owned()
        )]
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        events.try_recv().is_err(),
        "presses of other shortcuts and sessions are ignored"
    );
    client.abort();
    fake::stop(portal).await;
}

async fn declining() {
    let portal = fake::serve(fake::Portal {
        bind_response: CANCELLED,
        asked: std::sync::Arc::default(),
        configured: std::sync::Arc::default(),
    })
    .await;
    let (mut output, _events) = mpsc::channel(8);
    let result = run("F12", &mut output).await;
    assert!(matches!(result, Err(Error::Declined)), "{result:?}");
    fake::stop(portal).await;
}

//! The screens the drop-down can be pinned to, as GNOME's compositor reports them, which of the
//! configured ones applies, and the choices the settings panel offers for `window.monitor`.

use std::collections::HashMap;
use std::fmt;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};

use dropshade_config::{Monitor, Monitors};
use zbus::{Connection, OwnedVariant, proxy};

type Properties = HashMap<String, OwnedVariant>;
/// Connector, vendor, product and serial.
type MonitorSpec = (String, String, String, String);
type Mode = (String, i32, i32, f64, f64, Vec<f64>, Properties);
type PhysicalMonitor = (MonitorSpec, Vec<Mode>, Properties);
type LogicalMonitor = (i32, i32, f64, u32, bool, Vec<MonitorSpec>, Properties);

#[proxy(
    interface = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path = "/org/gnome/Mutter/DisplayConfig"
)]
trait DisplayConfig {
    fn get_current_state(
        &self,
    ) -> zbus::Result<(u32, Vec<PhysicalMonitor>, Vec<LogicalMonitor>, Properties)>;

    #[zbus(signal)]
    fn monitors_changed(&self) -> zbus::Result<()>;
}

/// A connected screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub connector: String,
    pub product: String,
    pub primary: bool,
}

/// The connected screens, primary first and then by connector.
pub async fn connected(connection: &Connection) -> zbus::Result<Vec<Screen>> {
    let proxy = DisplayConfigProxy::new(connection).await?;
    let (_serial, monitors, logical, _properties) = proxy.get_current_state().await?;
    Ok(from_state(&monitors, &logical))
}

/// The connected screens now and after every change, while GNOME's compositor answers.
pub fn changes() -> impl Stream<Item = Vec<Screen>> {
    iced::stream::channel(1, async |mut output: mpsc::Sender<Vec<Screen>>| {
        if let Err(error) = watch(&mut output).await {
            tracing::warn!(%error, "cannot follow the screens; monitor names may not resolve");
        }
    })
}

async fn watch(output: &mut mpsc::Sender<Vec<Screen>>) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let proxy = DisplayConfigProxy::new(&connection).await?;
    let mut changed = proxy.receive_monitors_changed().await?;
    loop {
        let (_serial, monitors, logical, _properties) = proxy.get_current_state().await?;
        if output.send(from_state(&monitors, &logical)).await.is_err()
            || changed.next().await.is_none()
        {
            return Ok(());
        }
    }
}

fn from_state(monitors: &[PhysicalMonitor], logical: &[LogicalMonitor]) -> Vec<Screen> {
    let primary: Vec<&str> = logical
        .iter()
        .filter(|(_, _, _, _, primary, ..)| *primary)
        .flat_map(|(.., specs, _)| specs.iter().map(|spec| spec.0.as_str()))
        .collect();
    let mut screens: Vec<Screen> = monitors
        .iter()
        .map(|((connector, _, product, _), ..)| Screen {
            connector: connector.clone(),
            product: product.trim().to_owned(),
            primary: primary.contains(&connector.as_str()),
        })
        .collect();
    screens.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(a.connector.cmp(&b.connector))
    });
    screens
}

/// The screen the drop-down goes to: the first of `preferred` that is connected, as its
/// connector. Pointer and primary always apply; with no match it is the pointer's screen. While
/// the screens are unknown the first entry is passed on as written.
pub fn resolve(preferred: &Monitors, screens: Option<&[Screen]>) -> Monitor {
    let entries = preferred.entries();
    let Some(screens) = screens else {
        return entries.first().cloned().unwrap_or_default();
    };
    entries
        .iter()
        .find_map(|entry| match entry {
            Monitor::Pointer | Monitor::Primary => Some(entry.clone()),
            Monitor::Named(_) => find(entry, screens).map(|s| Monitor::Named(s.connector.clone())),
        })
        .unwrap_or_default()
}

/// The first connected screen `entry` names.
pub fn find<'a>(entry: &Monitor, screens: &'a [Screen]) -> Option<&'a Screen> {
    screens
        .iter()
        .find(|screen| entry.names(&screen.connector, &screen.product))
}

/// A screen the settings panel can add to `window.monitor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenChoice {
    pub monitor: Monitor,
    label: String,
}

impl fmt::Display for ScreenChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// How an entry of `window.monitor` is shown.
pub fn label(entry: &Monitor) -> String {
    match entry {
        Monitor::Pointer => "Under the mouse pointer".to_owned(),
        Monitor::Primary => "Primary screen".to_owned(),
        Monitor::Named(name) => name.clone(),
    }
}

/// Pointer, primary and each connected screen, minus those already in `current`. A screen is
/// offered by model, which stays the same when a dock renumbers the connectors, unless it has
/// none or another connected screen is the same model.
pub fn choices(screens: &[Screen], current: &Monitors) -> Vec<ScreenChoice> {
    let fixed = [Monitor::Pointer, Monitor::Primary].map(|monitor| ScreenChoice {
        label: label(&monitor),
        monitor,
    });
    let connected = screens.iter().map(|screen| {
        let same_model = screens
            .iter()
            .filter(|other| other.product.eq_ignore_ascii_case(&screen.product))
            .count();
        if screen.product.is_empty() || same_model > 1 {
            ScreenChoice {
                monitor: Monitor::Named(screen.connector.clone()),
                label: screen.connector.clone(),
            }
        } else {
            ScreenChoice {
                monitor: Monitor::Named(screen.product.clone()),
                label: format!("{} (now on {})", screen.product, screen.connector),
            }
        }
    });
    fixed
        .into_iter()
        .chain(connected)
        .filter(|choice| {
            !current
                .entries()
                .iter()
                .any(|entry| same(entry, &choice.monitor))
        })
        .collect()
}

fn same(a: &Monitor, b: &Monitor) -> bool {
    match (a, b) {
        (Monitor::Named(a), Monitor::Named(b)) => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(connector: &str, product: &str) -> MonitorSpec {
        (
            connector.into(),
            "DEL".into(),
            product.into(),
            "serial".into(),
        )
    }

    fn physical(connector: &str, product: &str) -> PhysicalMonitor {
        (spec(connector, product), Vec::new(), HashMap::new())
    }

    fn logical(primary: bool, connectors: &[&str]) -> LogicalMonitor {
        let specs = connectors.iter().map(|c| spec(c, "")).collect();
        (0, 0, 1.0, 0, primary, specs, HashMap::new())
    }

    fn screen(connector: &str, product: &str, primary: bool) -> Screen {
        Screen {
            connector: connector.into(),
            product: product.into(),
            primary,
        }
    }

    fn labels(choices: &[ScreenChoice]) -> Vec<String> {
        choices.iter().map(ToString::to_string).collect()
    }

    fn monitors(names: &[&str]) -> Monitors {
        Monitors::new(names.iter().map(|name| Monitor::from(*name)).collect()).expect("entries")
    }

    fn named(name: &str) -> Monitor {
        Monitor::Named(name.into())
    }

    #[test]
    fn state_lists_the_primary_screen_first() {
        let monitors = [
            physical("DP-4", "LEN P27h-10"),
            physical("DP-1", " DELL U2719DC "),
            physical("DP-3", "DELL U2413"),
        ];
        let logical = [
            logical(false, &["DP-1"]),
            logical(true, &["DP-3"]),
            logical(false, &["DP-4"]),
        ];
        assert_eq!(
            from_state(&monitors, &logical),
            [
                screen("DP-3", "DELL U2413", true),
                screen("DP-1", "DELL U2719DC", false),
                screen("DP-4", "LEN P27h-10", false),
            ]
        );
    }

    #[test]
    fn mirrored_screens_can_both_be_primary() {
        let monitors = [physical("eDP-1", ""), physical("HDMI-1", "")];
        let screens = from_state(&monitors, &[logical(true, &["eDP-1", "HDMI-1"])]);
        assert!(screens.iter().all(|s| s.primary));
    }

    #[test]
    fn the_first_connected_screen_in_the_list_wins() {
        let preferred = monitors(&["LEN P27h-10", "DELL U2719DC", "eDP-1", "pointer"]);
        let office = [
            screen("DP-4", "LEN P27h-10", false),
            screen("eDP-1", "", true),
        ];
        let home = [
            screen("DP-1", "DELL U2719DC", false),
            screen("eDP-1", "", true),
        ];
        let laptop = [screen("eDP-1", "", true)];
        assert_eq!(resolve(&preferred, Some(&office)), named("DP-4"));
        assert_eq!(resolve(&preferred, Some(&home)), named("DP-1"));
        assert_eq!(resolve(&preferred, Some(&laptop)), named("eDP-1"));
        assert_eq!(resolve(&preferred, Some(&[])), Monitor::Pointer);
    }

    #[test]
    fn a_model_follows_its_screen_to_another_connector() {
        let preferred = monitors(&["dell u2719dc"]);
        let docked = [screen("DP-6", "DELL U2719DC", false)];
        assert_eq!(resolve(&preferred, Some(&docked)), named("DP-6"));
    }

    #[test]
    fn pointer_and_primary_apply_wherever_they_are_listed() {
        let screens = [screen("DP-1", "DELL U2719DC", true)];
        assert_eq!(
            resolve(&monitors(&["HDMI-9", "primary", "DP-1"]), Some(&screens)),
            Monitor::Primary
        );
        assert_eq!(
            resolve(&monitors(&["HDMI-9"]), Some(&screens)),
            Monitor::Pointer,
            "nothing connected matches"
        );
    }

    #[test]
    fn while_the_screens_are_unknown_the_first_entry_is_passed_on() {
        assert_eq!(
            resolve(&monitors(&["DP-2", "pointer"]), None),
            named("DP-2")
        );
    }

    #[test]
    fn choices_offer_screens_by_model_unless_it_is_missing_or_shared() {
        let screens = [
            screen("DP-1", "DELL U2719DC", true),
            screen("DP-2", "DELL U2413", false),
            screen("DP-3", "DELL U2413", false),
            screen("eDP-1", "", false),
        ];
        let choices = choices(&screens, &Monitors::default());
        assert_eq!(
            labels(&choices),
            [
                "Primary screen",
                "DELL U2719DC (now on DP-1)",
                "DP-2",
                "DP-3",
                "eDP-1"
            ]
        );
        assert_eq!(choices[1].monitor, named("DELL U2719DC"));
        assert_eq!(choices[2].monitor, named("DP-2"));
    }

    #[test]
    fn listed_screens_are_not_offered_again() {
        let screens = [
            screen("DP-1", "DELL U2719DC", true),
            screen("eDP-1", "", false),
        ];
        let current = monitors(&["dell u2719dc", "EDP-1", "primary"]);
        assert_eq!(
            labels(&choices(&screens, &current)),
            ["Under the mouse pointer"]
        );
    }

    #[test]
    fn entries_are_labelled_as_written() {
        assert_eq!(label(&Monitor::Pointer), "Under the mouse pointer");
        assert_eq!(label(&named("LEN P27h-10")), "LEN P27h-10");
    }

    #[tokio::test]
    #[ignore = "needs a GNOME session"]
    async fn the_running_compositor_reports_its_screens() {
        let connection = Connection::session().await.expect("session bus");
        let screens = connected(&connection)
            .await
            .expect("decodes GetCurrentState");
        assert_ne!(screens, []);
    }
}

//! The screens the drop-down can be pinned to, as GNOME's compositor reports them, and the
//! choices the settings panel offers for `window.monitor`.

use std::collections::HashMap;
use std::fmt;

use dropshade_config::Monitor;
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

/// One entry of the settings panel's screen picker.
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

/// Pointer and primary, then each connected screen. The configured screen is always listed so the
/// picker can show it as selected; it is marked as not connected only once `screens` is known.
pub fn choices(screens: Option<&[Screen]>, current: &Monitor) -> Vec<ScreenChoice> {
    let known = screens.is_some();
    let screens = screens.unwrap_or_default();
    let fixed = [
        (Monitor::Pointer, "Under the mouse pointer".to_owned()),
        (Monitor::Primary, "Primary screen".to_owned()),
    ];
    let connected = screens.iter().map(|screen| {
        let label = if screen.product.is_empty() {
            screen.connector.clone()
        } else {
            format!("{} · {}", screen.connector, screen.product)
        };
        (Monitor::Connector(screen.connector.clone()), label)
    });
    let missing = match current {
        Monitor::Connector(name) if !screens.iter().any(|s| &s.connector == name) => {
            let label = if known {
                format!("{name} (not connected)")
            } else {
                name.clone()
            };
            Some((current.clone(), label))
        }
        _ => None,
    };
    fixed
        .into_iter()
        .chain(connected)
        .chain(missing)
        .map(|(monitor, label)| ScreenChoice { monitor, label })
        .collect()
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
    fn choices_offer_pointer_primary_and_each_screen() {
        let screens = [
            screen("DP-1", "DELL U2719DC", true),
            screen("eDP-1", "", false),
        ];
        let choices = choices(Some(&screens), &Monitor::Pointer);
        assert_eq!(
            labels(&choices),
            [
                "Under the mouse pointer",
                "Primary screen",
                "DP-1 · DELL U2719DC",
                "eDP-1"
            ]
        );
        assert_eq!(choices[2].monitor, Monitor::Connector("DP-1".into()));
    }

    #[test]
    fn a_configured_screen_that_is_unplugged_stays_selectable() {
        let current = Monitor::Connector("HDMI-2".into());
        let choices = choices(Some(&[screen("DP-1", "", true)]), &current);
        let last = choices.last().expect("choices");
        assert_eq!(last.monitor, current);
        assert_eq!(last.to_string(), "HDMI-2 (not connected)");
    }

    #[test]
    fn the_configured_screen_is_not_called_unplugged_before_the_list_is_known() {
        let current = Monitor::Connector("DP-1".into());
        let choices = choices(None, &current);
        assert_eq!(
            labels(&choices),
            ["Under the mouse pointer", "Primary screen", "DP-1"]
        );
        assert_eq!(choices[2].monitor, current);
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

    #[test]
    fn a_connected_configured_screen_is_not_listed_twice() {
        let current = Monitor::Connector("DP-1".into());
        let choices = choices(Some(&[screen("DP-1", "", true)]), &current);
        assert_eq!(choices.iter().filter(|c| c.monitor == current).count(), 1);
    }
}

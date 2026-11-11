//! What the Wayland compositor offers: whether it can host a layer-shell drop-down, and its
//! screens for the settings panel.

use wayland_client::globals::{GlobalError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{ConnectError, Connection, Dispatch, DispatchError, QueueHandle};

use crate::screens::Screen;

const LAYER_SHELL: &str = "zwlr_layer_shell_v1";
/// The first `wl_output` version that sends the connector name.
const OUTPUT_NAMES: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("no Wayland compositor: {0}")]
    Connect(#[from] ConnectError),
    #[error("cannot list the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("the Wayland connection failed: {0}")]
    Dispatch(#[from] DispatchError),
}

#[derive(Debug)]
pub struct Compositor {
    pub layer_shell: bool,
    pub screens: Vec<Screen>,
}

/// Asks the compositor of the current session (`WAYLAND_DISPLAY`).
pub fn probe() -> Result<Compositor, ProbeError> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Outputs>(&connection)?;
    let handle = queue.handle();
    let mut outputs = Outputs::default();
    let layer_shell = globals.contents().with_list(|list| {
        for global in list.iter().filter(|global| global.interface == "wl_output") {
            if global.version >= OUTPUT_NAMES {
                let index = outputs.found.len();
                outputs.found.push(Output::default());
                let _: wl_output::WlOutput =
                    globals
                        .registry()
                        .bind(global.name, OUTPUT_NAMES, &handle, index);
            }
        }
        list.iter().any(|global| global.interface == LAYER_SHELL)
    });
    queue.roundtrip(&mut outputs)?;
    Ok(Compositor {
        layer_shell,
        screens: named_screens(outputs.found),
    })
}

/// The connected screens, or `None` (logged) when the compositor cannot be asked.
pub fn screens() -> Option<Vec<Screen>> {
    probe()
        .map(|compositor| compositor.screens)
        .inspect_err(|error| tracing::warn!(%error, "cannot list the screens"))
        .ok()
}

#[derive(Debug, Default)]
struct Outputs {
    found: Vec<Output>,
}

#[derive(Debug, Default, Clone)]
struct Output {
    name: String,
    description: String,
    model: String,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Outputs {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, usize> for Outputs {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.found.get_mut(*index) else {
            return;
        };
        match event {
            wl_output::Event::Name { name } => output.name = name,
            wl_output::Event::Description { description } => output.description = description,
            wl_output::Event::Geometry { model, .. } => output.model = model,
            _ => {}
        }
    }
}

/// Named outputs by connector. Wayland has no primary screen.
fn named_screens(outputs: Vec<Output>) -> Vec<Screen> {
    let mut screens: Vec<Screen> = outputs
        .into_iter()
        .filter(|output| !output.name.is_empty())
        .map(|output| Screen {
            product: product(&output),
            connector: output.name,
            primary: false,
        })
        .collect();
    screens.sort_by(|a, b| a.connector.cmp(&b.connector));
    screens
}

/// The model from the screen's EDID, as GNOME reports it too. Without one, the description minus
/// the connector that compositors often append, as in "Dell U2413 (DP-1)".
fn product(output: &Output) -> String {
    let model = output.model.trim();
    if !model.is_empty() && !model.eq_ignore_ascii_case("unknown") {
        return model.to_owned();
    }
    let description = output.description.trim();
    description
        .strip_suffix(&format!("({})", output.name))
        .unwrap_or(description)
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str, description: &str) -> Output {
        Output {
            name: name.into(),
            description: description.into(),
            model: String::new(),
        }
    }

    #[test]
    fn the_product_is_the_model() {
        let output = Output {
            model: " LEN P27h-10 ".into(),
            ..output("DP-4", "Lenovo Group Limited LEN P27h-10 (DP-4)")
        };
        assert_eq!(product(&output), "LEN P27h-10");
    }

    #[test]
    fn without_a_model_the_appended_connector_is_dropped_from_the_description() {
        assert_eq!(
            product(&output("DP-1", "Dell Inc. U2413 (DP-1)")),
            "Dell Inc. U2413"
        );
        assert_eq!(product(&output("DP-4", " LEN P27h-10 ")), "LEN P27h-10");
        let unknown = Output {
            model: "Unknown".into(),
            ..output("eDP-1", "Built-in (eDP-1) panel")
        };
        assert_eq!(product(&unknown), "Built-in (eDP-1) panel");
    }

    #[test]
    fn screens_are_sorted_by_connector_and_unnamed_outputs_skipped() {
        let found = named_screens(vec![
            output("DP-4", "LEN P27h-10 (DP-4)"),
            output("", "nameless"),
            output("DP-1", "DELL U2719DC"),
        ]);
        assert_eq!(
            found,
            [
                Screen {
                    connector: "DP-1".into(),
                    product: "DELL U2719DC".into(),
                    primary: false,
                },
                Screen {
                    connector: "DP-4".into(),
                    product: "LEN P27h-10".into(),
                    primary: false,
                },
            ]
        );
    }

    #[test]
    #[ignore = "needs a Wayland session"]
    fn the_running_compositor_lists_its_screens() {
        let compositor = probe().expect("Wayland session");
        assert_ne!(compositor.screens, []);
    }
}

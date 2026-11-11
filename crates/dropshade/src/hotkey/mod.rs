//! Binding `window.hotkey` outside GNOME, where the Shell extension does it. On COSMIC Dropshade
//! keeps its entry in COSMIC's custom shortcuts, so the key works before Dropshade runs. On other
//! layer-shell desktops it asks the desktop portal for a global shortcut (KDE Plasma, Hyprland).

pub mod accelerator;
mod cosmic;
mod portal;

pub use accelerator::Accelerator;
pub use portal::{Event as PortalEvent, events as portal_events};

/// Who runs `dropshade toggle` on the hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binder {
    /// COSMIC's custom shortcuts, edited by Dropshade.
    Cosmic,
    /// The desktop portal's global shortcuts, while Dropshade runs.
    Portal,
}

/// What the settings panel says about the hotkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The desktop runs `dropshade toggle` on these keys.
    Bound(String),
    Unbound,
    /// Why the desktop's shortcuts were left alone.
    Failed(String),
}

/// Whether this is a COSMIC session.
pub fn on_cosmic() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| {
        desktops
            .split(':')
            .any(|desktop| desktop.trim().eq_ignore_ascii_case("COSMIC"))
    })
}

/// Dropshade's shortcut in COSMIC, as it is now.
pub fn cosmic_status() -> Status {
    match cosmic::current() {
        Ok(Some(entry)) => Status::Bound(entry.keys),
        Ok(None) => Status::Unbound,
        Err(error) => Status::Failed(error),
    }
}

/// Sets Dropshade's shortcut in COSMIC to `hotkey` (GNOME accelerator syntax).
pub fn bind_cosmic(hotkey: &str) -> Status {
    let result = Accelerator::parse(hotkey)
        .and_then(|accelerator| cosmic::bind(accelerator, &toggle_command()?).map(|_| accelerator));
    match result {
        Ok(accelerator) => Status::Bound(accelerator.to_string()),
        Err(error) => {
            tracing::warn!(%error, "left COSMIC's shortcuts alone");
            Status::Failed(error)
        }
    }
}

/// `<this binary> toggle`, quoted for `/bin/sh -c`, which COSMIC runs shortcuts with.
fn toggle_command() -> Result<String, String> {
    let binary =
        crate::extension::launch_target(std::env::var_os("APPIMAGE"), std::env::current_exe().ok())
            .ok_or("cannot tell where the Dropshade binary is")?;
    let path = binary
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", binary.display()))?;
    let quoted = shlex::try_quote(path).map_err(|error| format!("cannot quote {path}: {error}"))?;
    Ok(format!("{quoted} toggle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_toggles_this_binary() {
        let command = toggle_command().expect("current exe");
        assert!(command.ends_with(" toggle"), "{command}");
        let words = shlex::split(&command).expect("shell words");
        assert_eq!(words.len(), 2);
        assert!(std::path::Path::new(&words[0]).is_absolute(), "{command}");
    }

    #[test]
    fn bad_hotkeys_are_reported_without_touching_cosmic() {
        assert!(
            matches!(bind_cosmic("<Hyper>x"), Status::Failed(error) if error.contains("Hyper"))
        );
    }
}

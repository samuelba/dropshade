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

/// Adds Dropshade's shortcut to COSMIC on the first start there, so the hotkey works from then
/// on, even before Dropshade runs. Later starts leave COSMIC's shortcuts to the user: a shortcut
/// they removed does not come back. `None` when there was nothing to do.
pub fn set_up_cosmic(hotkey: &str) -> Option<Status> {
    let marker = directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .and_then(|dirs| dirs.state_dir().map(|dir| dir.join(COSMIC_SET_UP)));
    let set_up_before = marker.as_deref().is_some_and(std::path::Path::exists);
    let status = first_start(set_up_before, cosmic_status, || bind_cosmic(hotkey))?;
    if let (Status::Bound(_), Some(marker)) = (&status, marker) {
        let written = marker
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&marker, ""));
        if let Err(error) = written {
            tracing::warn!(%error, path = %marker.display(), "cannot record COSMIC's shortcut");
        }
    }
    Some(status)
}

const COSMIC_SET_UP: &str = "cosmic-shortcut-added";

fn first_start(
    set_up_before: bool,
    current: impl FnOnce() -> Status,
    bind: impl FnOnce() -> Status,
) -> Option<Status> {
    if set_up_before {
        return None;
    }
    match current() {
        Status::Unbound => Some(bind()),
        status => Some(status),
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

    fn untouched() -> Status {
        panic!("COSMIC's shortcuts must be left alone")
    }

    #[test]
    fn the_first_start_adds_the_missing_shortcut() {
        let status = first_start(false, || Status::Unbound, || Status::Bound("F12".into()));
        assert_eq!(status, Some(Status::Bound("F12".into())));
    }

    #[test]
    fn the_first_start_keeps_a_shortcut_the_user_already_has() {
        let status = first_start(false, || Status::Bound("<Super>grave".into()), untouched);
        assert_eq!(status, Some(Status::Bound("<Super>grave".into())));
    }

    #[test]
    fn the_first_start_reports_an_unreadable_file_without_binding() {
        let status = first_start(false, || Status::Failed("bad".into()), untouched);
        assert_eq!(status, Some(Status::Failed("bad".into())));
    }

    #[test]
    fn a_shortcut_removed_after_the_first_start_stays_removed() {
        assert_eq!(first_start(true, || Status::Unbound, untouched), None);
    }

    #[test]
    fn bad_hotkeys_are_reported_without_touching_cosmic() {
        assert!(
            matches!(bind_cosmic("<Hyper>x"), Status::Failed(error) if error.contains("Hyper"))
        );
    }
}

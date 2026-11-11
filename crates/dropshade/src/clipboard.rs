//! Copying and pasting the terminal's text.
//!
//! A layer-shell surface makes a poor clipboard owner: hiding destroys it, and what it copied
//! goes with it, and offers from other apps only reach it while it has the keyboard focus. Where
//! the compositor offers a data-control protocol (COSMIC, KDE Plasma, Sway, Hyprland, niri, …)
//! Dropshade copies and pastes through it instead, on its own connection, like `wl-copy` does.

use std::io::Read;
use std::thread;

use iced::Task;
use iced::futures::channel::oneshot;
use wl_clipboard_rs::{copy, paste};

/// The Wayland globals of the data-control protocols, either of which carries both selections.
pub const DATA_CONTROL_GLOBALS: [&str; 2] = [
    "ext_data_control_manager_v1",
    "zwlr_data_control_manager_v1",
];

/// Where copied text goes and pasted text comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clipboard {
    /// The window's own, as iced provides it; it lasts only while a window is open.
    Window,
    /// The compositor's data-control protocol, independent of any window or focus.
    DataControl,
}

/// The selection a text is copied to or pasted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// What Copy and Paste use.
    Clipboard,
    /// What selecting sets and the middle button pastes.
    Primary,
}

impl Clipboard {
    pub fn new(data_control: bool) -> Self {
        if data_control {
            Self::DataControl
        } else {
            Self::Window
        }
    }

    pub fn read(self, selection: Selection) -> Task<Option<String>> {
        match (self, selection) {
            (Self::Window, Selection::Clipboard) => iced::clipboard::read(),
            (Self::Window, Selection::Primary) => iced::clipboard::read_primary(),
            (Self::DataControl, _) => {
                let (sender, receiver) = oneshot::channel();
                spawn("clipboard-paste", move || {
                    let _ = sender.send(paste_text(selection));
                });
                Task::perform(async move { receiver.await.ok().flatten() }, |text| text)
            }
        }
    }

    pub fn write<T>(self, selection: Selection, text: String) -> Task<T> {
        match (self, selection) {
            (Self::Window, Selection::Clipboard) => iced::clipboard::write(text),
            (Self::Window, Selection::Primary) => iced::clipboard::write_primary(text),
            (Self::DataControl, _) => {
                spawn("clipboard-copy", move || copy_text(selection, text));
                Task::none()
            }
        }
    }
}

/// Wayland calls block on round trips and on the other app, so they stay off the UI thread.
fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(error) = thread::Builder::new().name(name.to_owned()).spawn(work) {
        tracing::error!(%error, "could not start a clipboard thread");
    }
}

/// Takes the selection and serves it from a thread of its own until another app takes it over.
fn copy_text(selection: Selection, text: String) {
    let mut options = copy::Options::new();
    options.clipboard(match selection {
        Selection::Clipboard => copy::ClipboardType::Regular,
        Selection::Primary => copy::ClipboardType::Primary,
    });
    let source = copy::Source::Bytes(text.into_bytes().into_boxed_slice());
    if let Err(error) = options.copy(source, copy::MimeType::Text) {
        tracing::warn!(%error, ?selection, "could not copy");
    }
}

fn paste_text(selection: Selection) -> Option<String> {
    let kind = match selection {
        Selection::Clipboard => paste::ClipboardType::Regular,
        Selection::Primary => paste::ClipboardType::Primary,
    };
    let (mut pipe, _) =
        match paste::get_contents(kind, paste::Seat::Unspecified, paste::MimeType::Text) {
            Ok(contents) => contents,
            Err(
                paste::Error::NoSeats | paste::Error::ClipboardEmpty | paste::Error::NoMimeType,
            ) => return None,
            Err(error) => {
                tracing::warn!(%error, ?selection, "could not paste");
                return None;
            }
        };
    let mut bytes = Vec::new();
    if let Err(error) = pipe.read_to_end(&mut bytes) {
        tracing::warn!(%error, ?selection, "could not read the pasted text");
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn data_control_is_chosen_only_when_offered() {
        assert_eq!(Clipboard::new(true), Clipboard::DataControl);
        assert_eq!(Clipboard::new(false), Clipboard::Window);
    }

    /// Polls, since another app's copy settles asynchronously.
    fn eventually(mut check: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if check() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn wl_paste(primary: bool) -> String {
        let mut command = Command::new("wl-paste");
        command.arg("--no-newline");
        if primary {
            command.arg("--primary");
        }
        let output = command.output().expect("wl-paste runs");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn wl_copy(primary: bool, text: &str) {
        let mut command = Command::new("wl-copy");
        if primary {
            command.arg("--primary");
        }
        command.arg(text).stdin(Stdio::null());
        assert!(command.status().expect("wl-copy runs").success());
    }

    #[test]
    #[ignore = "needs a Wayland compositor with data control, and wl-clipboard"]
    fn selections_reach_other_apps_and_back() {
        for (selection, primary) in [(Selection::Clipboard, false), (Selection::Primary, true)] {
            let ours = format!("dropshade copied {selection:?}");
            copy_text(selection, ours.clone());
            assert!(
                eventually(|| wl_paste(primary) == ours),
                "{selection:?} out"
            );

            let theirs = format!("another app copied {selection:?}");
            wl_copy(primary, &theirs);
            assert!(
                eventually(|| paste_text(selection).as_deref() == Some(theirs.as_str())),
                "{selection:?} in"
            );
        }
    }
}

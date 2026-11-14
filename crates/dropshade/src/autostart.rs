//! Starting Dropshade hidden when the user logs in, through an XDG autostart entry. Desktops
//! whose hotkey only reaches a running Dropshade need this; the others start it on the first
//! press.

use std::io;
use std::path::{Path, PathBuf};

use crate::app::APP_ID;

/// Whether the autostart entry exists and is not switched off.
pub fn is_enabled() -> bool {
    dir().is_some_and(|dir| is_enabled_in(&dir))
}

/// Writes or removes the autostart entry.
pub fn set(on: bool) -> io::Result<()> {
    let dir = dir().ok_or_else(|| io::Error::other("no config directory (HOME is not set)"))?;
    set_in(&dir, on)
}

fn dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.config_dir().join("autostart"))
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join(format!("{APP_ID}.desktop"))
}

fn entry() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Dropshade\n\
         Comment=Starts the drop-down terminal hidden, so its hotkey works\n\
         Exec=dropshade --hidden\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// Desktops switch an entry off with `Hidden=true` instead of removing it.
fn is_enabled_in(dir: &Path) -> bool {
    std::fs::read_to_string(file_in(dir)).is_ok_and(|text| {
        !text
            .lines()
            .any(|line| line.trim().replace(' ', "") == "Hidden=true")
    })
}

fn set_in(dir: &Path, on: bool) -> io::Result<()> {
    let file = file_in(dir);
    if on {
        std::fs::create_dir_all(dir)?;
        return std::fs::write(file, entry());
    }
    match std::fs::remove_file(file) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dropshade-autostart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn switching_on_writes_a_hidden_start_and_off_removes_it() {
        let dir = temp_dir("toggle");
        assert!(!is_enabled_in(&dir));
        set_in(&dir, true).expect("enable");
        assert!(is_enabled_in(&dir));
        let text = std::fs::read_to_string(file_in(&dir)).expect("entry");
        assert!(text.contains("Exec=dropshade --hidden\n"));
        set_in(&dir, false).expect("disable");
        assert!(!file_in(&dir).exists());
        set_in(&dir, false).expect("disabling twice is fine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_the_desktop_hid_counts_as_off() {
        let dir = temp_dir("hidden");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(file_in(&dir), format!("{}Hidden=true\n", entry())).expect("write");
        assert!(!is_enabled_in(&dir));
        set_in(&dir, true).expect("enable");
        assert!(is_enabled_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

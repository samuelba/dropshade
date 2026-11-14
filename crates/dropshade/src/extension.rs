//! `dropshade gnome-extension install`: installs the companion GNOME Shell extension, which is
//! embedded in the binary so it always matches this version of the app.

use std::path::Path;
use std::process::{Command, ExitCode};

pub const UUID: &str = "dropshade@samuelba.dev";
const SCHEMA_ID: &str = "org.gnome.shell.extensions.dropshade";

const FILES: [(&str, &str); 10] = [
    (
        "metadata.json",
        include_str!("../../../gnome-extension/metadata.json"),
    ),
    (
        "extension.js",
        include_str!("../../../gnome-extension/extension.js"),
    ),
    (
        "launcher.js",
        include_str!("../../../gnome-extension/launcher.js"),
    ),
    (
        "dropdown.js",
        include_str!("../../../gnome-extension/dropdown.js"),
    ),
    ("dbus.js", include_str!("../../../gnome-extension/dbus.js")),
    (
        "indicator.js",
        include_str!("../../../gnome-extension/indicator.js"),
    ),
    (
        "app-settings.js",
        include_str!("../../../gnome-extension/app-settings.js"),
    ),
    ("LICENSE", include_str!("../../../gnome-extension/LICENSE")),
    (
        "icons/dropshade.svg",
        include_str!("../../../gnome-extension/icons/dropshade.svg"),
    ),
    (
        "schemas/org.gnome.shell.extensions.dropshade.gschema.xml",
        include_str!(
            "../../../gnome-extension/schemas/org.gnome.shell.extensions.dropshade.gschema.xml"
        ),
    ),
];

pub fn install() -> ExitCode {
    let Some(target) = directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("gnome-shell/extensions").join(UUID))
    else {
        eprintln!("dropshade: cannot find the data directory (HOME is not set)");
        return ExitCode::FAILURE;
    };
    if let Err(error) = write_files(&target) {
        eprintln!(
            "dropshade: cannot install the extension to {}: {error}",
            target.display()
        );
        return ExitCode::FAILURE;
    }
    let schemas = target.join("schemas");
    if let Err(error) = run("glib-compile-schemas", &[schemas.as_os_str()]) {
        eprintln!("dropshade: {error}");
        return ExitCode::FAILURE;
    }
    println!(
        "Installed the GNOME Shell extension to {}",
        target.display()
    );
    point_extension_at_this_binary(&schemas);
    match run("gnome-extensions", &["enable".as_ref(), UUID.as_ref()]) {
        Ok(()) => println!("Enabled it."),
        Err(_) => println!("Enable it with: gnome-extensions enable {UUID}"),
    }
    println!("On Wayland, log out and back in once so GNOME Shell loads the new extension.");
    ExitCode::SUCCESS
}

fn write_files(target: &Path) -> std::io::Result<()> {
    for (name, content) in FILES {
        let path = target.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

/// GNOME Shell does not see the user's shell PATH (e.g. ~/.cargo/bin), so the extension is told
/// exactly which binary to start.
fn point_extension_at_this_binary(schemas: &Path) {
    let current = std::env::current_exe().ok();
    let Some(binary) = launch_target(std::env::var_os("APPIMAGE"), current) else {
        return;
    };
    let value = format!("[{}]", quote(&binary));
    let args = [
        "--schemadir".as_ref(),
        schemas.as_os_str(),
        "set".as_ref(),
        SCHEMA_ID.as_ref(),
        "command".as_ref(),
        value.as_ref(),
    ];
    match run("gsettings", &args) {
        Ok(()) => println!("The extension starts {}", binary.display()),
        Err(error) => {
            eprintln!("dropshade: could not tell the extension where Dropshade is: {error}");
        }
    }
}

/// What the extension should start. Inside an `AppImage` the binary lives in a temporary mount,
/// so the `AppImage` file itself (`$APPIMAGE`) is the stable choice.
pub(crate) fn launch_target(
    appimage: Option<std::ffi::OsString>,
    current: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    appimage
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or(current)
}

/// A `GVariant` string literal for a path.
fn quote(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    format!("'{escaped}'")
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_extension_files_are_written() {
        let target =
            std::env::temp_dir().join(format!("dropshade-extension-test-{}", std::process::id()));
        write_files(&target).expect("write files");
        for (name, content) in FILES {
            assert_eq!(
                std::fs::read_to_string(target.join(name)).expect(name),
                content
            );
        }
        std::fs::remove_dir_all(&target).expect("clean up");
    }

    #[test]
    fn metadata_matches_the_uuid() {
        let metadata = FILES
            .iter()
            .find(|(name, _)| *name == "metadata.json")
            .expect("metadata")
            .1;
        assert!(metadata.contains(UUID));
    }

    #[test]
    fn appimages_point_the_extension_at_the_appimage_file() {
        let current = Some(std::path::PathBuf::from(
            "/tmp/.mount_abc/usr/bin/dropshade",
        ));
        let appimage = Some("/home/me/Apps/Dropshade.AppImage".into());
        assert_eq!(
            launch_target(appimage, current.clone()),
            Some(std::path::PathBuf::from("/home/me/Apps/Dropshade.AppImage"))
        );
        assert_eq!(launch_target(None, current.clone()), current);
        assert_eq!(
            launch_target(Some("relative".into()), current.clone()),
            current
        );
    }

    #[test]
    fn paths_are_quoted_for_gsettings() {
        assert_eq!(
            quote(Path::new("/usr/bin/dropshade")),
            "'/usr/bin/dropshade'"
        );
        assert_eq!(
            quote(Path::new("/home/o'neil/bin/dropshade")),
            "'/home/o\\'neil/bin/dropshade'"
        );
    }
}

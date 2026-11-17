//! Updating an install made by `install.sh` (in `~/.local`, without sudo) by running the new
//! release's own `install.sh`, and restarting into the new binary.
//!
//! Package installs (rpm, deb, Arch) are left to the package manager, and `AppImage` or cargo
//! installs to the user: those only get a link to the release.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};

use semver::Version;

use super::{REPOSITORY, REQUEST_TIMEOUT};
use crate::extension::UUID;

/// The script is a few KiB; anything far larger is not it.
const MAX_SCRIPT_BYTES: u64 = 1 << 20;
/// The marker `install.sh` writes: `method=script` and `version=<installed version>`.
const MARKER: &str = "dropshade/install-method";
/// The file that changes with every release even when the extension's code does not.
const EXTENSION_METADATA: &str = "metadata.json";

/// How the running binary was installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// By `install.sh`, at `binary`. `installed` is the version the script last put there: after
    /// an update it is newer than the running one, until Dropshade restarts.
    Script {
        binary: PathBuf,
        installed: Option<Version>,
    },
    /// By a package, an `AppImage`, cargo or by hand.
    Other,
}

/// What an update changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installed {
    /// The GNOME Shell extension's code changed, which GNOME loads only at the next log-in.
    pub extension_changed: bool,
}

pub fn install_kind() -> InstallKind {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = |name: &str, fallback: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    let (Some(bin_dir), Some(data_dir)) = (
        xdg("XDG_BIN_HOME", ".local/bin"),
        xdg("XDG_DATA_HOME", ".local/share"),
    ) else {
        return InstallKind::Other;
    };
    let marker = std::fs::read_to_string(data_dir.join(MARKER)).ok();
    kind_of(
        marker.as_deref(),
        std::env::current_exe().ok().as_deref(),
        &bin_dir,
    )
}

/// The install kind from the marker's text, the running binary and the script's bin directory.
fn kind_of(marker: Option<&str>, current: Option<&Path>, bin_dir: &Path) -> InstallKind {
    let value = |key: &str| {
        marker?
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(str::trim)
    };
    let binary = bin_dir.join("dropshade");
    // Linux reports a replaced binary as "<path> (deleted)".
    let running = current
        .and_then(Path::to_str)
        .map(|path| path.strip_suffix(" (deleted)").unwrap_or(path));
    if value("method") != Some("script") || running != binary.to_str() {
        return InstallKind::Other;
    }
    InstallKind::Script {
        installed: value("version").and_then(|version| Version::parse(version).ok()),
        binary,
    }
}

/// Downloads `install.sh` from the release and runs it for that version.
pub async fn self_update(version: Version) -> Result<Installed, String> {
    tokio::task::spawn_blocking(move || run_installer(&version))
        .await
        .map_err(|_| "the update was cancelled".to_owned())
        .and_then(|result| result)
}

fn run_installer(version: &Version) -> Result<Installed, String> {
    let url = format!(
        "{}/releases/download/v{version}/install.sh",
        REPOSITORY.trim_end_matches('/')
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    let mut script = String::new();
    agent
        .get(&url)
        .header(
            "User-Agent",
            concat!("dropshade/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| format!("cannot download the installer: {error}"))?
        .body_mut()
        .as_reader()
        .take(MAX_SCRIPT_BYTES)
        .read_to_string(&mut script)
        .map_err(|error| format!("cannot download the installer: {error}"))?;

    let extension = extension_dir();
    let before = extension.as_deref().and_then(extension_code);
    // The script comes in on stdin, so it never sits in a shared temporary directory.
    let mut child = Command::new("sh")
        .arg("-s")
        .env("DROPSHADE_VERSION", version.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run sh: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|error| format!("cannot run the installer: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("cannot run the installer: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::info!(%stdout, %stderr, status = %output.status, "ran install.sh");
    if !output.status.success() {
        return Err(failure_reason(&stderr, output.status));
    }
    let after = extension.as_deref().and_then(extension_code);
    Ok(Installed {
        extension_changed: after.is_some() && before != after,
    })
}

/// The installer's own explanation, without its prefix, or else how it ended.
fn failure_reason(stderr: &str, status: std::process::ExitStatus) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(
            || format!("the installer stopped ({status})"),
            |line| {
                line.strip_prefix("dropshade install: ")
                    .unwrap_or(line)
                    .to_owned()
            },
        )
}

fn extension_dir() -> Option<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("gnome-shell/extensions").join(UUID))
}

/// The extension's files apart from `metadata.json`, whose version changes with every release.
fn extension_code(dir: &Path) -> Option<BTreeMap<PathBuf, Vec<u8>>> {
    fn collect(dir: &Path, root: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, root, files);
            } else if let Ok(relative) = path.strip_prefix(root)
                && relative != Path::new(EXTENSION_METADATA)
                && let Ok(content) = std::fs::read(&path)
            {
                files.insert(relative.to_owned(), content);
            }
        }
    }
    if !dir.is_dir() {
        return None;
    }
    let mut files = BTreeMap::new();
    collect(dir, dir, &mut files);
    Some(files)
}

/// A restart into the updated binary, carried out by `main` once the app has shut down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restart {
    pub binary: PathBuf,
    pub config: Option<PathBuf>,
}

static RESTART: Mutex<Option<Restart>> = Mutex::new(None);

pub fn request_restart(restart: Restart) {
    *RESTART.lock().unwrap_or_else(PoisonError::into_inner) = Some(restart);
}

pub fn take_restart() -> Option<Restart> {
    RESTART
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Replaces this process with the new binary, keeping its PID: on GNOME the extension that
/// started Dropshade keeps tracking it. Returns only when that fails.
pub fn restart_into(restart: &Restart) -> std::io::Error {
    use std::os::unix::process::CommandExt;

    close_on_exec();
    let mut command = Command::new(&restart.binary);
    if let Some(config) = &restart.config {
        command.arg("--config").arg(config);
    }
    tracing::info!(binary = %restart.binary.display(), "restarting into the new release");
    command.exec()
}

/// Nothing but stdin, stdout and stderr may reach the new binary. A terminal's PTY that stayed
/// open would keep its shell running, orphaned, instead of hanging it up, and the D-Bus
/// connection would keep the old name.
#[expect(
    unsafe_code,
    reason = "nix only sets descriptor flags through a BorrowedFd, which needs an open descriptor"
)]
fn close_on_exec() {
    use std::os::fd::BorrowedFd;

    use nix::fcntl::{FcntlArg, FdFlag, fcntl};

    let Ok(entries) = std::fs::read_dir("/proc/self/fd") else {
        tracing::warn!("cannot list the open descriptors; the restart may keep old shells alive");
        return;
    };
    let descriptors: Vec<i32> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .filter(|&fd| fd > 2)
        .collect();
    for fd in descriptors {
        // SAFETY: the flag is only set, nothing is read or closed. The listing's own descriptor
        // is closed by now, and a descriptor another thread closed or reused in between only
        // gets a failed call or a flag it should have anyway, since the process execs next.
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        let _ = fcntl(borrowed, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = "/home/me/.local/bin";

    fn script(version: &str) -> InstallKind {
        InstallKind::Script {
            binary: PathBuf::from(BIN).join("dropshade"),
            installed: Version::parse(version).ok(),
        }
    }

    #[test]
    fn the_scripts_binary_with_its_marker_is_a_script_install() {
        let kind = kind_of(
            Some("method=script\nversion=0.11.0\n"),
            Some(Path::new("/home/me/.local/bin/dropshade")),
            Path::new(BIN),
        );
        assert_eq!(kind, script("0.11.0"));
    }

    #[test]
    fn a_replaced_binary_still_counts() {
        let kind = kind_of(
            Some("method=script\nversion=0.12.0\n"),
            Some(Path::new("/home/me/.local/bin/dropshade (deleted)")),
            Path::new(BIN),
        );
        assert_eq!(kind, script("0.12.0"));
    }

    #[test]
    fn without_the_marker_it_is_another_install() {
        let kind = kind_of(
            None,
            Some(Path::new("/home/me/.local/bin/dropshade")),
            Path::new(BIN),
        );
        assert_eq!(kind, InstallKind::Other);
    }

    #[test]
    fn a_package_binary_is_another_install_even_with_a_marker() {
        let kind = kind_of(
            Some("method=script\nversion=0.11.0\n"),
            Some(Path::new("/usr/bin/dropshade")),
            Path::new(BIN),
        );
        assert_eq!(kind, InstallKind::Other);
    }

    #[test]
    fn an_unreadable_marker_version_is_unknown() {
        let kind = kind_of(
            Some("method=script\nversion=nonsense\n"),
            Some(Path::new("/home/me/.local/bin/dropshade")),
            Path::new(BIN),
        );
        assert_eq!(kind, script("not a version"));
    }

    #[test]
    fn the_installers_message_explains_a_failure() {
        use std::os::unix::process::ExitStatusExt;
        let status = std::process::ExitStatus::from_raw(1 << 8);
        let stderr = "Downloading\ndropshade install: the download does not match its \
                      checksum. Nothing was installed.\n";
        assert_eq!(
            failure_reason(stderr, status),
            "the download does not match its checksum. Nothing was installed."
        );
        assert!(failure_reason("", status).starts_with("the installer stopped"));
    }

    #[test]
    fn only_the_extensions_code_counts_as_a_change() {
        let dir = std::env::temp_dir().join(format!("dropshade-extension-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("schemas")).expect("dir");
        let metadata = dir.join(EXTENSION_METADATA);
        std::fs::write(&metadata, r#"{"version-name": "0.11.0"}"#).expect("write");
        std::fs::write(dir.join("extension.js"), "old").expect("write");
        std::fs::write(dir.join("schemas/a.xml"), "schema").expect("write");
        let before = extension_code(&dir);
        std::fs::write(&metadata, r#"{"version-name": "0.12.0"}"#).expect("write");
        assert_eq!(
            before,
            extension_code(&dir),
            "a new version alone is no change"
        );
        std::fs::write(dir.join("extension.js"), "new").expect("write");
        assert_ne!(before, extension_code(&dir));
        std::fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(extension_code(&dir), None);
    }

    #[test]
    fn a_restart_is_taken_once() {
        let restart = Restart {
            binary: PathBuf::from("/home/me/.local/bin/dropshade"),
            config: None,
        };
        request_restart(restart.clone());
        assert_eq!(take_restart(), Some(restart));
        assert_eq!(take_restart(), None);
    }
}

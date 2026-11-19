//! Restarting under the GNOME Shell extension. The extension tells Dropshade's window apart by
//! the private Wayland connection it started the process on and tracks it by its PID, so the app
//! cannot replace itself in place: the new binary would open an ordinary window on a connection
//! of its own, and the extension, still seeing the PID, would not start it again on the hotkey.
//!
//! Instead a helper, this binary run again, waits for the app to leave the session bus and asks
//! the extension to show Dropshade, which starts the new binary the way it always does. That works
//! with every extension version, including the one GNOME keeps running until the next log-in.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::Instant;
use zbus::Connection;

use super::install::close_on_exec;
use crate::ipc::client::{self, Presence, Visibility};

/// The hidden subcommand that runs the helper.
pub const SUBCOMMAND: &str = "relaunch";
/// The running image, which `current_exe` names "<path> (deleted)" once an update replaced it.
const SELF_EXE: &str = "/proc/self/exe";
const POLL: Duration = Duration::from_millis(100);
/// Long enough for a slow shutdown and the extension noticing that the app quit.
const TIMEOUT: Duration = Duration::from_secs(15);
/// Outlasts the helper, so no instance it got started can hand over again.
const HAND_OVER_COOLDOWN: Duration = TIMEOUT.saturating_mul(2);
const HAND_OVER_STAMP: &str = "dropshade-hand-over";

/// The descriptor of the Wayland connection the extension hands the process it starts.
const WAYLAND_SOCKET: &str = "WAYLAND_SOCKET";
const GNOME_SHELL: &str = "gnome-shell";

/// Whether this binary was started in GNOME Shell's environment but not by the extension: an
/// older Dropshade replaced itself with it on restart, which loses the extension's connection.
/// The extension's own launches carry `WAYLAND_SOCKET`; commands run in a tab, which inherit the
/// environment too, have a shell as their parent.
pub fn replaced_an_older_instance() -> bool {
    let parent = std::fs::read_to_string(format!(
        "/proc/{}/comm",
        std::os::unix::process::parent_id()
    ))
    .ok();
    is_replacement(
        std::env::var_os(WAYLAND_SOCKET).as_deref(),
        parent.as_deref().map(str::trim),
    )
}

fn is_replacement(wayland_socket: Option<&OsStr>, parent: Option<&str>) -> bool {
    wayland_socket.is_none() && parent == Some(GNOME_SHELL)
}

/// Records a hand-over unless one happened within `HAND_OVER_COOLDOWN`. Should
/// `replaced_an_older_instance` ever misjudge the extension's own launch, every instance the
/// helper got started would hand over again, without end.
pub fn claim_hand_over() -> bool {
    let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return false;
    };
    claim(
        &Path::new(&runtime_dir).join(HAND_OVER_STAMP),
        SystemTime::now(),
    )
}

fn claim(stamp: &Path, now: SystemTime) -> bool {
    let last = std::fs::read_to_string(stamp)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds));
    let recent = last.is_some_and(|last| {
        now.duration_since(last)
            .is_ok_and(|age| age < HAND_OVER_COOLDOWN)
    });
    if recent {
        tracing::warn!("handed over to the extension moments ago; running here instead");
        return false;
    }
    let seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    std::fs::write(stamp, seconds.to_string())
        .inspect_err(|error| tracing::warn!(%error, "cannot record the hand-over"))
        .is_ok()
}

/// Starts the helper, which outlives this process. Called once the app has shut down.
pub fn spawn_helper() -> std::io::Result<()> {
    close_on_exec();
    Command::new(SELF_EXE)
        .arg(SUBCOMMAND)
        .stdin(Stdio::null())
        .spawn()
        .map(drop)
}

/// The helper: returns once the extension started the new instance.
pub fn run_helper() -> Result<(), String> {
    crate::cli::runtime()?.block_on(async {
        let connection = Connection::session()
            .await
            .map_err(|error| format!("no session bus: {error}"))?;
        relaunch(&connection, Instant::now() + TIMEOUT).await
    })
}

/// What the helper needs from the session bus.
trait Bus {
    async fn presence(&self) -> Result<Presence, String>;
    async fn ask_shell_to_show(&self) -> zbus::Result<()>;
}

impl Bus for Connection {
    async fn presence(&self) -> Result<Presence, String> {
        client::presence(self)
            .await
            .map_err(|error| error.to_string())
    }

    async fn ask_shell_to_show(&self) -> zbus::Result<()> {
        client::shell(self, Visibility::Show).await
    }
}

async fn relaunch(bus: &impl Bus, deadline: Instant) -> Result<(), String> {
    let mut presence = bus.presence().await?;
    while presence.app {
        pause(deadline).await?;
        presence = bus.presence().await?;
    }
    while !presence.app {
        if !presence.shell {
            return Err("the GNOME Shell extension is not running".to_owned());
        }
        // Until the extension noticed the old instance quit, it ignores the request; it only ever
        // starts one instance.
        if let Err(error) = bus.ask_shell_to_show().await {
            tracing::debug!(%error, "the extension did not take the request yet");
        }
        pause(deadline).await?;
        presence = bus.presence().await?;
    }
    Ok(())
}

async fn pause(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("Dropshade did not start again; press the hotkey to start it".to_owned());
    }
    tokio::time::sleep(POLL).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    use super::*;

    const OLD_APP: Presence = Presence {
        app: true,
        shell: true,
    };
    const SHELL: Presence = Presence {
        app: false,
        shell: true,
    };
    const NEW_APP: Presence = OLD_APP;
    const NOBODY: Presence = Presence {
        app: false,
        shell: false,
    };

    /// Reports `states` in turn, then the last one for good.
    struct Scripted {
        states: RefCell<VecDeque<Presence>>,
        shows: Cell<u32>,
    }

    impl Scripted {
        fn new(states: &[Presence]) -> Self {
            Self {
                states: RefCell::new(states.iter().copied().collect()),
                shows: Cell::new(0),
            }
        }
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the trait is async; the script answers at once"
    )]
    impl Bus for Scripted {
        async fn presence(&self) -> Result<Presence, String> {
            let mut states = self.states.borrow_mut();
            let state = if states.len() > 1 {
                states.pop_front()
            } else {
                states.front().copied()
            };
            Ok(state.expect("a scripted state"))
        }

        async fn ask_shell_to_show(&self) -> zbus::Result<()> {
            self.shows.set(self.shows.get() + 1);
            Ok(())
        }
    }

    fn soon() -> Instant {
        Instant::now() + TIMEOUT
    }

    #[tokio::test]
    async fn it_asks_the_extension_only_after_the_old_instance_left() {
        let bus = Scripted::new(&[OLD_APP, OLD_APP, SHELL, SHELL, NEW_APP]);
        assert_eq!(relaunch(&bus, soon()).await, Ok(()));
        assert_eq!(bus.shows.get(), 2);
    }

    #[tokio::test]
    async fn without_the_extension_nobody_can_start_it() {
        let bus = Scripted::new(&[OLD_APP, NOBODY]);
        assert!(relaunch(&bus, soon()).await.is_err());
        assert_eq!(bus.shows.get(), 0);
    }

    #[tokio::test]
    async fn it_gives_up_when_the_old_instance_does_not_quit() {
        let bus = Scripted::new(&[OLD_APP]);
        assert!(relaunch(&bus, Instant::now()).await.is_err());
        assert_eq!(bus.shows.get(), 0);
    }

    #[tokio::test]
    async fn it_gives_up_when_no_new_instance_appears() {
        let bus = Scripted::new(&[SHELL]);
        assert!(relaunch(&bus, Instant::now()).await.is_err());
        assert_eq!(bus.shows.get(), 1);
    }

    #[test]
    fn a_binary_exec_d_by_an_older_restart_is_a_replacement() {
        assert!(is_replacement(None, Some(GNOME_SHELL)));
    }

    #[test]
    fn the_extensions_own_launch_is_no_replacement() {
        assert!(!is_replacement(Some(OsStr::new("3")), Some(GNOME_SHELL)));
    }

    #[test]
    fn a_command_run_in_a_tab_is_no_replacement() {
        assert!(!is_replacement(None, Some("bash")));
        assert!(!is_replacement(None, None));
    }

    #[test]
    fn a_hand_over_is_claimed_once_per_cooldown() {
        let dir = std::env::temp_dir().join(format!("dropshade-relaunch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let stamp = dir.join(HAND_OVER_STAMP);
        let now = SystemTime::now();
        assert!(claim(&stamp, now));
        assert!(!claim(&stamp, now + Duration::from_secs(1)));
        assert!(claim(&stamp, now + HAND_OVER_COOLDOWN));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn without_a_place_to_record_it_nothing_hands_over() {
        let stamp = Path::new("/nonexistent/dropshade").join(HAND_OVER_STAMP);
        assert!(!claim(&stamp, SystemTime::now()));
    }
}

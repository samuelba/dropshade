//! Hands command-line requests to a running Dropshade or to the GNOME Shell extension, so that
//! there is only ever one instance.

use std::path::PathBuf;
use std::time::Duration;

use zbus::Connection;

use crate::ipc::client::{self, AppProxy, Presence, Visibility};

const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);
const LAUNCH_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Visibility(Visibility),
    NewTab(Option<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Handled,
    /// Nobody else can serve the request: start the app in this process.
    RunHere {
        first_directory: Option<PathBuf>,
    },
}

/// Who should serve a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    RunHere,
    Shell(Visibility),
    App(Visibility),
    AppNewTab,
    /// Let the extension start the app, then ask the new instance for the tab.
    LaunchThenNewTab,
    Nothing,
}

fn decide(request: &Request, presence: Presence, launched_by_shell: bool) -> Plan {
    if launched_by_shell {
        return Plan::RunHere;
    }
    match request {
        Request::Visibility(action) if presence.shell => Plan::Shell(*action),
        Request::Visibility(action) if presence.app => Plan::App(*action),
        Request::Visibility(Visibility::Hide) => Plan::Nothing,
        Request::NewTab(_) if presence.app => Plan::AppNewTab,
        Request::NewTab(_) if presence.shell => Plan::LaunchThenNewTab,
        Request::Visibility(_) | Request::NewTab(_) => Plan::RunHere,
    }
}

/// Serves `request` through D-Bus if possible. Without a session bus the app simply runs here.
pub fn hand_off(request: &Request, launched_by_shell: bool) -> Result<Outcome, String> {
    let run_here = || Outcome::RunHere {
        first_directory: match request {
            Request::NewTab(directory) => directory.clone(),
            Request::Visibility(_) => None,
        },
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    runtime.block_on(async {
        let Ok(connection) = Connection::session().await else {
            return Ok(run_here());
        };
        let presence = client::presence(&connection)
            .await
            .map_err(|error| error.to_string())?;
        let plan = decide(request, presence, launched_by_shell);
        execute(&connection, plan, request)
            .await
            .map_err(|error| error.to_string())?;
        Ok(if plan == Plan::RunHere {
            run_here()
        } else {
            Outcome::Handled
        })
    })
}

async fn execute(connection: &Connection, plan: Plan, request: &Request) -> zbus::Result<()> {
    let directory = match request {
        Request::NewTab(Some(directory)) => directory.to_string_lossy().into_owned(),
        Request::NewTab(None) | Request::Visibility(_) => String::new(),
    };
    match plan {
        Plan::RunHere | Plan::Nothing => Ok(()),
        Plan::Shell(action) => client::shell(connection, action).await,
        Plan::App(action) => client::app(connection, action).await,
        Plan::AppNewTab => AppProxy::new(connection).await?.new_tab(&directory).await,
        Plan::LaunchThenNewTab => {
            client::shell(connection, Visibility::Show).await?;
            wait_for_app(connection).await?;
            AppProxy::new(connection).await?.new_tab(&directory).await
        }
    }
}

async fn wait_for_app(connection: &Connection) -> zbus::Result<()> {
    let deadline = tokio::time::Instant::now() + LAUNCH_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if client::presence(connection).await?.app {
            return Ok(());
        }
        tokio::time::sleep(LAUNCH_POLL).await;
    }
    Err(zbus::Error::Failure(
        "Dropshade did not start in time".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOBODY: Presence = Presence {
        app: false,
        shell: false,
    };
    const APP: Presence = Presence {
        app: true,
        shell: false,
    };
    const SHELL: Presence = Presence {
        app: false,
        shell: true,
    };
    const BOTH: Presence = Presence {
        app: true,
        shell: true,
    };

    fn show() -> Request {
        Request::Visibility(Visibility::Show)
    }

    #[test]
    fn the_extension_handles_visibility_when_present() {
        assert_eq!(decide(&show(), SHELL, false), Plan::Shell(Visibility::Show));
        assert_eq!(decide(&show(), BOTH, false), Plan::Shell(Visibility::Show));
    }

    #[test]
    fn a_running_app_handles_visibility_without_the_extension() {
        let toggle = Request::Visibility(Visibility::Toggle);
        assert_eq!(decide(&toggle, APP, false), Plan::App(Visibility::Toggle));
    }

    #[test]
    fn with_nobody_around_the_app_starts_here_except_for_hide() {
        assert_eq!(decide(&show(), NOBODY, false), Plan::RunHere);
        assert_eq!(
            decide(&Request::Visibility(Visibility::Hide), NOBODY, false),
            Plan::Nothing
        );
    }

    #[test]
    fn new_tabs_go_to_the_running_app_or_launch_it() {
        let new_tab = Request::NewTab(Some(PathBuf::from("/tmp")));
        assert_eq!(decide(&new_tab, APP, false), Plan::AppNewTab);
        assert_eq!(decide(&new_tab, BOTH, false), Plan::AppNewTab);
        assert_eq!(decide(&new_tab, SHELL, false), Plan::LaunchThenNewTab);
        assert_eq!(decide(&new_tab, NOBODY, false), Plan::RunHere);
    }

    #[test]
    fn an_instance_launched_by_the_extension_always_runs() {
        assert_eq!(decide(&show(), BOTH, true), Plan::RunHere);
    }
}

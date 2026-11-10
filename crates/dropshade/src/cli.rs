//! Hands command-line requests to a running Dropshade or to the GNOME Shell extension, so that
//! there is only ever one instance.

use std::path::PathBuf;
use std::time::Duration;

use zbus::Connection;

use crate::app::Startup;
use crate::ipc::client::{self, AppProxy, Presence, Visibility};

const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);
const LAUNCH_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Visibility(Visibility),
    NewTab(Option<PathBuf>),
    SaveLayout(String),
    LoadLayout(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Handled,
    /// Nobody else can serve the request: start the app in this process.
    RunHere {
        startup: Startup,
    },
}

/// Who should serve a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    RunHere,
    Shell(Visibility),
    App(Visibility),
    /// Ask the running app for a new tab or layout.
    AppFollowUp,
    /// Let the extension start the app, then send the follow-up to the new instance.
    LaunchThenFollowUp,
    Nothing,
    NotRunning,
}

fn decide(request: &Request, presence: Presence, launched_by_shell: bool) -> Plan {
    if launched_by_shell {
        return Plan::RunHere;
    }
    match request {
        Request::Visibility(action) if presence.shell => Plan::Shell(*action),
        Request::Visibility(action) if presence.app => Plan::App(*action),
        Request::Visibility(Visibility::Hide) => Plan::Nothing,
        Request::NewTab(_) | Request::LoadLayout(_) | Request::SaveLayout(_) if presence.app => {
            Plan::AppFollowUp
        }
        Request::SaveLayout(_) => Plan::NotRunning,
        Request::NewTab(_) | Request::LoadLayout(_) if presence.shell => Plan::LaunchThenFollowUp,
        Request::Visibility(_) | Request::NewTab(_) | Request::LoadLayout(_) => Plan::RunHere,
    }
}

/// Serves `request` through D-Bus if possible. Without a session bus the app simply runs here.
pub fn hand_off(request: &Request, launched_by_shell: bool) -> Result<Outcome, String> {
    let run_here = || Outcome::RunHere {
        startup: match request {
            Request::NewTab(Some(directory)) => Startup::Directory(directory.clone()),
            Request::LoadLayout(name) => Startup::Layout(name.clone()),
            Request::NewTab(None) | Request::Visibility(_) | Request::SaveLayout(_) => {
                Startup::Default
            }
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
        if plan == Plan::NotRunning {
            return Err("Dropshade is not running".to_owned());
        }
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
    match plan {
        Plan::RunHere | Plan::Nothing | Plan::NotRunning => Ok(()),
        Plan::Shell(action) => client::shell(connection, action).await,
        Plan::App(action) => client::app(connection, action).await,
        Plan::AppFollowUp => follow_up(connection, request).await,
        Plan::LaunchThenFollowUp => {
            client::shell(connection, Visibility::Show).await?;
            wait_for_app(connection).await?;
            follow_up(connection, request).await
        }
    }
}

async fn follow_up(connection: &Connection, request: &Request) -> zbus::Result<()> {
    let app = AppProxy::new(connection).await?;
    match request {
        Request::NewTab(directory) => {
            let directory = directory
                .as_deref()
                .map(|dir| dir.to_string_lossy().into_owned());
            app.new_tab(directory.as_deref().unwrap_or_default()).await
        }
        Request::SaveLayout(name) => app.save_layout(name).await,
        Request::LoadLayout(name) => app.load_layout(name).await,
        Request::Visibility(action) => client::app(connection, *action).await,
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
        assert_eq!(decide(&new_tab, APP, false), Plan::AppFollowUp);
        assert_eq!(decide(&new_tab, BOTH, false), Plan::AppFollowUp);
        assert_eq!(decide(&new_tab, SHELL, false), Plan::LaunchThenFollowUp);
        assert_eq!(decide(&new_tab, NOBODY, false), Plan::RunHere);
    }

    #[test]
    fn layouts_load_anywhere_but_only_a_running_app_can_save_one() {
        let load = Request::LoadLayout("work".into());
        assert_eq!(decide(&load, APP, false), Plan::AppFollowUp);
        assert_eq!(decide(&load, SHELL, false), Plan::LaunchThenFollowUp);
        assert_eq!(decide(&load, NOBODY, false), Plan::RunHere);
        let save = Request::SaveLayout("work".into());
        assert_eq!(decide(&save, APP, false), Plan::AppFollowUp);
        assert_eq!(decide(&save, SHELL, false), Plan::NotRunning);
        assert_eq!(decide(&save, NOBODY, false), Plan::NotRunning);
    }

    #[test]
    fn an_instance_launched_by_the_extension_always_runs() {
        assert_eq!(decide(&show(), BOTH, true), Plan::RunHere);
    }
}

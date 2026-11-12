//! Dropshade: a drop-down terminal for Linux.

mod app;
mod cli;
mod clipboard;
mod dnd;
mod extension;
mod hotkey;
mod ipc;
mod release;
mod screens;
mod settings;
mod tabs;
mod theme;
mod wayland;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use iced::{Color, Theme};
use tracing_subscriber::EnvFilter;

use app::{APP_ID, App, Host, LAYER_NAMESPACE, Startup};
use cli::{Outcome, Request};
use clipboard::Clipboard;
use ipc::client::Visibility;

const SHELL_LAUNCHED_ENV: &str = "DROPSHADE_SHELL_LAUNCHED";

#[derive(Debug, Parser)]
#[command(version, about = "A drop-down terminal for Linux")]
struct Cli {
    /// Read this configuration file instead of ~/.config/dropshade/config.toml.
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the terminal if it is hidden, hide it if it is in front.
    Toggle,
    /// Show the terminal (the default when no command is given).
    Show,
    /// Hide the terminal.
    Hide,
    /// Open a new tab, in DIR or the current tab's directory.
    NewTab {
        #[arg(long, value_name = "DIR")]
        cwd: Option<PathBuf>,
    },
    /// Save or open sets of tabs.
    #[command(subcommand)]
    Layout(LayoutCommand),
    /// Inspect the configuration file.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Manage the companion GNOME Shell extension.
    #[command(subcommand)]
    GnomeExtension(ExtensionCommand),
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the default configuration, with documentation for every setting.
    PrintDefault,
    /// Print where the configuration file is read from.
    Path,
    /// Check the configuration file and report every problem.
    Check,
}

#[derive(Debug, Subcommand)]
enum LayoutCommand {
    /// Save the open tabs (names and directories) as a layout.
    Save { name: String },
    /// Open the tabs of a saved layout.
    Load { name: String },
    /// List the saved layouts.
    List,
}

#[derive(Debug, Subcommand)]
enum ExtensionCommand {
    /// Install (or update) the extension for the current user.
    Install,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let config_path = cli.config.or_else(dropshade_config::default_path);
    let request = match cli.command {
        Some(Command::Config(command)) => return config_command(&command, config_path.as_deref()),
        Some(Command::GnomeExtension(ExtensionCommand::Install)) => return extension::install(),
        Some(Command::Layout(LayoutCommand::List)) => return list_layouts(),
        Some(Command::Layout(LayoutCommand::Save { name })) => match valid_layout(name) {
            Ok(name) => Request::SaveLayout(name),
            Err(code) => return code,
        },
        Some(Command::Layout(LayoutCommand::Load { name })) => match valid_layout(name) {
            Ok(name) => Request::LoadLayout(name),
            Err(code) => return code,
        },
        Some(Command::Toggle) => Request::Visibility(Visibility::Toggle),
        Some(Command::Show) | None => Request::Visibility(Visibility::Show),
        Some(Command::Hide) => Request::Visibility(Visibility::Hide),
        Some(Command::NewTab { cwd }) => match cwd.map(std::path::absolute).transpose() {
            Ok(cwd) => Request::NewTab(cwd),
            Err(error) => {
                eprintln!("dropshade: invalid --cwd: {error}");
                return ExitCode::FAILURE;
            }
        },
    };
    launch(&request, config_path)
}

fn launch(request: &Request, config_path: Option<PathBuf>) -> ExitCode {
    let managed = is_shell_managed(std::env::var(SHELL_LAUNCHED_ENV).ok().as_deref());
    match cli::hand_off(request, managed) {
        Ok(Outcome::Handled) => ExitCode::SUCCESS,
        Ok(Outcome::RunHere { startup }) => {
            let (host, clipboard) = platform(managed);
            run(config_path, host, clipboard, startup)
        }
        Err(error) => {
            eprintln!("dropshade: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The GNOME Shell extension if it started this process, else layer shell where the compositor
/// offers it; and the compositor's data control for the clipboard where it offers that.
fn platform(managed: bool) -> (Host, Clipboard) {
    if managed {
        return (Host::Shell, Clipboard::Window);
    }
    match wayland::probe() {
        Ok(compositor) => {
            let host = if compositor.layer_shell {
                Host::Layer
            } else {
                Host::Window
            };
            (host, Clipboard::new(compositor.data_control))
        }
        Err(error) => {
            tracing::debug!(%error, "no layer shell");
            (Host::Window, Clipboard::Window)
        }
    }
}

fn run(
    config_path: Option<PathBuf>,
    host: Host,
    clipboard: Clipboard,
    startup: Startup,
) -> ExitCode {
    dropshade_term::setup_env();
    tracing::debug!(?host, ?clipboard, "showing and hiding");
    // A layer surface lives on a connection made here, shared with the drop target; winit's
    // window shares its own once it opens.
    let layer_connection = (host == Host::Layer)
        .then(|| {
            wayland_client::Connection::connect_to_env()
                .inspect_err(|error| tracing::warn!(%error, "dropped files are ignored"))
                .ok()
        })
        .flatten();
    let drop_target = layer_connection.clone().map(dnd::DropTarget::new);
    let boot = move || {
        App::boot(
            config_path.clone(),
            host,
            clipboard,
            startup.clone(),
            drop_target.clone(),
        )
    };
    let result = match host {
        Host::Layer => run_layer(boot, layer_connection),
        Host::Shell | Host::Window => run_window(boot),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dropshade: {error}");
            ExitCode::FAILURE
        }
    }
}

fn style(_: &App, theme: &Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    }
}

fn run_layer(
    boot: impl Fn() -> (App, iced::Task<app::Message>) + 'static,
    connection: Option<wayland_client::Connection>,
) -> Result<(), String> {
    use iced_layershell::reexport::WithConnection;
    use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

    iced_layershell::daemon(boot, LAYER_NAMESPACE, App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .style(style)
        .settings(Settings {
            id: Some(APP_ID.to_owned()),
            // The app opens its surface itself, on the configured screen.
            layer_settings: LayerShellSettings {
                start_mode: StartMode::Background,
                ..LayerShellSettings::default()
            },
            with_connection: connection.map(WithConnection::Value),
            ..Settings::default()
        })
        .run()
        .map_err(|error| error.to_string())
}

fn run_window(boot: impl Fn() -> (App, iced::Task<app::Message>) + 'static) -> Result<(), String> {
    iced::daemon(boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        // FIFO presentation stalls under Mutter: a commit carrying a wp_fifo_v1 barrier can wait
        // forever for a repaint, freezing the drop-down until it is hidden and shown again.
        // Without vsync, wgpu picks mailbox or immediate; winit still paces redraws with frame
        // callbacks, and ICED_PRESENT_MODE overrides this choice.
        .settings(iced::Settings {
            vsync: false,
            ..iced::Settings::default()
        })
        .style(style)
        .run()
        .map_err(|error| error.to_string())
}

fn config_command(command: &ConfigCommand, path: Option<&Path>) -> ExitCode {
    match command {
        ConfigCommand::PrintDefault => {
            print!("{}", dropshade_config::DEFAULT_CONFIG);
            ExitCode::SUCCESS
        }
        ConfigCommand::Path => match path {
            Some(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            None => no_config_path(),
        },
        ConfigCommand::Check => path.map_or_else(no_config_path, check_config),
    }
}

fn check_config(path: &Path) -> ExitCode {
    let config = match dropshade_config::load(path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let themes = dropshade_config::themes_dir();
    let warnings = settings::build(&config, false, themes.as_deref()).warnings;
    if warnings.is_empty() {
        println!("{}: OK", path.display());
        return ExitCode::SUCCESS;
    }
    for warning in warnings {
        eprintln!("{}: {warning}", path.display());
    }
    ExitCode::FAILURE
}

fn valid_layout(name: String) -> Result<String, ExitCode> {
    let Some(dir) = dropshade_session::layouts_dir() else {
        eprintln!("dropshade: no config directory (HOME is not set)");
        return Err(ExitCode::FAILURE);
    };
    match dropshade_session::layout_path(&dir, &name) {
        Ok(_) => Ok(name),
        Err(error) => {
            eprintln!("dropshade: {error}");
            Err(ExitCode::FAILURE)
        }
    }
}

fn list_layouts() -> ExitCode {
    let Some(dir) = dropshade_session::layouts_dir() else {
        return no_config_path();
    };
    for name in dropshade_session::list_layouts(&dir) {
        println!("{name}");
    }
    ExitCode::SUCCESS
}

fn no_config_path() -> ExitCode {
    eprintln!("dropshade: no configuration path (HOME is not set); pass --config");
    ExitCode::FAILURE
}

/// Whether the GNOME Shell extension started the app and therefore owns window placement.
fn is_shell_managed(env_value: Option<&str>) -> bool {
    env_value == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_extension_marker_enables_managed_mode() {
        assert!(is_shell_managed(Some("1")));
        assert!(!is_shell_managed(Some("0")));
        assert!(!is_shell_managed(Some("")));
        assert!(!is_shell_managed(None));
    }

    #[test]
    fn cli_parses_window_commands() {
        let cli = Cli::try_parse_from(["dropshade", "new-tab", "--cwd", "/tmp"]).expect("valid");
        assert!(matches!(
            cli.command,
            Some(Command::NewTab { cwd: Some(_) })
        ));
        assert!(matches!(
            Cli::try_parse_from(["dropshade", "toggle"])
                .expect("valid")
                .command,
            Some(Command::Toggle)
        ));
        assert!(
            Cli::try_parse_from(["dropshade"])
                .expect("valid")
                .command
                .is_none()
        );
        let save = Cli::try_parse_from(["dropshade", "layout", "save", "work"]).expect("valid");
        assert!(matches!(
            save.command,
            Some(Command::Layout(LayoutCommand::Save { .. }))
        ));
        let install =
            Cli::try_parse_from(["dropshade", "gnome-extension", "install"]).expect("valid");
        assert!(matches!(
            install.command,
            Some(Command::GnomeExtension(ExtensionCommand::Install))
        ));
    }

    #[test]
    fn cli_parses_config_commands() {
        let cli = Cli::try_parse_from(["dropshade", "--config", "/tmp/x.toml", "config", "check"])
            .expect("valid");
        assert_eq!(cli.config.as_deref(), Some(Path::new("/tmp/x.toml")));
        assert!(matches!(
            cli.command,
            Some(Command::Config(ConfigCommand::Check))
        ));
        assert!(Cli::try_parse_from(["dropshade", "config", "explode"]).is_err());
    }
}

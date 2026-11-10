//! Dropshade: a drop-down terminal for Linux.

mod app;
mod settings;
mod tabs;
mod theme;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use iced::window::settings::PlatformSpecific;
use iced::{Color, Theme, window};
use tracing_subscriber::EnvFilter;

use app::{App, INITIAL_SIZE};

const APP_ID: &str = "io.github.samuelba.Dropshade";
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
    /// Inspect the configuration file.
    #[command(subcommand)]
    Config(ConfigCommand),
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

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let config_path = cli.config.or_else(dropshade_config::default_path);
    match cli.command {
        Some(Command::Config(command)) => config_command(&command, config_path.as_deref()),
        None => run(config_path),
    }
}

fn run(config_path: Option<PathBuf>) -> ExitCode {
    dropshade_term::setup_env();
    let managed = is_shell_managed(std::env::var(SHELL_LAUNCHED_ENV).ok().as_deref());

    let result = iced::application(
        move || App::boot(config_path.clone()),
        App::update,
        App::view,
    )
    .title(App::title)
    .subscription(App::subscription)
    // FIFO presentation stalls under Mutter: a commit carrying a wp_fifo_v1 barrier can wait
    // forever for a repaint, freezing the drop-down until it is hidden and shown again.
    // Without vsync, wgpu picks mailbox or immediate; winit still paces redraws with frame
    // callbacks, and ICED_PRESENT_MODE overrides this choice.
    .settings(iced::Settings {
        vsync: false,
        ..iced::Settings::default()
    })
    .window(window::Settings {
        size: INITIAL_SIZE,
        transparent: true,
        decorations: !managed,
        platform_specific: PlatformSpecific {
            application_id: APP_ID.to_owned(),
            ..PlatformSpecific::default()
        },
        ..window::Settings::default()
    })
    .style(|_, theme: &Theme| iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    })
    .run();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dropshade: {error}");
            ExitCode::FAILURE
        }
    }
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

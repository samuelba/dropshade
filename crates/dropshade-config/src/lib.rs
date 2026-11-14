//! Dropshade's configuration: the TOML file, themes and keybinding definitions.

pub mod edit;
mod error;
mod schema;
pub mod theme;

use std::path::{Path, PathBuf};

pub use error::ConfigError;
pub use schema::{
    AnsiColors, Appearance, Binding, ColorOverrides, Config, CursorColors, CursorShape, Font,
    Input, Monitor, Monitors, Notifications, PrimaryColors, Scrollback, SelectionColors, Session,
    TabBarPosition, TabWidth, Terminal, ThemeChoice, Unlimited, Updates, Window, WindowAlign,
    WindowAnimation, WindowVerticalAlign,
};

/// The default configuration, documented; `dropshade config print-default` prints it.
pub const DEFAULT_CONFIG: &str = include_str!("default.toml");

/// `$XDG_CONFIG_HOME/dropshade`, or `None` when no home directory is known.
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

/// Parses and validates configuration text. `path` is only used in error messages.
///
/// # Errors
///
/// Fails with the position of a syntax error, an unknown key, or an out-of-range value.
pub fn parse(text: &str, path: &Path) -> Result<Config, ConfigError> {
    let config: Config = toml::from_str(text).map_err(|error| {
        ConfigError::at(
            path,
            text,
            error.span().map(|span| span.start),
            error.message().trim(),
        )
    })?;
    validate(&config).map_err(|message| ConfigError::at(path, text, None, message))?;
    Ok(config)
}

fn validate(config: &Config) -> Result<(), String> {
    if config.font.family.trim().is_empty() {
        return Err("font.family must not be empty".into());
    }
    check_range("font.size", config.font.size, 4.0, 200.0)?;
    check_range("font.line_height", config.font.line_height, 0.8, 3.0)?;
    check_range(
        "appearance.background_opacity",
        config.appearance.background_opacity,
        0.0,
        1.0,
    )?;
    check_range("appearance.padding", config.appearance.padding, 0.0, 200.0)?;
    check_range(
        "appearance.unfocused_pane_dim",
        config.appearance.unfocused_pane_dim,
        0.0,
        0.8,
    )?;
    check_fraction("window.width", config.window.width, 0.2)?;
    check_fraction("window.height", config.window.height, 0.1)?;
    if config.window.animation_duration > 2000 {
        return Err(format!(
            "window.animation_duration must be at most 2000 ms, but is {}",
            config.window.animation_duration
        ));
    }
    if config.window.hotkey.trim().is_empty() {
        return Err("window.hotkey must not be empty".into());
    }
    if config
        .window
        .monitor
        .entries()
        .iter()
        .any(|monitor| monitor.as_str().is_empty())
    {
        return Err("window.monitor must not contain an empty name".into());
    }
    Ok(())
}

fn check_fraction(key: &str, value: f64, min: f64) -> Result<(), String> {
    if (min..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{key} must be between {min} and 1.0, but is {value}"
        ))
    }
}

fn check_range(key: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{key} must be between {min} and {max}, but is {value}"
        ))
    }
}

/// The default location of the configuration file.
pub fn default_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

/// The directory searched for theme files.
pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("themes"))
}

/// Reads the configuration file; a missing file yields the defaults.
///
/// # Errors
///
/// Fails when the file exists but cannot be read or is invalid.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(ConfigError::read(path, &error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(text: &str) -> Result<Config, ConfigError> {
        parse(text, Path::new("config.toml"))
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(parse_str("").expect("valid"), Config::default());
    }

    #[test]
    fn documented_default_file_matches_the_built_in_defaults() {
        assert_eq!(
            parse_str(DEFAULT_CONFIG).expect("default file parses"),
            Config::default()
        );
    }

    #[test]
    fn values_override_defaults() {
        let config = parse_str(
            r#"
            [font]
            family = "JetBrains Mono"
            size = 13

            [appearance]
            theme = { dark = "dusk", light = "dawn" }
            tab_bar = "left"
            tab_width = "fit"

            [terminal]
            scrollback = "unlimited"

            [window]
            align = "right"
            vertical_align = "bottom"
            animation = "slide-bottom"

            [keybindings]
            "ctrl+shift+t" = "new-tab"
            "ctrl+alt+l" = { send = "clear\r" }
            "#,
        )
        .expect("valid");
        assert_eq!(config.font.family, "JetBrains Mono");
        assert!((config.font.size - 13.0).abs() < f32::EPSILON);
        assert_eq!(config.appearance.theme.name(true), "dawn");
        assert_eq!(config.appearance.theme.name(false), "dusk");
        assert_eq!(config.appearance.tab_bar, TabBarPosition::Left);
        assert_eq!(config.appearance.tab_width, TabWidth::Fit);
        assert_eq!(config.terminal.scrollback.lines(), usize::MAX);
        assert_eq!(config.window.align, WindowAlign::Right);
        assert_eq!(config.window.vertical_align, WindowVerticalAlign::Bottom);
        assert_eq!(config.window.animation, WindowAnimation::SlideBottom);
        assert_eq!(
            config.keybindings["ctrl+shift+t"],
            Binding::Action("new-tab".into())
        );
        assert_eq!(
            config.keybindings["ctrl+alt+l"],
            Binding::Send {
                send: "clear\r".into()
            }
        );
    }

    #[test]
    fn unknown_keys_are_reported_with_their_line() {
        let error = parse_str("[font]\nsize = 12\nfamliy = \"x\"\n").expect_err("typo");
        assert_eq!(error.line(), Some(3));
        assert!(error.to_string().contains("famliy"), "{error}");
        assert!(error.to_string().contains("config.toml:3"), "{error}");
    }

    #[test]
    fn wrong_types_are_reported() {
        let error = parse_str("[appearance]\ntab_bar = \"middle\"\n").expect_err("bad value");
        assert_eq!(error.line(), Some(2));
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        for (text, key) in [
            ("[font]\nsize = 1", "font.size"),
            ("[font]\nline_height = 9", "font.line_height"),
            (
                "[appearance]\nbackground_opacity = 1.5",
                "appearance.background_opacity",
            ),
            ("[appearance]\npadding = -2", "appearance.padding"),
            ("[font]\nfamily = \"\"", "font.family"),
        ] {
            let error = parse_str(text).expect_err(key);
            assert!(error.to_string().contains(key), "{key}: {error}");
        }
    }

    #[test]
    fn window_section_configures_the_drop_down() {
        let config = parse_str(
            "[window]\nhotkey = \"<Super>grave\"\nwidth = 0.8\nheight = 0.4\nmonitor = \"DP-1\"\n\
             animation_duration = 0\nhide_on_unfocus = true\ntray_icon = false\n",
        )
        .expect("valid");
        assert_eq!(config.window.hotkey, "<Super>grave");
        assert!((config.window.width - 0.8).abs() < f64::EPSILON);
        assert_eq!(
            config.window.monitor,
            Monitors::from(Monitor::Named("DP-1".into()))
        );
        assert_eq!(config.window.animation_duration, 0);
        assert!(config.window.hide_on_unfocus);
        assert!(!config.window.tray_icon);
        assert!(Window::default().tray_icon);
        assert_eq!(
            parse_str("[window]\nmonitor = \"primary\"")
                .expect("valid")
                .window
                .monitor,
            Monitors::from(Monitor::Primary)
        );
    }

    #[test]
    fn monitors_can_be_listed_in_order_of_preference() {
        let config = parse_str("[window]\nmonitor = [\"LEN P27h-10\", \"eDP-1\", \"mouse\"]\n")
            .expect("valid");
        assert_eq!(
            config.window.monitor.entries(),
            [
                Monitor::Named("LEN P27h-10".into()),
                Monitor::Named("eDP-1".into()),
                Monitor::Pointer,
            ]
        );
    }

    #[test]
    fn a_monitor_is_named_by_connector_or_model_in_any_case() {
        let dell = Monitor::from("dell u2413");
        assert!(dell.names("DP-3", "DELL U2413"));
        assert!(!dell.names("DP-1", "DELL U2719DC"));
        assert!(Monitor::from("dp-3").names("DP-3", ""));
        assert!(!Monitor::from("").names("DP-3", ""));
        assert!(!Monitor::Pointer.names("pointer", "pointer"));
    }

    #[test]
    fn window_values_are_validated() {
        for (text, key) in [
            ("[window]\nheight = 0.0", "window.height"),
            ("[window]\nwidth = 1.5", "window.width"),
            (
                "[window]\nanimation_duration = 99999",
                "window.animation_duration",
            ),
            ("[window]\nhotkey = \"\"", "window.hotkey"),
            ("[window]\nmonitor = \" \"", "window.monitor"),
            ("[window]\nmonitor = [\"DP-1\", \"\"]", "window.monitor"),
            ("[window]\nmonitor = []", "monitors is empty"),
        ] {
            let error = parse_str(text).expect_err(key);
            assert!(error.to_string().contains(key), "{key}: {error}");
        }
    }

    #[test]
    fn session_section_controls_startup() {
        let config = parse_str("[session]\nrestore = false\nlayout = \"work\"\n").expect("valid");
        assert!(!config.session.restore);
        assert_eq!(config.session.layout, "work");
        assert!(Config::default().session.restore);
    }

    #[test]
    fn scrollback_restore_is_opt_in() {
        assert!(!Config::default().session.restore_scrollback);
        let config = parse_str("[session]\nrestore_scrollback = true\n").expect("valid");
        assert!(config.session.restore_scrollback);
    }

    #[test]
    fn update_checks_are_on_by_default_and_can_be_turned_off() {
        assert!(Config::default().updates.check);
        let config = parse_str("[updates]\ncheck = false\n").expect("valid");
        assert!(!config.updates.check);
    }

    #[test]
    fn notifications_section_controls_indicators_and_command_notifications() {
        let defaults = Config::default().notifications;
        assert!(!defaults.visual_bell);
        assert!(defaults.bell_indicator);
        assert!(!defaults.activity_indicator);
        assert!(defaults.command_finished);
        assert_eq!(defaults.command_min_duration, 10);
        let config = parse_str(
            "[notifications]\nvisual_bell = true\nbell_indicator = false\nactivity_indicator = true\n\
             command_finished = false\ncommand_min_duration = 30\n",
        )
        .expect("valid");
        assert!(config.notifications.visual_bell);
        assert!(!config.notifications.bell_indicator);
        assert!(config.notifications.activity_indicator);
        assert!(!config.notifications.command_finished);
        assert_eq!(config.notifications.command_min_duration, 30);
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let missing = Path::new("/nonexistent/dropshade/config.toml");
        assert_eq!(load(missing).expect("defaults"), Config::default());
    }

    #[test]
    fn colors_override_the_theme_by_alacritty_key() {
        let config = parse_str(
            "[colors.primary]\nbackground = \"#101010\"\n[colors.cursor]\ntext = \"0x202020\"\n\
             [colors.selection]\nbackground = \"#303030\"\n[colors.bright]\nmagenta = \"#ff00ff\"\n",
        )
        .expect("valid");
        let rgb = theme::Rgb8::hex;
        assert_eq!(config.colors.primary.background, Some(rgb(0x10_10_10)));
        assert_eq!(config.colors.primary.foreground, None);
        assert_eq!(config.colors.cursor.text, Some(rgb(0x20_20_20)));
        assert_eq!(config.colors.selection.background, Some(rgb(0x30_30_30)));
        assert_eq!(config.colors.bright.as_array()[5], Some(rgb(0xff_00_ff)));
    }

    #[test]
    fn unknown_color_names_and_bad_colors_are_rejected() {
        let error = parse_str("[colors.normal]\npurple = \"#ff00ff\"\n").expect_err("unknown");
        assert!(error.to_string().contains("purple"), "{error}");
        let error = parse_str("[colors.primary]\nbackground = \"black\"\n").expect_err("bad");
        assert!(error.to_string().contains("#rrggbb"), "{error}");
    }
}

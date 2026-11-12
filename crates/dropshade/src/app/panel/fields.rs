//! Conversions between config values and what the settings panel shows and edits.

use std::fmt::Write as _;

use dropshade_config::edit::SettingValue;
use dropshade_config::{Binding, Config, Scrollback};

/// A slider setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slider {
    FontSize,
    LineHeight,
    Opacity,
    Padding,
    Width,
    Height,
    Animation,
    CommandMinDuration,
}

/// How a slider maps onto its config key.
#[derive(Debug, Clone, Copy)]
pub struct SliderSpec {
    pub label: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl Slider {
    pub const fn spec(self) -> SliderSpec {
        let (label, section, key, min, max, step) = match self {
            Self::FontSize => ("Size", "font", "size", 8.0, 32.0, 1.0),
            Self::LineHeight => ("Line height", "font", "line_height", 0.8, 2.0, 0.05),
            Self::Opacity => (
                "Background opacity",
                "appearance",
                "background_opacity",
                0.3,
                1.0,
                0.01,
            ),
            Self::Padding => ("Padding", "appearance", "padding", 0.0, 40.0, 1.0),
            Self::Width => ("Width", "window", "width", 0.2, 1.0, 0.05),
            Self::Height => ("Height", "window", "height", 0.1, 1.0, 0.05),
            Self::Animation => (
                "Slide animation",
                "window",
                "animation_duration",
                0.0,
                1000.0,
                10.0,
            ),
            Self::CommandMinDuration => (
                "Long commands take",
                "notifications",
                "command_min_duration",
                1.0,
                600.0,
                1.0,
            ),
        };
        SliderSpec {
            label,
            section,
            key,
            min,
            max,
            step,
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn current(self, config: &Config) -> f32 {
        match self {
            Self::FontSize => config.font.size,
            Self::LineHeight => config.font.line_height,
            Self::Opacity => config.appearance.background_opacity,
            Self::Padding => config.appearance.padding,
            Self::Width => config.window.width as f32,
            Self::Height => config.window.height as f32,
            Self::Animation => config.window.animation_duration as f32,
            Self::CommandMinDuration => config.notifications.command_min_duration as f32,
        }
    }

    pub fn shown(self, value: f32) -> String {
        match self {
            Self::FontSize => format!("{value:.0}"),
            Self::LineHeight => format!("{value:.2}×"),
            Self::Opacity | Self::Width | Self::Height => format!("{:.0}%", value * 100.0),
            Self::Padding => format!("{value:.0} px"),
            Self::Animation if value < 1.0 => "off".into(),
            Self::Animation => format!("{value:.0} ms"),
            Self::CommandMinDuration => format!("{value:.0} s"),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn value(self, value: f32) -> SettingValue {
        match self {
            Self::Animation | Self::CommandMinDuration => {
                SettingValue::Integer(value.round() as i64)
            }
            _ => SettingValue::Float(f64::from(value)),
        }
    }
}

/// A free-text setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    FontFamily,
    Hotkey,
    Shell,
    ShellArgs,
    WorkingDirectory,
    Scrollback,
    ShiftEnter,
    /// The rows for adding an environment variable or a keybinding.
    EnvName,
    EnvValue,
    Chord,
    SendText,
}

impl Field {
    /// Fields that map onto one config key; the others are drafts for new list entries.
    pub const SETTINGS: [Self; 7] = [
        Self::FontFamily,
        Self::Hotkey,
        Self::Shell,
        Self::ShellArgs,
        Self::WorkingDirectory,
        Self::Scrollback,
        Self::ShiftEnter,
    ];

    pub fn key(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::FontFamily => ("font", "family"),
            Self::Hotkey => ("window", "hotkey"),
            Self::Shell => ("terminal", "shell"),
            Self::ShellArgs => ("terminal", "shell_args"),
            Self::WorkingDirectory => ("terminal", "working_directory"),
            Self::Scrollback => ("terminal", "scrollback"),
            Self::ShiftEnter => ("input", "shift_enter"),
            Self::EnvName | Self::EnvValue | Self::Chord | Self::SendText => return None,
        })
    }

    /// The text the field shows for the current configuration.
    pub fn current(self, config: &Config) -> String {
        match self {
            Self::FontFamily => config.font.family.clone(),
            Self::Hotkey => config.window.hotkey.clone(),
            Self::Shell => config.terminal.shell.clone(),
            Self::ShellArgs => join_args(&config.terminal.shell_args),
            Self::WorkingDirectory => config.terminal.working_directory.clone(),
            Self::Scrollback => match config.terminal.scrollback {
                Scrollback::Lines(lines) => lines.to_string(),
                Scrollback::Keyword(_) => String::new(),
            },
            Self::ShiftEnter => escape(&config.input.shift_enter),
            Self::EnvName | Self::EnvValue | Self::Chord | Self::SendText => String::new(),
        }
    }

    /// The config value for what the user typed.
    pub fn parse(self, text: &str) -> Result<SettingValue, String> {
        match self {
            Self::ShellArgs => split_args(text).map(SettingValue::List),
            Self::Scrollback => text
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|lines| *lines >= 0)
                .map(SettingValue::Integer)
                .ok_or_else(|| format!("\"{}\" is not a number of lines", text.trim())),
            Self::ShiftEnter | Self::SendText => unescape(text).map(SettingValue::Text),
            Self::FontFamily | Self::Hotkey | Self::Shell | Self::WorkingDirectory => {
                Ok(SettingValue::Text(text.trim().to_owned()))
            }
            Self::EnvName | Self::EnvValue | Self::Chord => Ok(SettingValue::Text(text.to_owned())),
        }
    }
}

/// A toggle setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    FollowSystem,
    CursorBlink,
    Scrollbar,
    HideOnUnfocus,
    UnlimitedScrollback,
    Ime,
    CopyOnSelect,
    Restore,
    CheckUpdates,
    VisualBell,
    BellIndicator,
    ActivityIndicator,
    CommandFinished,
}

impl Switch {
    pub fn current(self, config: &Config) -> bool {
        match self {
            Self::FollowSystem => matches!(
                config.appearance.theme,
                dropshade_config::ThemeChoice::FollowSystem { .. }
            ),
            Self::CursorBlink => config.appearance.cursor_blink,
            Self::Scrollbar => config.appearance.scrollbar,
            Self::HideOnUnfocus => config.window.hide_on_unfocus,
            Self::UnlimitedScrollback => {
                matches!(config.terminal.scrollback, Scrollback::Keyword(_))
            }
            Self::Ime => config.input.ime,
            Self::CopyOnSelect => config.input.copy_on_select,
            Self::Restore => config.session.restore,
            Self::CheckUpdates => config.updates.check,
            Self::VisualBell => config.notifications.visual_bell,
            Self::BellIndicator => config.notifications.bell_indicator,
            Self::ActivityIndicator => config.notifications.activity_indicator,
            Self::CommandFinished => config.notifications.command_finished,
        }
    }

    /// The plain boolean keys; the others write something more involved.
    pub fn key(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::CursorBlink => ("appearance", "cursor_blink"),
            Self::Scrollbar => ("appearance", "scrollbar"),
            Self::HideOnUnfocus => ("window", "hide_on_unfocus"),
            Self::Ime => ("input", "ime"),
            Self::CopyOnSelect => ("input", "copy_on_select"),
            Self::Restore => ("session", "restore"),
            Self::CheckUpdates => ("updates", "check"),
            Self::VisualBell => ("notifications", "visual_bell"),
            Self::BellIndicator => ("notifications", "bell_indicator"),
            Self::ActivityIndicator => ("notifications", "activity_indicator"),
            Self::CommandFinished => ("notifications", "command_finished"),
            Self::FollowSystem | Self::UnlimitedScrollback => return None,
        })
    }
}

/// What a keybinding row shows next to its chord.
pub fn describe_binding(binding: &Binding) -> String {
    match binding {
        Binding::Action(name) if name.trim().eq_ignore_ascii_case("none") => {
            "passed to the terminal".into()
        }
        Binding::Action(name) => name.clone(),
        Binding::Send { send } => format!("sends \"{}\"", escape(send)),
    }
}

/// Every action a keybinding can be set to, in the order the picker lists them.
pub fn action_names() -> Vec<String> {
    let tabs = (1..=9).map(|n| format!("go-to-tab-{n}"));
    dropshade_term::Shortcut::NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .chain(tabs)
        .chain(std::iter::once("none".to_owned()))
        .collect()
}

fn join_args(args: &[String]) -> String {
    shlex::try_join(args.iter().map(String::as_str)).unwrap_or_else(|_| args.join(" "))
}

fn split_args(text: &str) -> Result<Vec<String>, String> {
    shlex::split(text).ok_or_else(|| "the arguments have an unclosed quote".to_owned())
}

/// Shows control characters as escapes (`\e`, `\r`, `\x07`), so they can be seen and edited.
pub fn escape(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{1b}' => shown.push_str("\\e"),
            '\r' => shown.push_str("\\r"),
            '\n' => shown.push_str("\\n"),
            '\t' => shown.push_str("\\t"),
            '\\' => shown.push_str("\\\\"),
            c if c.is_ascii_control() => {
                let _ = write!(shown, "\\x{:02x}", u32::from(c));
            }
            c => shown.push(c),
        }
    }
    shown
}

/// The inverse of [`escape`].
pub fn unescape(text: &str) -> Result<String, String> {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('e') => result.push('\u{1b}'),
            Some('r') => result.push('\r'),
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('\\') => result.push('\\'),
            Some('x') => {
                let digits: String = chars.by_ref().take(2).collect();
                let byte = u8::from_str_radix(&digits, 16)
                    .ok()
                    .filter(u8::is_ascii)
                    .ok_or_else(|| format!("\\x{digits} is not an ASCII code like \\x07"))?;
                result.push(char::from(byte));
            }
            Some(other) => return Err(format!("unknown escape \\{other}")),
            None => return Err("a trailing \\ escapes nothing".into()),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_round_trip_through_escapes() {
        for text in [
            "\u{1b}\r",
            "clear\n",
            "tab\there",
            "back\\slash",
            "bell\u{7}",
            "é",
        ] {
            assert_eq!(unescape(&escape(text)).as_deref(), Ok(text), "{text:?}");
        }
        assert_eq!(escape("\u{1b}\r"), "\\e\\r");
        assert_eq!(unescape("\\x1b[A").as_deref(), Ok("\u{1b}[A"));
    }

    #[test]
    fn bad_escapes_are_reported() {
        assert!(unescape("\\q").is_err());
        assert!(unescape("end\\").is_err());
        assert!(unescape("\\xzz").is_err());
        assert!(unescape("\\xff").is_err());
    }

    #[test]
    fn shell_arguments_keep_their_quoting() {
        let args = vec!["-c".to_owned(), "echo hi".to_owned()];
        let shown = join_args(&args);
        assert_eq!(split_args(&shown), Ok(args));
        assert!(split_args("\"open").is_err());
    }

    #[test]
    fn scrollback_must_be_a_line_count() {
        assert_eq!(
            Field::Scrollback.parse(" 5000 "),
            Ok(SettingValue::Integer(5000))
        );
        assert!(Field::Scrollback.parse("lots").is_err());
        assert!(Field::Scrollback.parse("-1").is_err());
    }

    #[test]
    fn every_setting_field_shows_the_defaults() {
        let config = Config::default();
        assert_eq!(Field::ShiftEnter.current(&config), "\\e\\r");
        assert_eq!(Field::Scrollback.current(&config), "10000");
        assert_eq!(Field::FontFamily.current(&config), "monospace");
        for field in Field::SETTINGS {
            assert!(field.key().is_some(), "{field:?}");
            let text = field.current(&config);
            assert!(field.parse(&text).is_ok(), "{field:?}: {text:?}");
        }
    }

    #[test]
    fn slider_values_match_the_config_types() {
        assert_eq!(Slider::Animation.value(179.6), SettingValue::Integer(180));
        assert_eq!(Slider::Animation.shown(0.0), "off");
        assert_eq!(Slider::Width.shown(0.5), "50%");
        assert_eq!(Slider::CommandMinDuration.shown(30.0), "30 s");
        assert_eq!(
            Slider::CommandMinDuration.value(29.7),
            SettingValue::Integer(30)
        );
        let config = Config::default();
        for slider in [
            Slider::FontSize,
            Slider::LineHeight,
            Slider::Opacity,
            Slider::Padding,
            Slider::Width,
            Slider::Height,
            Slider::Animation,
            Slider::CommandMinDuration,
        ] {
            let spec = slider.spec();
            let value = slider.current(&config);
            assert!((spec.min..=spec.max).contains(&value), "{slider:?}");
        }
    }

    #[test]
    fn actions_list_parses_and_bindings_are_described() {
        for name in action_names().iter().filter(|name| *name != "none") {
            assert!(name.parse::<dropshade_term::Shortcut>().is_ok(), "{name}");
        }
        assert_eq!(
            describe_binding(&Binding::Send {
                send: "clear\r".into()
            }),
            "sends \"clear\\r\""
        );
        assert_eq!(
            describe_binding(&Binding::Action("None".into())),
            "passed to the terminal"
        );
    }
}

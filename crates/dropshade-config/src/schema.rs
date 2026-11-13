//! The configuration file's structure. Every section and key is optional; unknown keys are
//! rejected so typos are reported instead of silently ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::theme::Rgb8;

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub font: Font,
    pub appearance: Appearance,
    /// Colors that replace the theme's, laid out like an Alacritty theme.
    pub colors: ColorOverrides,
    pub terminal: Terminal,
    pub input: Input,
    pub window: Window,
    pub session: Session,
    pub updates: Updates,
    pub notifications: Notifications,
    /// Chord (e.g. `"ctrl+shift+t"`) to action name or `{ send = "text" }`.
    pub keybindings: BTreeMap<String, Binding>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
    /// An installed font family, or "monospace" for the system's default monospace font.
    pub family: String,
    pub size: f32,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: "monospace".into(),
            size: 15.0,
            line_height: 1.25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub theme: ThemeChoice,
    pub background_opacity: f32,
    pub padding: f32,
    pub tab_bar: TabBarPosition,
    pub tab_width: TabWidth,
    pub cursor_shape: CursorShape,
    pub cursor_blink: bool,
    pub scrollbar: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            background_opacity: 0.94,
            padding: 10.0,
            tab_bar: TabBarPosition::Top,
            tab_width: TabWidth::Fixed,
            cursor_shape: CursorShape::Block,
            cursor_blink: false,
            scrollbar: true,
        }
    }
}

/// One theme for everything, or a pair that follows the system's light/dark preference.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ThemeChoice {
    Single(String),
    FollowSystem { dark: String, light: String },
}

impl Default for ThemeChoice {
    fn default() -> Self {
        Self::Single("dusk".into())
    }
}

impl ThemeChoice {
    /// The theme name to use given whether the system prefers a light appearance.
    pub fn name(&self, prefers_light: bool) -> &str {
        match self {
            Self::Single(name) => name,
            Self::FollowSystem { light, .. } if prefers_light => light,
            Self::FollowSystem { dark, .. } => dark,
        }
    }
}

/// Colors set on top of the theme. Every color is optional; unset ones come from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorOverrides {
    pub primary: PrimaryColors,
    pub cursor: CursorColors,
    pub selection: SelectionColors,
    pub normal: AnsiColors,
    pub bright: AnsiColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrimaryColors {
    pub foreground: Option<Rgb8>,
    pub background: Option<Rgb8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CursorColors {
    pub cursor: Option<Rgb8>,
    /// Text under a block cursor.
    pub text: Option<Rgb8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionColors {
    pub text: Option<Rgb8>,
    pub background: Option<Rgb8>,
}

/// The eight ANSI colors of one row (normal or bright).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnsiColors {
    pub black: Option<Rgb8>,
    pub red: Option<Rgb8>,
    pub green: Option<Rgb8>,
    pub yellow: Option<Rgb8>,
    pub blue: Option<Rgb8>,
    pub magenta: Option<Rgb8>,
    pub cyan: Option<Rgb8>,
    pub white: Option<Rgb8>,
}

impl AnsiColors {
    /// Key names in palette order, as written in the file.
    pub const NAMES: [&'static str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];

    /// The colors in palette order (black first).
    pub const fn as_array(&self) -> [Option<Rgb8>; 8] {
        [
            self.black,
            self.red,
            self.green,
            self.yellow,
            self.blue,
            self.magenta,
            self.cyan,
            self.white,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabBarPosition {
    Top,
    Bottom,
    Left,
    Right,
}

/// How wide the tabs of a top or bottom tab bar are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabWidth {
    /// Every tab gets the same share of the bar.
    Fixed,
    /// Each tab is as wide as its name, up to the same maximum.
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Terminal {
    /// Program to run; empty means the user's login shell.
    pub shell: String,
    pub shell_args: Vec<String>,
    pub scrollback: Scrollback,
    /// Jump back to the live end when new output arrives while scrolled up.
    pub scroll_on_output: bool,
    /// Jump back to the live end when a key is typed while scrolled up.
    pub scroll_on_keystroke: bool,
    /// Directory for the first tab; empty means the home directory.
    pub working_directory: String,
    pub env: BTreeMap<String, String>,
    /// Sets up bash and zsh to report prompts and commands (OSC 133).
    pub shell_integration: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            shell: String::new(),
            shell_args: Vec::new(),
            scrollback: Scrollback::Lines(10_000),
            scroll_on_output: false,
            scroll_on_keystroke: true,
            working_directory: String::new(),
            env: BTreeMap::new(),
            shell_integration: true,
        }
    }
}

/// Scrollback size: a number of lines, or "unlimited" (kept in memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Scrollback {
    Lines(usize),
    Keyword(Unlimited),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unlimited {
    Unlimited,
}

impl Scrollback {
    pub fn lines(self) -> usize {
        match self {
            Self::Lines(lines) => lines,
            Self::Keyword(Unlimited::Unlimited) => usize::MAX,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    /// Text sent for Shift+Enter outside the kitty keyboard protocol; empty sends a plain Enter.
    pub shift_enter: String,
    pub ime: bool,
    /// Also copy selections to the clipboard (they always go to the primary selection).
    pub copy_on_select: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            shift_enter: "\u{1b}\r".into(),
            ime: true,
            copy_on_select: false,
        }
    }
}

/// What Dropshade opens when it starts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Session {
    /// Reopen the tabs (names and directories) that were open when Dropshade last quit.
    pub restore: bool,
    /// Always start with this layout from ~/.config/dropshade/layouts instead; empty for none.
    pub layout: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            restore: true,
            layout: String::new(),
        }
    }
}

/// Looking for newer releases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Updates {
    /// Ask GitHub for the latest release on start and notify when it is newer.
    pub check: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self { check: true }
    }
}

/// Telling the user about tabs and commands that need attention.
#[allow(clippy::struct_excessive_bools)] // independent on/off settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    /// Flash the terminal briefly when it rings the bell.
    pub visual_bell: bool,
    /// Mark background tabs that rang the bell.
    pub bell_indicator: bool,
    /// Mark background tabs that printed output.
    pub activity_indicator: bool,
    /// Send a desktop notification when a long command finishes while Dropshade is hidden or
    /// unfocused.
    pub command_finished: bool,
    /// How many seconds a command has to run to count as long.
    pub command_min_duration: u32,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            visual_bell: false,
            bell_indicator: true,
            activity_indicator: false,
            command_finished: true,
            command_min_duration: 10,
        }
    }
}

/// How the GNOME Shell extension shows the drop-down window.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Window {
    /// Global shortcut in GNOME accelerator syntax, e.g. "F12" or "<Super>grave".
    pub hotkey: String,
    /// Width as a fraction of the monitor's work area.
    pub width: f64,
    /// Height as a fraction of the monitor's work area.
    pub height: f64,
    /// Where a window narrower than the work area sits.
    pub align: WindowAlign,
    pub monitor: Monitors,
    /// Slide animation length in milliseconds; 0 disables it.
    pub animation_duration: u32,
    /// Hide the terminal when another window gets the focus.
    pub hide_on_unfocus: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            hotkey: "F12".into(),
            width: 1.0,
            height: 0.5,
            align: WindowAlign::Center,
            monitor: Monitors::default(),
            animation_duration: 180,
            hide_on_unfocus: false,
        }
    }
}

/// The horizontal position of the drop-down on its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowAlign {
    Left,
    #[default]
    Center,
    Right,
}

impl WindowAlign {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// One entry of `window.monitor`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Monitor {
    /// The monitor under the mouse pointer.
    #[default]
    Pointer,
    Primary,
    /// A monitor by connector, such as "DP-1" or "eDP-1", or by model, such as "DELL U2413".
    Named(String),
}

impl<'de> Deserialize<'de> for Monitor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?.as_str()))
    }
}

impl From<&str> for Monitor {
    fn from(name: &str) -> Self {
        match name.trim() {
            "pointer" | "mouse" => Self::Pointer,
            "primary" => Self::Primary,
            name => Self::Named(name.to_owned()),
        }
    }
}

impl Monitor {
    /// How the entry is written in the config file.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pointer => "pointer",
            Self::Primary => "primary",
            Self::Named(name) => name,
        }
    }

    /// Whether this names the screen on `connector` showing `model`. Case does not matter.
    pub fn names(&self, connector: &str, model: &str) -> bool {
        match self {
            Self::Named(name) => {
                name.eq_ignore_ascii_case(connector)
                    || (!model.is_empty() && name.eq_ignore_ascii_case(model))
            }
            Self::Pointer | Self::Primary => false,
        }
    }
}

/// The monitors the terminal drops down on, most preferred first: the first one that is
/// connected is used. Written as one string or a list, and never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitors(Vec<Monitor>);

impl Monitors {
    /// `None` for an empty list.
    pub fn new(entries: Vec<Monitor>) -> Option<Self> {
        (!entries.is_empty()).then_some(Self(entries))
    }

    pub fn entries(&self) -> &[Monitor] {
        &self.0
    }
}

impl Default for Monitors {
    fn default() -> Self {
        Self(vec![Monitor::Pointer])
    }
}

impl From<Monitor> for Monitors {
    fn from(monitor: Monitor) -> Self {
        Self(vec![monitor])
    }
}

impl<'de> Deserialize<'de> for Monitors {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(Monitor),
            Many(Vec<Monitor>),
        }
        let entries = match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(monitor) => vec![monitor],
            OneOrMany::Many(entries) => entries,
        };
        Self::new(entries).ok_or_else(|| serde::de::Error::custom("the list of monitors is empty"))
    }
}

/// What a key chord does: a named action (or "none" to unbind), or text to send.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Action(String),
    Send { send: String },
}

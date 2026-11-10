//! The configuration file's structure. Every section and key is optional; unknown keys are
//! rejected so typos are reported instead of silently ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub font: Font,
    pub appearance: Appearance,
    pub terminal: Terminal,
    pub input: Input,
    pub window: Window,
    pub session: Session,
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
    /// Directory for the first tab; empty means the home directory.
    pub working_directory: String,
    pub env: BTreeMap<String, String>,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            shell: String::new(),
            shell_args: Vec::new(),
            scrollback: Scrollback::Lines(10_000),
            working_directory: String::new(),
            env: BTreeMap::new(),
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
    pub monitor: Monitor,
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
            monitor: Monitor::default(),
            animation_duration: 180,
            hide_on_unfocus: false,
        }
    }
}

/// The monitor the terminal drops down on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Monitor {
    /// The monitor under the mouse pointer.
    #[default]
    Pointer,
    Primary,
    /// A monitor by connector name, such as "DP-1" or "eDP-1".
    Connector(String),
}

impl<'de> Deserialize<'de> for Monitor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(match name.trim() {
            "pointer" | "mouse" => Self::Pointer,
            "primary" => Self::Primary,
            connector => Self::Connector(connector.to_owned()),
        })
    }
}

impl Monitor {
    /// The value sent to the GNOME Shell extension.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pointer => "pointer",
            Self::Primary => "primary",
            Self::Connector(name) => name,
        }
    }
}

/// What a key chord does: a named action (or "none" to unbind), or text to send.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Action(String),
    Send { send: String },
}

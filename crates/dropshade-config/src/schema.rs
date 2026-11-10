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
    pub cursor_shape: CursorShape,
    pub cursor_blink: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            background_opacity: 0.94,
            padding: 10.0,
            tab_bar: TabBarPosition::Top,
            cursor_shape: CursorShape::Block,
            cursor_blink: false,
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

/// What a key chord does: a named action (or "none" to unbind), or text to send.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Binding {
    Action(String),
    Send { send: String },
}

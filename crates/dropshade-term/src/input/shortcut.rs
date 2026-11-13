//! Terminal shortcuts that are handled locally instead of being sent to the application.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// Which neighbouring pane, or which way a divider moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    Paste,
    PastePrimary,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    MoveTabLeft,
    MoveTabRight,
    RenameTab,
    Search,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Fills the work area, or returns to the configured size.
    ToggleMaximize,
    /// Scrolls the prompt above the top of the viewport into view (needs shell integration).
    PreviousPrompt,
    /// Scrolls the next prompt into view, or returns to the live end after the newest one.
    NextPrompt,
    /// Selects the output of the newest command (needs shell integration).
    SelectCommandOutput,
    /// Opens the settings panel.
    OpenSettings,
    /// Turns vi mode (keyboard selection) on or off.
    ToggleViMode,
    /// Activates the tab at this zero-based index.
    GoToTab(u8),
    /// Splits the focused pane, putting a new terminal to its right.
    SplitRight,
    /// Splits the focused pane, putting a new terminal below it.
    SplitDown,
    /// Closes the focused pane, or the tab when it is the only one.
    ClosePane,
    /// Gives the neighbouring pane the keyboard.
    FocusPane(PaneDirection),
    /// Moves the focused pane's divider.
    ResizePane(PaneDirection),
    /// Fills the tab with the focused pane, or shows all panes again.
    TogglePaneZoom,
}

impl Shortcut {
    /// Every action name a keybinding accepts, apart from `go-to-tab-1` to `go-to-tab-9`.
    pub const NAMES: [&'static str; 36] = [
        "copy",
        "paste",
        "paste-primary",
        "scroll-page-up",
        "scroll-page-down",
        "scroll-to-top",
        "scroll-to-bottom",
        "new-tab",
        "close-tab",
        "next-tab",
        "previous-tab",
        "move-tab-left",
        "move-tab-right",
        "rename-tab",
        "search",
        "zoom-in",
        "zoom-out",
        "zoom-reset",
        "toggle-maximize",
        "previous-prompt",
        "next-prompt",
        "select-command-output",
        "open-settings",
        "toggle-vi-mode",
        "split-right",
        "split-down",
        "close-pane",
        "focus-pane-left",
        "focus-pane-right",
        "focus-pane-up",
        "focus-pane-down",
        "resize-pane-left",
        "resize-pane-right",
        "resize-pane-up",
        "resize-pane-down",
        "toggle-pane-zoom",
    ];

    /// Shortcuts that only mean something in a split tab. Elsewhere their keys reach the
    /// program, so a lone pane keeps Alt+Arrows for the shell.
    pub fn needs_split(self) -> bool {
        matches!(
            self,
            Self::FocusPane(_) | Self::ResizePane(_) | Self::TogglePaneZoom
        )
    }
}

impl std::str::FromStr for Shortcut {
    type Err = String;

    /// Parses an action name such as `new-tab` or `go-to-tab-3`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let name = name.trim().to_ascii_lowercase();
        if let Some(number) = name.strip_prefix("go-to-tab-") {
            return number
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=9).contains(n))
                .map(|n| Self::GoToTab(n - 1))
                .ok_or_else(|| {
                    format!("unknown action \"{name}\" (tabs go from go-to-tab-1 to go-to-tab-9)")
                });
        }
        let shortcut = match name.as_str() {
            "copy" => Self::Copy,
            "paste" => Self::Paste,
            "paste-primary" => Self::PastePrimary,
            "scroll-page-up" => Self::ScrollPageUp,
            "scroll-page-down" => Self::ScrollPageDown,
            "scroll-to-top" => Self::ScrollToTop,
            "scroll-to-bottom" => Self::ScrollToBottom,
            "new-tab" => Self::NewTab,
            "close-tab" => Self::CloseTab,
            "next-tab" => Self::NextTab,
            "previous-tab" => Self::PreviousTab,
            "move-tab-left" => Self::MoveTabLeft,
            "move-tab-right" => Self::MoveTabRight,
            "rename-tab" => Self::RenameTab,
            "search" => Self::Search,
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
            "zoom-reset" => Self::ZoomReset,
            "toggle-maximize" => Self::ToggleMaximize,
            "previous-prompt" => Self::PreviousPrompt,
            "next-prompt" => Self::NextPrompt,
            "select-command-output" => Self::SelectCommandOutput,
            "open-settings" => Self::OpenSettings,
            "toggle-vi-mode" => Self::ToggleViMode,
            "split-right" => Self::SplitRight,
            "split-down" => Self::SplitDown,
            "close-pane" => Self::ClosePane,
            "focus-pane-left" => Self::FocusPane(PaneDirection::Left),
            "focus-pane-right" => Self::FocusPane(PaneDirection::Right),
            "focus-pane-up" => Self::FocusPane(PaneDirection::Up),
            "focus-pane-down" => Self::FocusPane(PaneDirection::Down),
            "resize-pane-left" => Self::ResizePane(PaneDirection::Left),
            "resize-pane-right" => Self::ResizePane(PaneDirection::Right),
            "resize-pane-up" => Self::ResizePane(PaneDirection::Up),
            "resize-pane-down" => Self::ResizePane(PaneDirection::Down),
            "toggle-pane-zoom" => Self::TogglePaneZoom,
            _ => return Err(format!("unknown action \"{name}\"")),
        };
        Ok(shortcut)
    }
}

/// The shortcut bound to a key press, if any.
pub fn shortcut(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    match key {
        Key::Character(c) if ctrl && shift && !alt => ctrl_shift_character(c),
        Key::Character(c) if ctrl && !alt && !shift && c == "," => Some(Shortcut::OpenSettings),
        Key::Character(c) if ctrl && !alt => ctrl_zoom(c),
        Key::Character(c) if alt && !ctrl && !shift => alt_digit(c),
        Key::Named(named) if ctrl && !alt => ctrl_named(*named, shift),
        Key::Named(named) if alt && !ctrl && !shift => arrow(*named).map(Shortcut::FocusPane),
        Key::Named(named) if ctrl && shift && alt => arrow(*named).map(Shortcut::ResizePane),
        Key::Named(named) if shift && !ctrl && !alt => shift_named(*named),
        Key::Named(Named::F11) if modifiers.is_empty() => Some(Shortcut::ToggleMaximize),
        Key::Named(Named::F2) if modifiers.is_empty() => Some(Shortcut::RenameTab),
        _ => None,
    }
}

fn ctrl_shift_character(c: &str) -> Option<Shortcut> {
    match c.to_lowercase().as_str() {
        "c" => Some(Shortcut::Copy),
        "v" => Some(Shortcut::Paste),
        "t" => Some(Shortcut::NewTab),
        "w" => Some(Shortcut::ClosePane),
        "x" => Some(Shortcut::TogglePaneZoom),
        "r" => Some(Shortcut::SplitRight),
        "d" => Some(Shortcut::SplitDown),
        "f" => Some(Shortcut::Search),
        "a" => Some(Shortcut::SelectCommandOutput),
        _ => ctrl_zoom(c),
    }
}

fn arrow(named: Named) -> Option<PaneDirection> {
    match named {
        Named::ArrowLeft => Some(PaneDirection::Left),
        Named::ArrowRight => Some(PaneDirection::Right),
        Named::ArrowUp => Some(PaneDirection::Up),
        Named::ArrowDown => Some(PaneDirection::Down),
        _ => None,
    }
}

/// Zoom keys, which work with or without Shift so `Ctrl+=` and `Ctrl++` both zoom in.
fn ctrl_zoom(c: &str) -> Option<Shortcut> {
    match c {
        "=" | "+" => Some(Shortcut::ZoomIn),
        "-" | "_" => Some(Shortcut::ZoomOut),
        "0" | ")" => Some(Shortcut::ZoomReset),
        _ => None,
    }
}

fn alt_digit(c: &str) -> Option<Shortcut> {
    let digit = c
        .parse::<u8>()
        .ok()
        .filter(|digit| (1..=9).contains(digit))?;
    Some(Shortcut::GoToTab(digit - 1))
}

fn ctrl_named(named: Named, shift: bool) -> Option<Shortcut> {
    match (named, shift) {
        (Named::PageDown | Named::Tab, false) => Some(Shortcut::NextTab),
        (Named::PageUp, false) | (Named::Tab, true) => Some(Shortcut::PreviousTab),
        (Named::PageUp, true) => Some(Shortcut::MoveTabLeft),
        (Named::PageDown, true) => Some(Shortcut::MoveTabRight),
        (Named::ArrowUp, true) => Some(Shortcut::PreviousPrompt),
        (Named::ArrowDown, true) => Some(Shortcut::NextPrompt),
        (Named::Space, true) => Some(Shortcut::ToggleViMode),
        _ => None,
    }
}

fn shift_named(named: Named) -> Option<Shortcut> {
    match named {
        Named::Insert => Some(Shortcut::PastePrimary),
        Named::PageUp => Some(Shortcut::ScrollPageUp),
        Named::PageDown => Some(Shortcut::ScrollPageDown),
        Named::Home => Some(Shortcut::ScrollToTop),
        Named::End => Some(Shortcut::ScrollToBottom),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(c: &str, modifiers: Modifiers) -> Option<Shortcut> {
        shortcut(&Key::Character(c.into()), modifiers)
    }

    fn named(named: Named, modifiers: Modifiers) -> Option<Shortcut> {
        shortcut(&Key::Named(named), modifiers)
    }

    #[test]
    fn clipboard_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(character("c", ctrl_shift), Some(Shortcut::Copy));
        assert_eq!(character("C", ctrl_shift), Some(Shortcut::Copy));
        assert_eq!(character("v", ctrl_shift), Some(Shortcut::Paste));
        assert_eq!(
            named(Named::Insert, Modifiers::SHIFT),
            Some(Shortcut::PastePrimary)
        );
    }

    #[test]
    fn scrollback_shortcuts() {
        assert_eq!(
            named(Named::PageUp, Modifiers::SHIFT),
            Some(Shortcut::ScrollPageUp)
        );
        assert_eq!(
            named(Named::PageDown, Modifiers::SHIFT),
            Some(Shortcut::ScrollPageDown)
        );
        assert_eq!(
            named(Named::Home, Modifiers::SHIFT),
            Some(Shortcut::ScrollToTop)
        );
        assert_eq!(
            named(Named::End, Modifiers::SHIFT),
            Some(Shortcut::ScrollToBottom)
        );
    }

    #[test]
    fn tab_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(character("t", ctrl_shift), Some(Shortcut::NewTab));
        assert_eq!(character("w", ctrl_shift), Some(Shortcut::ClosePane));
        assert_eq!("close-tab".parse(), Ok(Shortcut::CloseTab));
        assert_eq!(
            named(Named::F2, Modifiers::empty()),
            Some(Shortcut::RenameTab)
        );
        assert_eq!(named(Named::F2, Modifiers::SHIFT), None);
        assert_eq!(character("f", ctrl_shift), Some(Shortcut::Search));
        assert_eq!("search".parse(), Ok(Shortcut::Search));
        assert_eq!(
            named(Named::PageDown, Modifiers::CTRL),
            Some(Shortcut::NextTab)
        );
        assert_eq!(
            named(Named::PageUp, Modifiers::CTRL),
            Some(Shortcut::PreviousTab)
        );
        assert_eq!(named(Named::Tab, Modifiers::CTRL), Some(Shortcut::NextTab));
        assert_eq!(named(Named::Tab, ctrl_shift), Some(Shortcut::PreviousTab));
        assert_eq!(
            named(Named::PageUp, ctrl_shift),
            Some(Shortcut::MoveTabLeft)
        );
        assert_eq!(
            named(Named::PageDown, ctrl_shift),
            Some(Shortcut::MoveTabRight)
        );
    }

    #[test]
    fn zoom_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(character("=", Modifiers::CTRL), Some(Shortcut::ZoomIn));
        assert_eq!(character("+", ctrl_shift), Some(Shortcut::ZoomIn));
        assert_eq!(character("-", Modifiers::CTRL), Some(Shortcut::ZoomOut));
        assert_eq!(character("0", Modifiers::CTRL), Some(Shortcut::ZoomReset));
        assert_eq!(character("=", Modifiers::empty()), None);
        assert_eq!("zoom-in".parse(), Ok(Shortcut::ZoomIn));
        assert_eq!("zoom-out".parse(), Ok(Shortcut::ZoomOut));
        assert_eq!("zoom-reset".parse(), Ok(Shortcut::ZoomReset));
    }

    #[test]
    fn f11_toggles_maximize() {
        assert_eq!(
            named(Named::F11, Modifiers::empty()),
            Some(Shortcut::ToggleMaximize)
        );
        assert_eq!(named(Named::F11, Modifiers::CTRL), None);
        assert_eq!("toggle-maximize".parse(), Ok(Shortcut::ToggleMaximize));
    }

    #[test]
    fn shell_integration_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            named(Named::ArrowUp, ctrl_shift),
            Some(Shortcut::PreviousPrompt)
        );
        assert_eq!(
            named(Named::ArrowDown, ctrl_shift),
            Some(Shortcut::NextPrompt)
        );
        assert_eq!(named(Named::ArrowUp, Modifiers::CTRL), None);
        assert_eq!(
            character("a", ctrl_shift),
            Some(Shortcut::SelectCommandOutput)
        );
        assert_eq!("previous-prompt".parse(), Ok(Shortcut::PreviousPrompt));
        assert_eq!("next-prompt".parse(), Ok(Shortcut::NextPrompt));
        assert_eq!(
            "select-command-output".parse(),
            Ok(Shortcut::SelectCommandOutput)
        );
    }

    #[test]
    fn ctrl_comma_opens_settings() {
        assert_eq!(
            character(",", Modifiers::CTRL),
            Some(Shortcut::OpenSettings)
        );
        assert_eq!(character(",", Modifiers::empty()), None);
        assert_eq!(character(",", Modifiers::CTRL | Modifiers::ALT), None);
        assert_eq!("open-settings".parse(), Ok(Shortcut::OpenSettings));
    }

    #[test]
    fn ctrl_shift_space_toggles_vi_mode() {
        assert_eq!(
            named(Named::Space, Modifiers::CTRL | Modifiers::SHIFT),
            Some(Shortcut::ToggleViMode)
        );
        assert_eq!(named(Named::Space, Modifiers::CTRL), None);
        assert_eq!("toggle-vi-mode".parse(), Ok(Shortcut::ToggleViMode));
    }

    #[test]
    fn alt_digits_go_to_tabs() {
        assert_eq!(character("1", Modifiers::ALT), Some(Shortcut::GoToTab(0)));
        assert_eq!(character("9", Modifiers::ALT), Some(Shortcut::GoToTab(8)));
        assert_eq!(character("0", Modifiers::ALT), None);
        assert_eq!(character("x", Modifiers::ALT), None);
    }

    #[test]
    fn split_shortcuts() {
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let ctrl_shift_alt = ctrl_shift | Modifiers::ALT;
        assert_eq!(character("r", ctrl_shift), Some(Shortcut::SplitRight));
        assert_eq!(character("D", ctrl_shift), Some(Shortcut::SplitDown));
        assert_eq!(
            character("d", Modifiers::CTRL | Modifiers::ALT),
            None,
            "Ctrl+Alt+D is the desktop's"
        );
        assert_eq!(character("x", ctrl_shift), Some(Shortcut::TogglePaneZoom));
        assert_eq!(
            named(Named::ArrowLeft, Modifiers::ALT),
            Some(Shortcut::FocusPane(PaneDirection::Left))
        );
        assert_eq!(
            named(Named::ArrowDown, Modifiers::ALT),
            Some(Shortcut::FocusPane(PaneDirection::Down))
        );
        assert_eq!(
            named(Named::ArrowRight, ctrl_shift_alt),
            Some(Shortcut::ResizePane(PaneDirection::Right))
        );
        assert_eq!(
            named(Named::ArrowUp, ctrl_shift_alt),
            Some(Shortcut::ResizePane(PaneDirection::Up))
        );
        assert_eq!(named(Named::Enter, Modifiers::ALT), None);
        assert_eq!("split-right".parse(), Ok(Shortcut::SplitRight));
        assert_eq!(
            "resize-pane-down".parse(),
            Ok(Shortcut::ResizePane(PaneDirection::Down))
        );
        assert!(Shortcut::FocusPane(PaneDirection::Up).needs_split());
        assert!(Shortcut::TogglePaneZoom.needs_split());
        assert!(!Shortcut::SplitRight.needs_split());
        assert!(!Shortcut::ClosePane.needs_split());
    }

    #[test]
    fn ordinary_keys_pass_through() {
        assert_eq!(character("c", Modifiers::CTRL), None);
        assert_eq!(named(Named::PageUp, Modifiers::empty()), None);
        assert_eq!(named(Named::Insert, Modifiers::empty()), None);
    }

    #[test]
    fn every_listed_name_parses() {
        for name in Shortcut::NAMES {
            assert!(name.parse::<Shortcut>().is_ok(), "{name}");
        }
    }
}

//! Terminal shortcuts that are handled locally instead of being sent to the application.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

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
    /// Activates the tab at this zero-based index.
    GoToTab(u8),
}

impl Shortcut {
    /// Every action name a keybinding accepts, apart from `go-to-tab-1` to `go-to-tab-9`.
    pub const NAMES: [&'static str; 15] = [
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
    ];
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
        Key::Character(c) if alt && !ctrl && !shift => alt_digit(c),
        Key::Named(named) if ctrl && !alt => ctrl_named(*named, shift),
        Key::Named(named) if shift && !ctrl && !alt => shift_named(*named),
        _ => None,
    }
}

fn ctrl_shift_character(c: &str) -> Option<Shortcut> {
    match c.to_lowercase().as_str() {
        "c" => Some(Shortcut::Copy),
        "v" => Some(Shortcut::Paste),
        "t" => Some(Shortcut::NewTab),
        "w" => Some(Shortcut::CloseTab),
        "r" => Some(Shortcut::RenameTab),
        "f" => Some(Shortcut::Search),
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
        assert_eq!(character("w", ctrl_shift), Some(Shortcut::CloseTab));
        assert_eq!(character("r", ctrl_shift), Some(Shortcut::RenameTab));
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
    fn alt_digits_go_to_tabs() {
        assert_eq!(character("1", Modifiers::ALT), Some(Shortcut::GoToTab(0)));
        assert_eq!(character("9", Modifiers::ALT), Some(Shortcut::GoToTab(8)));
        assert_eq!(character("0", Modifiers::ALT), None);
        assert_eq!(character("x", Modifiers::ALT), None);
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

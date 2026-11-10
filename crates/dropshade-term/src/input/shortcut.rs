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
    /// Activates the tab at this zero-based index.
    GoToTab(u8),
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
}

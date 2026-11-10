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
}

/// The shortcut bound to a key press, if any.
pub fn shortcut(key: &Key, modifiers: Modifiers) -> Option<Shortcut> {
    let ctrl_shift = modifiers.control() && modifiers.shift() && !modifiers.alt();
    let shift_only = modifiers.shift() && !modifiers.control() && !modifiers.alt();
    match key {
        Key::Character(c) if ctrl_shift => match c.to_lowercase().as_str() {
            "c" => Some(Shortcut::Copy),
            "v" => Some(Shortcut::Paste),
            _ => None,
        },
        Key::Named(named) if shift_only => match named {
            Named::Insert => Some(Shortcut::PastePrimary),
            Named::PageUp => Some(Shortcut::ScrollPageUp),
            Named::PageDown => Some(Shortcut::ScrollPageDown),
            Named::Home => Some(Shortcut::ScrollToTop),
            Named::End => Some(Shortcut::ScrollToBottom),
            _ => None,
        },
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
    fn ordinary_keys_pass_through() {
        assert_eq!(character("c", Modifiers::CTRL), None);
        assert_eq!(named(Named::PageUp, Modifiers::empty()), None);
        assert_eq!(named(Named::Insert, Modifiers::empty()), None);
    }
}

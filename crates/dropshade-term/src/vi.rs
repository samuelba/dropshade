//! Vi mode: moving a cursor through the output and scrollback with the keyboard to select text.

use alacritty_terminal::vi_mode::ViMotion;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

use crate::input::KeyPress;
use crate::terminal::SearchDirection;

/// How a vi mode selection grows from where it was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViSelection {
    /// Cell by cell (`v`).
    Simple,
    /// Whole lines (`V`).
    Line,
    /// A rectangle (`Ctrl+v`).
    Block,
    /// Whole words (`Alt+v`).
    Semantic,
}

/// Scrolling the view in vi mode; the cursor moves along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViScroll {
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
}

/// What a key press does in vi mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViAction {
    Motion(ViMotion),
    /// Starts a selection of this kind, switches to it, or clears it when it is already the one.
    Select(ViSelection),
    /// Copies the selection and leaves vi mode.
    Yank,
    /// Clears the selection, or leaves vi mode when there is none.
    Escape,
    /// Leaves vi mode.
    Exit,
    Scroll(ViScroll),
    /// To the oldest line of the scrollback (`gg`).
    Top,
    /// To the newest line of the output (`G`).
    Bottom,
    /// Opens the search bar to search towards older (`?`) or newer (`/`) output.
    Search(SearchDirection),
    /// Repeats the last search the same way (`n`) or the other way (`N`).
    SearchNext {
        reverse: bool,
    },
}

/// Resolves a key press in vi mode. `pending` holds the first key of a two-key command (`g`).
/// Keys that mean nothing in vi mode give `None`.
pub fn resolve(press: KeyPress<'_>, pending: &mut Option<char>) -> Option<ViAction> {
    let modifiers = press.modifiers;
    let previous = pending.take();
    if modifiers.logo() {
        return None;
    }
    match press.key {
        Key::Named(named) => named_action(*named, modifiers),
        Key::Character(c) if modifiers.control() && !modifiers.alt() => control_action(c),
        Key::Character(c) if modifiers.alt() && !modifiers.control() => {
            (c.eq_ignore_ascii_case("v")).then_some(ViAction::Select(ViSelection::Semantic))
        }
        Key::Character(_) | Key::Unidentified if !modifiers.control() && !modifiers.alt() => {
            let c = typed_char(press)?;
            if c == 'g' {
                if previous == Some('g') {
                    return Some(ViAction::Top);
                }
                *pending = Some('g');
                return None;
            }
            character_action(c)
        }
        _ => None,
    }
}

/// The character a key press types: its text, or else the key with Shift applied.
fn typed_char(press: KeyPress<'_>) -> Option<char> {
    if let Some(text) = press.text.filter(|text| !text.is_empty()) {
        return single_char(text);
    }
    let Key::Character(key) = press.key else {
        return None;
    };
    let c = single_char(key)?;
    Some(if press.modifiers.shift() {
        c.to_ascii_uppercase()
    } else {
        c
    })
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

fn character_action(c: char) -> Option<ViAction> {
    use ViMotion as M;
    let motion = match c {
        'h' => M::Left,
        'j' => M::Down,
        'k' => M::Up,
        'l' => M::Right,
        'w' => M::SemanticRight,
        'b' => M::SemanticLeft,
        'e' => M::SemanticRightEnd,
        'W' => M::WordRight,
        'B' => M::WordLeft,
        'E' => M::WordRightEnd,
        '0' => M::First,
        '^' => M::FirstOccupied,
        '$' => M::Last,
        'H' => M::High,
        'M' => M::Middle,
        'L' => M::Low,
        '{' => M::ParagraphUp,
        '}' => M::ParagraphDown,
        '%' => M::Bracket,
        'G' => return Some(ViAction::Bottom),
        'v' => return Some(ViAction::Select(ViSelection::Simple)),
        'V' => return Some(ViAction::Select(ViSelection::Line)),
        'y' => return Some(ViAction::Yank),
        'i' | 'q' => return Some(ViAction::Exit),
        '/' => return Some(ViAction::Search(SearchDirection::Newer)),
        '?' => return Some(ViAction::Search(SearchDirection::Older)),
        'n' => return Some(ViAction::SearchNext { reverse: false }),
        'N' => return Some(ViAction::SearchNext { reverse: true }),
        _ => return None,
    };
    Some(ViAction::Motion(motion))
}

fn control_action(c: &str) -> Option<ViAction> {
    match c.to_ascii_lowercase().as_str() {
        "u" => Some(ViAction::Scroll(ViScroll::HalfPageUp)),
        "d" => Some(ViAction::Scroll(ViScroll::HalfPageDown)),
        "b" => Some(ViAction::Scroll(ViScroll::PageUp)),
        "f" => Some(ViAction::Scroll(ViScroll::PageDown)),
        "v" => Some(ViAction::Select(ViSelection::Block)),
        "c" => Some(ViAction::Exit),
        _ => None,
    }
}

fn named_action(named: Named, modifiers: Modifiers) -> Option<ViAction> {
    if modifiers.control() || modifiers.alt() {
        return None;
    }
    let motion = match named {
        Named::ArrowLeft => ViMotion::Left,
        Named::ArrowDown => ViMotion::Down,
        Named::ArrowUp => ViMotion::Up,
        Named::ArrowRight => ViMotion::Right,
        Named::Home => ViMotion::First,
        Named::End => ViMotion::Last,
        Named::PageUp => return Some(ViAction::Scroll(ViScroll::PageUp)),
        Named::PageDown => return Some(ViAction::Scroll(ViScroll::PageDown)),
        Named::Escape => return Some(ViAction::Escape),
        _ => return None,
    };
    Some(ViAction::Motion(motion))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str, text: &str, modifiers: Modifiers) -> Option<ViAction> {
        resolve(
            KeyPress {
                key: &Key::Character(key.into()),
                modifiers,
                text: Some(text),
            },
            &mut None,
        )
    }

    fn plain(c: &str) -> Option<ViAction> {
        press(c, c, Modifiers::empty())
    }

    fn named(named: Named) -> Option<ViAction> {
        resolve(
            KeyPress {
                key: &Key::Named(named),
                modifiers: Modifiers::empty(),
                text: None,
            },
            &mut None,
        )
    }

    #[test]
    fn letters_move_the_cursor() {
        assert_eq!(plain("h"), Some(ViAction::Motion(ViMotion::Left)));
        assert_eq!(plain("j"), Some(ViAction::Motion(ViMotion::Down)));
        assert_eq!(plain("k"), Some(ViAction::Motion(ViMotion::Up)));
        assert_eq!(plain("l"), Some(ViAction::Motion(ViMotion::Right)));
        assert_eq!(plain("w"), Some(ViAction::Motion(ViMotion::SemanticRight)));
    }

    #[test]
    fn shifted_keys_use_the_typed_text() {
        assert_eq!(
            press("v", "V", Modifiers::SHIFT),
            Some(ViAction::Select(ViSelection::Line))
        );
        assert_eq!(
            press("4", "$", Modifiers::SHIFT),
            Some(ViAction::Motion(ViMotion::Last))
        );
        assert_eq!(press("g", "G", Modifiers::SHIFT), Some(ViAction::Bottom));
        assert_eq!(
            press("n", "N", Modifiers::SHIFT),
            Some(ViAction::SearchNext { reverse: true })
        );
    }

    #[test]
    fn without_text_shift_uppercases_the_key() {
        let action = resolve(
            KeyPress {
                key: &Key::Character("w".into()),
                modifiers: Modifiers::SHIFT,
                text: None,
            },
            &mut None,
        );
        assert_eq!(action, Some(ViAction::Motion(ViMotion::WordRight)));
    }

    #[test]
    fn selection_kinds() {
        assert_eq!(plain("v"), Some(ViAction::Select(ViSelection::Simple)));
        assert_eq!(
            press("v", "\u{16}", Modifiers::CTRL),
            Some(ViAction::Select(ViSelection::Block))
        );
        assert_eq!(
            press("v", "v", Modifiers::ALT),
            Some(ViAction::Select(ViSelection::Semantic))
        );
    }

    #[test]
    fn gg_goes_to_the_top() {
        let g = Key::Character("g".into());
        let key = KeyPress {
            key: &g,
            modifiers: Modifiers::empty(),
            text: Some("g"),
        };
        let mut pending = None;
        assert_eq!(resolve(key, &mut pending), None);
        assert_eq!(pending, Some('g'));
        assert_eq!(resolve(key, &mut pending), Some(ViAction::Top));
        assert_eq!(pending, None);
    }

    #[test]
    fn another_key_cancels_a_pending_g() {
        let mut pending = Some('g');
        let j = Key::Character("j".into());
        let key = KeyPress {
            key: &j,
            modifiers: Modifiers::empty(),
            text: Some("j"),
        };
        assert_eq!(
            resolve(key, &mut pending),
            Some(ViAction::Motion(ViMotion::Down))
        );
        assert_eq!(pending, None);
    }

    #[test]
    fn control_keys_scroll_and_leave() {
        assert_eq!(
            press("u", "\u{15}", Modifiers::CTRL),
            Some(ViAction::Scroll(ViScroll::HalfPageUp))
        );
        assert_eq!(
            press("d", "\u{4}", Modifiers::CTRL),
            Some(ViAction::Scroll(ViScroll::HalfPageDown))
        );
        assert_eq!(press("c", "\u{3}", Modifiers::CTRL), Some(ViAction::Exit));
    }

    #[test]
    fn named_keys() {
        assert_eq!(named(Named::Escape), Some(ViAction::Escape));
        assert_eq!(named(Named::ArrowUp), Some(ViAction::Motion(ViMotion::Up)));
        assert_eq!(
            named(Named::PageUp),
            Some(ViAction::Scroll(ViScroll::PageUp))
        );
        assert_eq!(named(Named::Enter), None);
    }

    #[test]
    fn search_keys() {
        assert_eq!(plain("/"), Some(ViAction::Search(SearchDirection::Newer)));
        assert_eq!(
            press("/", "?", Modifiers::SHIFT),
            Some(ViAction::Search(SearchDirection::Older))
        );
        assert_eq!(plain("n"), Some(ViAction::SearchNext { reverse: false }));
    }

    #[test]
    fn leaving_and_yanking() {
        assert_eq!(plain("y"), Some(ViAction::Yank));
        assert_eq!(plain("i"), Some(ViAction::Exit));
        assert_eq!(plain("q"), Some(ViAction::Exit));
    }

    #[test]
    fn unknown_keys_do_nothing() {
        assert_eq!(plain("x"), None);
        assert_eq!(plain("a"), None);
        assert_eq!(press("x", "x", Modifiers::CTRL), None);
        assert_eq!(press("v", "v", Modifiers::LOGO), None);
    }
}

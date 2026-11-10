//! Translates key presses into the bytes a terminal application expects.

mod bindings;
mod kitty;
mod paste;
mod shortcut;

pub use bindings::{Binding, BindingAction, Chord, Resolution, resolve};
pub use paste::paste_bytes;
pub use shortcut::{Shortcut, shortcut};

use alacritty_terminal::term::TermMode;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

/// User-configurable input behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputConfig {
    /// Bytes sent for Shift+Enter outside the kitty protocol. `None` sends a plain carriage return.
    pub shift_enter: Option<Vec<u8>>,
    /// Enables the input method (IME) for composed and CJK input.
    pub ime: bool,
    /// User keybindings, consulted before the default shortcuts.
    pub bindings: Vec<Binding>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            shift_enter: Some(b"\x1b\r".to_vec()),
            ime: true,
            bindings: Vec::new(),
        }
    }
}

/// A single key press as delivered by the windowing layer.
#[derive(Debug, Clone, Copy)]
pub struct KeyPress<'a> {
    pub key: &'a Key,
    pub modifiers: Modifiers,
    pub text: Option<&'a str>,
}

const ESC: u8 = 0x1b;

/// Encodes a key press, or returns `None` when the terminal should not receive it.
pub fn encode(press: KeyPress<'_>, mode: TermMode, config: &InputConfig) -> Option<Vec<u8>> {
    if press.modifiers.logo() {
        return None;
    }
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
        match kitty::encode(press, mode) {
            kitty::Encoding::Bytes(bytes) => return Some(bytes),
            kitty::Encoding::Ignore => return None,
            kitty::Encoding::Legacy => {}
        }
    }
    match press.key {
        Key::Named(named) => encode_named(*named, press.modifiers, mode, config),
        Key::Character(c) => encode_character(c, press.modifiers, press.text),
        Key::Unidentified => press
            .text
            .filter(|t| !t.is_empty())
            .map(|t| t.as_bytes().to_vec()),
    }
}

fn encode_named(
    named: Named,
    mods: Modifiers,
    mode: TermMode,
    config: &InputConfig,
) -> Option<Vec<u8>> {
    let bytes = match named {
        Named::Enter if mods.shift() => {
            config.shift_enter.clone().unwrap_or_else(|| b"\r".to_vec())
        }
        Named::Enter => with_alt(b"\r", mods),
        Named::Backspace if mods.control() => with_alt(&[0x08], mods),
        Named::Backspace => with_alt(&[0x7f], mods),
        Named::Tab if mods.shift() => b"\x1b[Z".to_vec(),
        Named::Tab => with_alt(b"\t", mods),
        Named::Escape => vec![ESC],
        Named::Space if mods.control() => vec![0x00],
        Named::Space => with_alt(b" ", mods),
        Named::ArrowUp => cursor_key(b'A', mods, mode),
        Named::ArrowDown => cursor_key(b'B', mods, mode),
        Named::ArrowRight => cursor_key(b'C', mods, mode),
        Named::ArrowLeft => cursor_key(b'D', mods, mode),
        Named::Home => cursor_key(b'H', mods, mode),
        Named::End => cursor_key(b'F', mods, mode),
        Named::F1 => ss3_key(b'P', mods),
        Named::F2 => ss3_key(b'Q', mods),
        Named::F3 => ss3_key(b'R', mods),
        Named::F4 => ss3_key(b'S', mods),
        other => tilde_key(tilde_code(other)?, mods),
    };
    Some(bytes)
}

fn tilde_code(named: Named) -> Option<u8> {
    let code = match named {
        Named::Insert => 2,
        Named::Delete => 3,
        Named::PageUp => 5,
        Named::PageDown => 6,
        Named::F5 => 15,
        Named::F6 => 17,
        Named::F7 => 18,
        Named::F8 => 19,
        Named::F9 => 20,
        Named::F10 => 21,
        Named::F11 => 23,
        Named::F12 => 24,
        _ => return None,
    };
    Some(code)
}

fn encode_character(c: &str, mods: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
    if mods.control() {
        if mods.shift() {
            return None;
        }
        if let Some(code) = control_code(c) {
            return Some(with_alt(&[code], mods));
        }
    }
    let text = text.filter(|t| !t.is_empty()).unwrap_or(c);
    Some(with_alt(text.as_bytes(), mods))
}

fn control_code(c: &str) -> Option<u8> {
    let mut chars = c.chars();
    let ch = chars.next()?.to_ascii_lowercase();
    if chars.next().is_some() {
        return None;
    }
    let code = match ch {
        'a'..='z' => ch as u8 & 0x1f,
        '@' | '2' => 0x00,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '/' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    };
    Some(code)
}

/// The xterm modifier parameter (`1 + shift + 2·alt + 4·ctrl`), or `None` without modifiers.
fn modifier_param(mods: Modifiers) -> Option<u8> {
    let param =
        1 + u8::from(mods.shift()) + 2 * u8::from(mods.alt()) + 4 * u8::from(mods.control());
    (param > 1).then_some(param)
}

fn with_alt(bytes: &[u8], mods: Modifiers) -> Vec<u8> {
    if mods.alt() {
        [&[ESC], bytes].concat()
    } else {
        bytes.to_vec()
    }
}

fn cursor_key(letter: u8, mods: Modifiers, mode: TermMode) -> Vec<u8> {
    match modifier_param(mods) {
        Some(param) => format!("\x1b[1;{param}{}", char::from(letter)).into_bytes(),
        None if mode.contains(TermMode::APP_CURSOR) => vec![ESC, b'O', letter],
        None => vec![ESC, b'[', letter],
    }
}

fn ss3_key(letter: u8, mods: Modifiers) -> Vec<u8> {
    match modifier_param(mods) {
        Some(param) => format!("\x1b[1;{param}{}", char::from(letter)).into_bytes(),
        None => vec![ESC, b'O', letter],
    }
}

fn tilde_key(code: u8, mods: Modifiers) -> Vec<u8> {
    match modifier_param(mods) {
        Some(param) => format!("\x1b[{code};{param}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
        encode(
            KeyPress {
                key: &key,
                modifiers,
                text,
            },
            TermMode::empty(),
            &InputConfig::default(),
        )
    }

    fn named(named: Named, modifiers: Modifiers) -> Option<Vec<u8>> {
        press(Key::Named(named), modifiers, None)
    }

    fn character(c: &str, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
        press(Key::Character(c.into()), modifiers, text)
    }

    #[test]
    fn printable_text_is_sent_verbatim() {
        assert_eq!(
            character("a", Modifiers::empty(), Some("a")),
            Some(b"a".to_vec())
        );
        assert_eq!(
            character("a", Modifiers::SHIFT, Some("A")),
            Some(b"A".to_vec())
        );
        assert_eq!(
            character("é", Modifiers::empty(), Some("é")),
            Some("é".as_bytes().to_vec())
        );
    }

    #[test]
    fn enter_sends_carriage_return() {
        assert_eq!(
            named(Named::Enter, Modifiers::empty()),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn shift_enter_uses_configured_sequence() {
        assert_eq!(
            named(Named::Enter, Modifiers::SHIFT),
            Some(b"\x1b\r".to_vec())
        );

        let plain = InputConfig {
            shift_enter: None,
            ..InputConfig::default()
        };
        let key = Key::Named(Named::Enter);
        let bytes = encode(
            KeyPress {
                key: &key,
                modifiers: Modifiers::SHIFT,
                text: None,
            },
            TermMode::empty(),
            &plain,
        );
        assert_eq!(bytes, Some(b"\r".to_vec()));
    }

    #[test]
    fn control_letters_map_to_c0_codes() {
        assert_eq!(character("c", Modifiers::CTRL, None), Some(vec![0x03]));
        assert_eq!(character("a", Modifiers::CTRL, None), Some(vec![0x01]));
        assert_eq!(character("[", Modifiers::CTRL, None), Some(vec![0x1b]));
        assert_eq!(named(Named::Space, Modifiers::CTRL), Some(vec![0x00]));
    }

    #[test]
    fn ctrl_shift_letters_are_reserved_for_shortcuts() {
        assert_eq!(
            character("c", Modifiers::CTRL | Modifiers::SHIFT, None),
            None
        );
        assert_eq!(
            character("v", Modifiers::CTRL | Modifiers::SHIFT, None),
            None
        );
    }

    #[test]
    fn alt_prefixes_escape() {
        assert_eq!(
            character("x", Modifiers::ALT, Some("x")),
            Some(b"\x1bx".to_vec())
        );
        assert_eq!(
            named(Named::Backspace, Modifiers::ALT),
            Some(b"\x1b\x7f".to_vec())
        );
    }

    #[test]
    fn editing_keys() {
        assert_eq!(
            named(Named::Backspace, Modifiers::empty()),
            Some(vec![0x7f])
        );
        assert_eq!(named(Named::Backspace, Modifiers::CTRL), Some(vec![0x08]));
        assert_eq!(named(Named::Tab, Modifiers::empty()), Some(b"\t".to_vec()));
        assert_eq!(
            named(Named::Tab, Modifiers::SHIFT),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            named(Named::Escape, Modifiers::empty()),
            Some(b"\x1b".to_vec())
        );
        assert_eq!(
            named(Named::Delete, Modifiers::empty()),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            named(Named::Insert, Modifiers::empty()),
            Some(b"\x1b[2~".to_vec())
        );
        assert_eq!(
            named(Named::PageUp, Modifiers::empty()),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            named(Named::PageDown, Modifiers::CTRL),
            Some(b"\x1b[6;5~".to_vec())
        );
    }

    #[test]
    fn cursor_keys_follow_application_cursor_mode() {
        assert_eq!(
            named(Named::ArrowUp, Modifiers::empty()),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            named(Named::ArrowLeft, Modifiers::CTRL),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            named(Named::Home, Modifiers::SHIFT),
            Some(b"\x1b[1;2H".to_vec())
        );

        let key = Key::Named(Named::ArrowUp);
        let app = encode(
            KeyPress {
                key: &key,
                modifiers: Modifiers::empty(),
                text: None,
            },
            TermMode::APP_CURSOR,
            &InputConfig::default(),
        );
        assert_eq!(app, Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn function_keys() {
        assert_eq!(
            named(Named::F1, Modifiers::empty()),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            named(Named::F4, Modifiers::SHIFT),
            Some(b"\x1b[1;2S".to_vec())
        );
        assert_eq!(
            named(Named::F5, Modifiers::empty()),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            named(Named::F12, Modifiers::CTRL),
            Some(b"\x1b[24;5~".to_vec())
        );
    }

    #[test]
    fn kitty_mode_takes_over_shift_enter() {
        let key = Key::Named(Named::Enter);
        let bytes = encode(
            KeyPress {
                key: &key,
                modifiers: Modifiers::SHIFT,
                text: None,
            },
            TermMode::DISAMBIGUATE_ESC_CODES,
            &InputConfig::default(),
        );
        assert_eq!(bytes, Some(b"\x1b[13;2u".to_vec()));
    }

    #[test]
    fn bare_modifiers_send_nothing() {
        assert_eq!(named(Named::Shift, Modifiers::SHIFT), None);
        assert_eq!(named(Named::Control, Modifiers::CTRL), None);
    }
}

//! The kitty keyboard protocol (`CSI … u`), which applications such as Claude Code enable so
//! that keys like Shift+Enter or Ctrl+I can be told apart from their legacy byte sequences.

use alacritty_terminal::term::TermMode;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

use super::KeyPress;

/// How a key press is sent while the kitty protocol is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Encoding {
    /// The protocol defines bytes for this key.
    Bytes(Vec<u8>),
    /// The key keeps its legacy encoding.
    Legacy,
    /// The key is not reported.
    Ignore,
}

const NO_MODIFIERS: u32 = 1;
const SHIFT_ONLY: u32 = 2;

pub(super) fn encode(press: KeyPress<'_>, mode: TermMode) -> Encoding {
    let mods = modifiers(press.modifiers);
    match press.key {
        Key::Named(Named::Space) => encode_text_key(" ", Some(" "), mods, mode),
        Key::Named(named) => encode_named(*named, mods, mode),
        Key::Character(c) => encode_text_key(c, press.text, mods, mode),
        Key::Unidentified => Encoding::Legacy,
    }
}

/// The protocol's modifier parameter: 1 + shift + 2·alt + 4·ctrl + 8·super.
fn modifiers(mods: Modifiers) -> u32 {
    1 + u32::from(mods.shift())
        + 2 * u32::from(mods.alt())
        + 4 * u32::from(mods.control())
        + 8 * u32::from(mods.logo())
}

fn encode_named(named: Named, mods: u32, mode: TermMode) -> Encoding {
    let code = match named {
        Named::Escape => return csi_u(27, mods, None),
        Named::Enter => 13,
        Named::Tab => 9,
        Named::Backspace => 127,
        Named::F3 if mods > NO_MODIFIERS => {
            return Encoding::Bytes(format!("\x1b[13;{mods}~").into_bytes());
        }
        Named::Shift | Named::Control | Named::Alt | Named::Super | Named::Meta | Named::Hyper => {
            return Encoding::Ignore;
        }
        _ => return Encoding::Legacy,
    };
    if mods == NO_MODIFIERS && !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
        Encoding::Legacy
    } else {
        csi_u(code, mods, None)
    }
}

fn encode_text_key(key: &str, text: Option<&str>, mods: u32, mode: TermMode) -> Encoding {
    let mut chars = key.chars().flat_map(char::to_lowercase);
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return Encoding::Legacy;
    };
    let report_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    if !report_all && mods <= SHIFT_ONLY {
        return Encoding::Legacy;
    }
    let text = text
        .filter(|_| report_all && mode.contains(TermMode::REPORT_ASSOCIATED_TEXT))
        .filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
    csi_u(u32::from(c), mods, text)
}

fn csi_u(code: u32, mods: u32, text: Option<&str>) -> Encoding {
    let sequence = match text {
        Some(text) => {
            let codepoints: Vec<String> = text.chars().map(|c| u32::from(c).to_string()).collect();
            format!("\x1b[{code};{mods};{}u", codepoints.join(":"))
        }
        None if mods == NO_MODIFIERS => format!("\x1b[{code}u"),
        None => format!("\x1b[{code};{mods}u"),
    };
    Encoding::Bytes(sequence.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISAMBIGUATE: TermMode = TermMode::DISAMBIGUATE_ESC_CODES;

    fn named(named: Named, modifiers: Modifiers, mode: TermMode) -> Encoding {
        let key = Key::Named(named);
        encode(
            KeyPress {
                key: &key,
                modifiers,
                text: None,
            },
            mode,
        )
    }

    fn character(c: &str, modifiers: Modifiers, text: Option<&str>, mode: TermMode) -> Encoding {
        let key = Key::Character(c.into());
        encode(
            KeyPress {
                key: &key,
                modifiers,
                text,
            },
            mode,
        )
    }

    fn bytes(s: &str) -> Encoding {
        Encoding::Bytes(s.as_bytes().to_vec())
    }

    #[test]
    fn modified_enter_tab_and_backspace_are_disambiguated() {
        assert_eq!(
            named(Named::Enter, Modifiers::SHIFT, DISAMBIGUATE),
            bytes("\x1b[13;2u")
        );
        assert_eq!(
            named(Named::Tab, Modifiers::CTRL, DISAMBIGUATE),
            bytes("\x1b[9;5u")
        );
        assert_eq!(
            named(Named::Backspace, Modifiers::CTRL, DISAMBIGUATE),
            bytes("\x1b[127;5u")
        );
    }

    #[test]
    fn unmodified_enter_tab_and_backspace_stay_legacy() {
        assert_eq!(
            named(Named::Enter, Modifiers::empty(), DISAMBIGUATE),
            Encoding::Legacy
        );
        assert_eq!(
            named(Named::Tab, Modifiers::empty(), DISAMBIGUATE),
            Encoding::Legacy
        );
        assert_eq!(
            named(Named::Backspace, Modifiers::empty(), DISAMBIGUATE),
            Encoding::Legacy
        );
    }

    #[test]
    fn escape_is_always_disambiguated() {
        assert_eq!(
            named(Named::Escape, Modifiers::empty(), DISAMBIGUATE),
            bytes("\x1b[27u")
        );
        assert_eq!(
            named(Named::Escape, Modifiers::ALT, DISAMBIGUATE),
            bytes("\x1b[27;3u")
        );
    }

    #[test]
    fn ctrl_and_alt_text_keys_use_the_unshifted_codepoint() {
        assert_eq!(
            character("c", Modifiers::CTRL, None, DISAMBIGUATE),
            bytes("\x1b[99;5u")
        );
        assert_eq!(
            character("x", Modifiers::ALT, Some("x"), DISAMBIGUATE),
            bytes("\x1b[120;3u")
        );
        assert_eq!(
            character("i", Modifiers::CTRL | Modifiers::SHIFT, None, DISAMBIGUATE),
            bytes("\x1b[105;6u")
        );
    }

    #[test]
    fn plain_and_shifted_text_stays_legacy() {
        assert_eq!(
            character("a", Modifiers::empty(), Some("a"), DISAMBIGUATE),
            Encoding::Legacy
        );
        assert_eq!(
            character("a", Modifiers::SHIFT, Some("A"), DISAMBIGUATE),
            Encoding::Legacy
        );
    }

    #[test]
    fn navigation_keys_keep_legacy_forms_except_f3() {
        assert_eq!(
            named(Named::ArrowUp, Modifiers::CTRL, DISAMBIGUATE),
            Encoding::Legacy
        );
        assert_eq!(
            named(Named::F1, Modifiers::empty(), DISAMBIGUATE),
            Encoding::Legacy
        );
        assert_eq!(
            named(Named::F3, Modifiers::CTRL, DISAMBIGUATE),
            bytes("\x1b[13;5~")
        );
    }

    #[test]
    fn report_all_keys_encodes_text_and_enter() {
        let all = DISAMBIGUATE | TermMode::REPORT_ALL_KEYS_AS_ESC;
        assert_eq!(
            character("a", Modifiers::empty(), Some("a"), all),
            bytes("\x1b[97u")
        );
        assert_eq!(
            character("a", Modifiers::SHIFT, Some("A"), all),
            bytes("\x1b[97;2u")
        );
        assert_eq!(
            named(Named::Enter, Modifiers::empty(), all),
            bytes("\x1b[13u")
        );
        assert_eq!(
            named(Named::Space, Modifiers::empty(), all),
            bytes("\x1b[32u")
        );
    }

    #[test]
    fn associated_text_is_appended_as_codepoints() {
        let mode = TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_ASSOCIATED_TEXT;
        assert_eq!(
            character("a", Modifiers::SHIFT, Some("A"), mode),
            bytes("\x1b[97;2;65u")
        );
        assert_eq!(
            character("a", Modifiers::empty(), Some("a"), mode),
            bytes("\x1b[97;1;97u")
        );
    }

    #[test]
    fn bare_modifier_keys_are_not_reported() {
        let all = TermMode::REPORT_ALL_KEYS_AS_ESC;
        assert_eq!(named(Named::Shift, Modifiers::SHIFT, all), Encoding::Ignore);
    }
}

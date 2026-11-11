//! `window.hotkey` in GNOME accelerator syntax ("F12", "<Super>grave", "<Control><Alt>t"), checked
//! against the key names of xkbcommon, which COSMIC and the portals use too.

use std::fmt;

use xkbcommon::xkb::{self, Keysym};

const NO_SYMBOL: u32 = 0;

/// A key with modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accelerator {
    pub modifiers: Modifiers,
    pub key: Keysym,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per modifier key, as xkb has them"
)]
pub struct Modifiers {
    pub logo: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub fn any(self) -> bool {
        self.logo || self.ctrl || self.alt || self.shift
    }
}

impl Accelerator {
    /// Parses GNOME accelerator syntax.
    ///
    /// # Errors
    ///
    /// Fails for unknown modifiers, unknown key names, and printable keys without a modifier,
    /// which would stop that character from being typed.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut modifiers = Modifiers::default();
        let mut rest = text.trim();
        while let Some(tag) = rest.strip_prefix('<') {
            let (name, after) = tag
                .split_once('>')
                .ok_or_else(|| format!("\"{text}\" has an unclosed <"))?;
            match name.to_ascii_lowercase().as_str() {
                "super" => modifiers.logo = true,
                "control" | "ctrl" | "primary" => modifiers.ctrl = true,
                "alt" | "mod1" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                _ => return Err(format!("the modifier <{name}> is not supported")),
            }
            rest = after.trim_start();
        }
        let key = keysym(rest).ok_or_else(|| format!("\"{rest}\" is not a key name"))?;
        if !modifiers.any() && printable(key) {
            return Err(format!(
                "\"{rest}\" types a character; add a modifier such as <Super>"
            ));
        }
        Ok(Self { modifiers, key })
    }

    /// The key's xkb name, such as "grave" or "F12".
    pub fn key_name(self) -> String {
        xkb::keysym_get_name(self.key)
    }
}

/// The keysym for an xkb key name, as COSMIC looks it up: exact first, then ignoring case.
/// Single letters are taken in lower case, as COSMIC and GNOME bind them.
pub fn keysym(name: &str) -> Option<Keysym> {
    let name = name.trim();
    if name.is_empty() || name.contains(['<', '>', '+', ' ']) {
        return None;
    }
    let lowered;
    let name = if name.chars().count() == 1 {
        lowered = name.to_lowercase();
        lowered.as_str()
    } else {
        name
    };
    lookup(name)
}

/// The keysym for a key name exactly as COSMIC reads its shortcuts file: exact, then ignoring case.
pub fn lookup(name: &str) -> Option<Keysym> {
    [xkb::KEYSYM_NO_FLAGS, xkb::KEYSYM_CASE_INSENSITIVE]
        .into_iter()
        .map(|flags| xkb::keysym_from_name(name, flags))
        .find(|keysym| keysym.raw() != NO_SYMBOL)
}

fn printable(key: Keysym) -> bool {
    xkb::keysym_to_utf8(key)
        .trim_end_matches('\0')
        .chars()
        .any(|c| !c.is_control())
}

/// "Super+Ctrl+grave", the way COSMIC's settings show shortcuts.
impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Modifiers {
            logo,
            ctrl,
            alt,
            shift,
        } = self.modifiers;
        for (on, name) in [
            (logo, "Super"),
            (ctrl, "Ctrl"),
            (alt, "Alt"),
            (shift, "Shift"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Accelerator {
        Accelerator::parse(text).expect(text)
    }

    #[test]
    fn function_keys_need_no_modifier() {
        let f12 = parse("F12");
        assert_eq!(f12.modifiers, Modifiers::default());
        assert_eq!(f12.key_name(), "F12");
    }

    #[test]
    fn modifiers_are_read_in_any_case_and_spelling() {
        let accelerator = parse("<Super><control><Alt><Shift>grave");
        assert_eq!(
            accelerator.modifiers,
            Modifiers {
                logo: true,
                ctrl: true,
                alt: true,
                shift: true,
            }
        );
        assert_eq!(parse("<Primary>t").modifiers, parse("<Ctrl>t").modifiers);
        assert_eq!(accelerator.to_string(), "Super+Ctrl+Alt+Shift+grave");
    }

    #[test]
    fn letters_are_bound_in_lower_case_and_names_ignore_case() {
        assert_eq!(parse("<Control>T").key_name(), "t");
        assert_eq!(parse("<Super>RETURN").key_name(), "Return");
    }

    #[test]
    fn unknown_keys_and_modifiers_are_refused() {
        assert!(Accelerator::parse("<Super>nokey").is_err());
        assert!(Accelerator::parse("<Hyper>t").is_err());
        assert!(Accelerator::parse("<Super").is_err());
        assert!(Accelerator::parse("<Super>").is_err());
        assert!(Accelerator::parse("Super+t").is_err());
    }

    #[test]
    fn printable_keys_need_a_modifier() {
        assert!(Accelerator::parse("grave").is_err());
        assert!(Accelerator::parse("space").is_err());
        assert!(Accelerator::parse("<Super>space").is_ok());
        assert!(Accelerator::parse("Pause").is_ok());
    }
}

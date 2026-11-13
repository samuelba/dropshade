//! User keybindings: chords such as `ctrl+shift+t` mapped to shortcuts or text.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

use super::shortcut::{Shortcut, shortcut};

/// A key plus the exact modifiers that must be held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    key: ChordKey,
    modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChordKey {
    Character(String),
    Named(Named),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingAction {
    Shortcut(Shortcut),
    Send(Vec<u8>),
    /// Unbinds a default shortcut so the key reaches the application.
    PassThrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub chord: Chord,
    pub action: BindingAction,
}

/// What a key press resolves to after user bindings and default shortcuts are consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Shortcut(Shortcut),
    Send(Vec<u8>),
    /// Encode the key for the application as usual.
    Encode,
}

impl Chord {
    /// Parses `ctrl+shift+t`, `alt+enter`, `ctrl+plus`, `f5` and similar (case-insensitive).
    ///
    /// # Errors
    ///
    /// Fails with a message naming the empty chord, unknown modifier or unknown key.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim().to_ascii_lowercase();
        if text.is_empty() {
            return Err("key chord is empty".into());
        }
        let mut parts: Vec<&str> = text.split('+').collect();
        let key = parts
            .pop()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("\"{text}\" has no key after the modifiers"))?;
        let mut modifiers = Modifiers::empty();
        for part in parts {
            modifiers |= match part {
                "ctrl" | "control" => Modifiers::CTRL,
                "shift" => Modifiers::SHIFT,
                "alt" => Modifiers::ALT,
                "super" | "logo" | "meta" => Modifiers::LOGO,
                other => {
                    return Err(format!(
                        "\"{other}\" in \"{text}\" is not a modifier (ctrl, shift, alt, super)"
                    ));
                }
            };
        }
        Ok(Self {
            key: chord_key(key).ok_or_else(|| format!("\"{key}\" in \"{text}\" is not a key"))?,
            modifiers,
        })
    }

    /// The chord as shown in menus, such as `Ctrl+Shift+T`.
    pub fn label(&self) -> String {
        let mut label = String::new();
        for (held, name) in [
            (self.modifiers.control(), "Ctrl"),
            (self.modifiers.alt(), "Alt"),
            (self.modifiers.shift(), "Shift"),
            (self.modifiers.logo(), "Super"),
        ] {
            if held {
                label.push_str(name);
                label.push('+');
            }
        }
        match &self.key {
            ChordKey::Character(c) => label.push_str(&c.to_uppercase()),
            ChordKey::Named(named) => {
                let name = format!("{named:?}");
                label.push_str(name.strip_prefix("Arrow").unwrap_or(&name));
            }
        }
        label
    }

    fn key(&self) -> Key {
        match &self.key {
            ChordKey::Character(c) => Key::Character(c.as_str().into()),
            ChordKey::Named(named) => Key::Named(*named),
        }
    }

    pub fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        let exact = |m: Modifiers| (m.control(), m.shift(), m.alt(), m.logo());
        if exact(modifiers) != exact(self.modifiers) {
            return false;
        }
        match (&self.key, key) {
            (ChordKey::Character(wanted), Key::Character(pressed)) => {
                pressed.to_lowercase() == *wanted
            }
            (ChordKey::Named(wanted), Key::Named(pressed)) => wanted == pressed,
            _ => false,
        }
    }
}

fn chord_key(name: &str) -> Option<ChordKey> {
    if name.chars().count() == 1 {
        return Some(ChordKey::Character(name.to_owned()));
    }
    let named = match name {
        "plus" => return Some(ChordKey::Character("+".into())),
        "enter" | "return" => Named::Enter,
        "tab" => Named::Tab,
        "space" => Named::Space,
        "backspace" => Named::Backspace,
        "escape" | "esc" => Named::Escape,
        "insert" => Named::Insert,
        "delete" | "del" => Named::Delete,
        "home" => Named::Home,
        "end" => Named::End,
        "pageup" => Named::PageUp,
        "pagedown" => Named::PageDown,
        "up" => Named::ArrowUp,
        "down" => Named::ArrowDown,
        "left" => Named::ArrowLeft,
        "right" => Named::ArrowRight,
        function => return function_key(function).map(ChordKey::Named),
    };
    Some(ChordKey::Named(named))
}

fn function_key(name: &str) -> Option<Named> {
    const KEYS: [Named; 12] = [
        Named::F1,
        Named::F2,
        Named::F3,
        Named::F4,
        Named::F5,
        Named::F6,
        Named::F7,
        Named::F8,
        Named::F9,
        Named::F10,
        Named::F11,
        Named::F12,
    ];
    let number: usize = name.strip_prefix('f')?.parse().ok()?;
    KEYS.get(number.checked_sub(1)?).copied()
}

/// User bindings win over the default shortcuts, which win over sending the key.
pub fn resolve(key: &Key, modifiers: Modifiers, bindings: &[Binding]) -> Resolution {
    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.chord.matches(key, modifiers))
    {
        return match &binding.action {
            BindingAction::Shortcut(shortcut) => Resolution::Shortcut(*shortcut),
            BindingAction::Send(bytes) => Resolution::Send(bytes.clone()),
            BindingAction::PassThrough => Resolution::Encode,
        };
    }
    shortcut(key, modifiers).map_or(Resolution::Encode, Resolution::Shortcut)
}

/// Default chords for the shortcuts that appear in menus.
const MENU_DEFAULTS: [(Shortcut, &str); 14] = [
    (Shortcut::Copy, "ctrl+shift+c"),
    (Shortcut::Paste, "ctrl+shift+v"),
    (Shortcut::NewTab, "ctrl+shift+t"),
    (Shortcut::Search, "ctrl+shift+f"),
    (Shortcut::ZoomIn, "ctrl+="),
    (Shortcut::ZoomOut, "ctrl+-"),
    (Shortcut::ZoomReset, "ctrl+0"),
    (Shortcut::ToggleMaximize, "f11"),
    (Shortcut::OpenSettings, "ctrl+,"),
    (Shortcut::ToggleViMode, "ctrl+shift+space"),
    (Shortcut::SplitRight, "ctrl+shift+r"),
    (Shortcut::SplitDown, "ctrl+shift+d"),
    (Shortcut::ClosePane, "ctrl+shift+w"),
    (Shortcut::TogglePaneZoom, "ctrl+shift+x"),
];

/// The chord that triggers `target`, for display: the first user binding to it, otherwise its
/// default unless a user binding took that key away. `None` when nothing triggers it.
pub fn shortcut_label(target: Shortcut, bindings: &[Binding]) -> Option<String> {
    let custom = bindings.iter().find_map(|binding| match binding.action {
        BindingAction::Shortcut(shortcut) if shortcut == target => Some(binding.chord.label()),
        _ => None,
    });
    custom.or_else(|| {
        let (_, default) = MENU_DEFAULTS
            .iter()
            .find(|(shortcut, _)| *shortcut == target)?;
        let chord = Chord::parse(default).ok()?;
        let still_bound =
            resolve(&chord.key(), chord.modifiers, bindings) == Resolution::Shortcut(target);
        still_bound.then(|| chord.label())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).expect(text)
    }

    fn character(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn chords_parse_modifiers_and_keys() {
        let ctrl_shift_t = chord("ctrl+shift+t");
        assert!(ctrl_shift_t.matches(&character("t"), Modifiers::CTRL | Modifiers::SHIFT));
        assert!(ctrl_shift_t.matches(&character("T"), Modifiers::CTRL | Modifiers::SHIFT));
        assert!(!ctrl_shift_t.matches(&character("t"), Modifiers::CTRL));
        assert!(!ctrl_shift_t.matches(
            &character("t"),
            Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT
        ));

        assert!(chord("Alt+Enter").matches(&Key::Named(Named::Enter), Modifiers::ALT));
        assert!(chord("control+PageUp").matches(&Key::Named(Named::PageUp), Modifiers::CTRL));
        assert!(chord("f5").matches(&Key::Named(Named::F5), Modifiers::empty()));
        assert!(chord("super+space").matches(&Key::Named(Named::Space), Modifiers::LOGO));
        assert!(chord("ctrl+plus").matches(&character("+"), Modifiers::CTRL));
    }

    #[test]
    fn invalid_chords_explain_the_problem() {
        assert!(Chord::parse("").unwrap_err().contains("empty"));
        assert!(Chord::parse("ctrl+").unwrap_err().contains("key"));
        assert!(Chord::parse("hyper+t").unwrap_err().contains("hyper"));
        assert!(Chord::parse("ctrl+banana").unwrap_err().contains("banana"));
        assert!(Chord::parse("ctrl+t+u").is_err());
    }

    #[test]
    fn user_bindings_override_defaults() {
        let bindings = [
            Binding {
                chord: chord("ctrl+alt+l"),
                action: BindingAction::Send(b"clear\r".to_vec()),
            },
            Binding {
                chord: chord("ctrl+shift+n"),
                action: BindingAction::Shortcut(Shortcut::NewTab),
            },
            Binding {
                chord: chord("ctrl+tab"),
                action: BindingAction::PassThrough,
            },
        ];
        let resolve_with = |key: Key, mods| resolve(&key, mods, &bindings);
        assert_eq!(
            resolve_with(character("l"), Modifiers::CTRL | Modifiers::ALT),
            Resolution::Send(b"clear\r".to_vec())
        );
        assert_eq!(
            resolve_with(character("n"), Modifiers::CTRL | Modifiers::SHIFT),
            Resolution::Shortcut(Shortcut::NewTab)
        );
        assert_eq!(
            resolve_with(Key::Named(Named::Tab), Modifiers::CTRL),
            Resolution::Encode
        );
        assert_eq!(
            resolve_with(character("t"), Modifiers::CTRL | Modifiers::SHIFT),
            Resolution::Shortcut(Shortcut::NewTab)
        );
        assert_eq!(
            resolve_with(character("a"), Modifiers::empty()),
            Resolution::Encode
        );
    }

    #[test]
    fn action_names_parse() {
        assert_eq!("new-tab".parse(), Ok(Shortcut::NewTab));
        assert_eq!("paste-primary".parse(), Ok(Shortcut::PastePrimary));
        assert_eq!("go-to-tab-3".parse(), Ok(Shortcut::GoToTab(2)));
        assert!("go-to-tab-0".parse::<Shortcut>().is_err());
        assert!(
            "explode"
                .parse::<Shortcut>()
                .unwrap_err()
                .contains("explode")
        );
    }

    #[test]
    fn menu_labels_follow_user_bindings() {
        assert_eq!(
            shortcut_label(Shortcut::NewTab, &[]).as_deref(),
            Some("Ctrl+Shift+T")
        );
        assert_eq!(
            shortcut_label(Shortcut::ZoomIn, &[]).as_deref(),
            Some("Ctrl+=")
        );
        assert_eq!(
            shortcut_label(Shortcut::OpenSettings, &[]).as_deref(),
            Some("Ctrl+,")
        );
        assert_eq!(shortcut_label(Shortcut::CloseTab, &[]), None);

        let remapped = [Binding {
            chord: chord("alt+n"),
            action: BindingAction::Shortcut(Shortcut::NewTab),
        }];
        assert_eq!(
            shortcut_label(Shortcut::NewTab, &remapped).as_deref(),
            Some("Alt+N")
        );

        let unbound = [Binding {
            chord: chord("ctrl+shift+t"),
            action: BindingAction::PassThrough,
        }];
        assert_eq!(shortcut_label(Shortcut::NewTab, &unbound), None);
        assert_eq!(chord("ctrl+pageup").label(), "Ctrl+PageUp");
        assert_eq!(chord("ctrl+down").label(), "Ctrl+Down");
    }
}

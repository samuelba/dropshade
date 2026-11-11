//! COSMIC keeps the user's shortcuts in a RON file, `custom`, which its compositor reloads when
//! it changes. Dropshade owns one entry there, the one described as "Dropshade".
//!
//! COSMIC ignores the whole file when one entry does not load, which would drop all of the
//! user's shortcuts, so this is careful: the file is read with a copy of COSMIC's own types, and
//! anything unexpected (unknown fields, unknown key names, several Dropshade entries, a Dropshade
//! entry that does not run a command) leaves it untouched. The new text is read back the same
//! way and must match entry for entry before it replaces the file. The previous file is kept as
//! a backup, and nothing is written when COSMIC changed the file in the meantime.

use std::fmt;
use std::path::{Path, PathBuf};

use ron::value::RawValue;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Unexpected, Visitor};

use super::accelerator::{self, Accelerator, Modifiers};

const SHORTCUTS: &str = "cosmic/com.system76.CosmicSettings.Shortcuts/v1";
const DESCRIPTION: &str = "Dropshade";
const BACKUP: &str = "cosmic-shortcuts-custom.backup";

/// What binding did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Unchanged,
    Added,
    Updated,
}

/// Dropshade's entry in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub keys: String,
    pub command: String,
}

/// Sets Dropshade's shortcut to `wanted`, adding an entry that runs `command` if there is none.
/// An existing entry keeps its command.
///
/// # Errors
///
/// Fails, leaving the file as it was, when it cannot be read or written, is not in the expected
/// format, or the keys are taken.
pub fn bind(wanted: Accelerator, command: &str) -> Result<Change, String> {
    let path = custom_path().ok_or("no home directory")?;
    let original = read(&path)?;
    let defaults = defaults();
    let (change, text) = plan(
        original.as_deref().unwrap_or_default(),
        defaults.as_deref(),
        wanted,
        command,
    )?;
    if change == Change::Unchanged {
        return Ok(change);
    }
    if let Some(original) = &original {
        backup(original)?;
    }
    replace(&path, &text, original.as_deref())?;
    tracing::info!(path = %path.display(), keys = %wanted, "set the shortcut in COSMIC");
    Ok(change)
}

/// Dropshade's current entry, if there is one.
///
/// # Errors
///
/// Fails when the file cannot be read or is not in the expected format.
pub fn current() -> Result<Option<Entry>, String> {
    let path = custom_path().ok_or("no home directory")?;
    let Some(text) = read(&path)? else {
        return Ok(None);
    };
    let entries = parse(&text)?;
    let Some(ours) = ours(&entries)? else {
        return Ok(None);
    };
    Ok(Some(Entry {
        keys: ours.binding.to_string(),
        command: spawned(&ours.action).unwrap_or_else(|| ours.action.clone()),
    }))
}

/// The new file text for binding `wanted`, from the current `custom` text and COSMIC's default
/// shortcuts (when they could be read).
fn plan(
    custom: &str,
    defaults: Option<&str>,
    wanted: Accelerator,
    command: &str,
) -> Result<(Change, String), String> {
    let entries = parse(custom)?;
    let existing = ours(&entries)?;
    if let Some(ours) = existing {
        if spawned(&ours.action).is_none() {
            return Err(format!(
                "the COSMIC shortcut named {DESCRIPTION} does not run a command ({})",
                ours.action
            ));
        }
        if ours.binding.is(wanted) {
            return Ok((Change::Unchanged, custom.to_owned()));
        }
    }
    if let Some(other) = entries
        .iter()
        .find(|entry| !entry.binding.is_ours() && entry.binding.is(wanted))
    {
        return Err(format!(
            "{wanted} is already a custom COSMIC shortcut for {}",
            other.purpose()
        ));
    }
    if let Some(taken) = defaults
        .and_then(|text| {
            parse(text)
                .inspect_err(|error| tracing::debug!(%error, "cannot read COSMIC's defaults"))
                .ok()
        })
        .and_then(|defaults| defaults.into_iter().find(|entry| entry.binding.is(wanted)))
    {
        return Err(format!(
            "COSMIC already uses {wanted} for {}",
            taken.purpose()
        ));
    }

    let binding = Binding::new(
        wanted,
        existing.and_then(|ours| ours.binding.description.clone()),
    );
    let (change, expected) = if existing.is_some() {
        let replaced = entries
            .iter()
            .map(|entry| {
                if entry.binding.is_ours() {
                    Shortcut {
                        binding: binding.clone(),
                        action: entry.action.clone(),
                    }
                } else {
                    entry.clone()
                }
            })
            .collect();
        (Change::Updated, replaced)
    } else {
        let action = format!("Spawn({})", quote(command)?);
        let mut added = entries.clone();
        added.push(Shortcut { binding, action });
        (Change::Added, added)
    };
    let text = render(&expected)?;
    verify(&text, &expected, wanted)?;
    Ok((change, text))
}

/// Reads `text` back as COSMIC would and checks that it holds exactly `expected`.
fn verify(text: &str, expected: &[Shortcut], wanted: Accelerator) -> Result<(), String> {
    let reread = parse(text).map_err(|error| format!("the new file would not load: {error}"))?;
    let bound = ours(&reread)?
        .is_some_and(|ours| ours.binding.is(wanted) && spawned(&ours.action).is_some());
    if reread != expected || !bound {
        return Err("the new file did not read back as intended".into());
    }
    Ok(())
}

/// The single entry described as Dropshade's.
fn ours(entries: &[Shortcut]) -> Result<Option<&Shortcut>, String> {
    let mut found = entries.iter().filter(|entry| entry.binding.is_ours());
    let first = found.next();
    if found.next().is_some() {
        return Err(format!(
            "several COSMIC shortcuts are named {DESCRIPTION}; keep one in COSMIC Settings"
        ));
    }
    Ok(first)
}

/// The command a `Spawn("...")` action runs.
fn spawned(action: &str) -> Option<String> {
    #[derive(Deserialize)]
    enum Action {
        Spawn(String),
    }
    let raw = RawValue::from_ron(action).ok()?;
    match raw.into_rust::<Action>().ok()? {
        Action::Spawn(command) => Some(command),
    }
}

fn custom_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.config_dir().join(SHORTCUTS).join("custom"))
}

/// COSMIC's default shortcuts, from the first data directory that has them.
fn defaults() -> Option<String> {
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    dirs.split(':')
        .map(|dir| Path::new(dir).join(SHORTCUTS).join("defaults"))
        .find_map(|path| std::fs::read_to_string(path).ok())
}

fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn backup(original: &str) -> Result<(), String> {
    let dirs = directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .ok_or("no home directory for the backup")?;
    let dir = dirs.state_dir().unwrap_or_else(|| dirs.data_local_dir());
    let path = dir.join(BACKUP);
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&path, original))
        .map_err(|error| {
            format!(
                "cannot back up COSMIC's shortcuts to {}: {error}",
                path.display()
            )
        })
}

/// Writes `text` next to the file and moves it into place, unless the file no longer holds
/// `original` (COSMIC Settings saved it meanwhile).
fn replace(path: &Path, text: &str, original: Option<&str>) -> Result<(), String> {
    let dir = path.parent().ok_or("the shortcuts file has no directory")?;
    let temporary = dir.join(".custom.dropshade-tmp");
    let fail = |error: std::io::Error| format!("cannot write {}: {error}", path.display());
    std::fs::create_dir_all(dir).map_err(fail)?;
    std::fs::write(&temporary, text).map_err(fail)?;
    if read(path)?.as_deref() != original {
        let _ = std::fs::remove_file(&temporary);
        return Err("COSMIC changed its shortcuts meanwhile; try again".into());
    }
    std::fs::rename(&temporary, path).map_err(fail)
}

/// One `binding: action` pair, with the action kept as written.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shortcut {
    binding: Binding,
    action: String,
}

impl Shortcut {
    /// What the entry is for, for messages: its description, or else its action.
    fn purpose(&self) -> &str {
        self.binding.description.as_deref().unwrap_or(&self.action)
    }
}

/// The entries in file order.
fn parse(text: &str) -> Result<Vec<Shortcut>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    ron::from_str::<Entries>(text)
        .map(|entries| entries.0)
        .map_err(|error| format!("COSMIC's shortcuts are not in the expected format ({error})"))
}

struct Entries(Vec<Shortcut>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntriesVisitor;

        impl<'de> Visitor<'de> for EntriesVisitor {
            type Value = Entries;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of shortcuts")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Entries, M::Error> {
                let mut entries = Vec::new();
                while let Some((binding, action)) = map.next_entry::<Binding, &RawValue>()? {
                    entries.push(Shortcut {
                        binding,
                        action: action.get_ron().trim().to_owned(),
                    });
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(EntriesVisitor)
    }
}

/// COSMIC's `Binding`, field for field, unknown fields refused just as COSMIC does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    modifiers: Vec<Modifier>,
    #[serde(default, deserialize_with = "key")]
    key: Option<Key>,
    #[serde(default)]
    keycode: Option<u32>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

/// A key name as written, with the keysym COSMIC reads it as.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    name: String,
    keysym: u32,
}

/// COSMIC refuses key names xkbcommon does not know, so they are refused here too.
fn key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Key>, D::Error> {
    let name = String::deserialize(deserializer)?;
    let keysym = accelerator::lookup(&name)
        .ok_or_else(|| de::Error::invalid_value(Unexpected::Str(&name), &"an xkb key name"))?;
    Ok(Some(Key {
        name,
        keysym: keysym.raw(),
    }))
}

impl Binding {
    fn new(accelerator: Accelerator, description: Option<String>) -> Self {
        let Modifiers {
            logo,
            ctrl,
            alt,
            shift,
        } = accelerator.modifiers;
        let modifiers = [
            (logo, Modifier::Super),
            (ctrl, Modifier::Ctrl),
            (alt, Modifier::Alt),
            (shift, Modifier::Shift),
        ]
        .into_iter()
        .filter_map(|(on, modifier)| on.then_some(modifier))
        .collect();
        Self {
            modifiers,
            key: Some(Key {
                name: accelerator.key_name(),
                keysym: accelerator.key.raw(),
            }),
            keycode: None,
            description: Some(description.unwrap_or_else(|| DESCRIPTION.to_owned())),
        }
    }

    fn is_ours(&self) -> bool {
        self.description
            .as_deref()
            .is_some_and(|description| description.trim().eq_ignore_ascii_case(DESCRIPTION))
    }

    fn modifier_set(&self) -> Modifiers {
        self.modifiers
            .iter()
            .fold(Modifiers::default(), |mut set, modifier| {
                match modifier {
                    Modifier::Super => set.logo = true,
                    Modifier::Ctrl => set.ctrl = true,
                    Modifier::Alt => set.alt = true,
                    Modifier::Shift => set.shift = true,
                }
                set
            })
    }

    /// Whether pressing `accelerator` triggers this binding; COSMIC compares modifiers and key.
    fn is(&self, accelerator: Accelerator) -> bool {
        self.modifier_set() == accelerator.modifiers
            && self.key.as_ref().map(|key| key.keysym) == Some(accelerator.key.raw())
    }

    fn render(&self) -> Result<String, String> {
        let modifiers: Vec<String> = self.modifiers.iter().map(|m| format!("{m:?}")).collect();
        let mut fields = vec![format!("modifiers: [{}]", modifiers.join(", "))];
        if let Some(key) = &self.key {
            fields.push(format!("key: {}", quote(&key.name)?));
        }
        if let Some(keycode) = self.keycode {
            fields.push(format!("keycode: Some({keycode})"));
        }
        if let Some(description) = &self.description {
            fields.push(format!("description: Some({})", quote(description)?));
        }
        let mut text = String::from("(\n");
        for field in fields {
            text.push_str("        ");
            text.push_str(&field);
            text.push_str(",\n");
        }
        text.push_str("    )");
        Ok(text)
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in &self.modifiers {
            write!(f, "{modifier:?}+")?;
        }
        match (&self.key, self.keycode) {
            (Some(key), _) => f.write_str(&key.name),
            (None, Some(keycode)) => write!(f, "keycode {keycode}"),
            (None, None) => Ok(()),
        }
    }
}

/// The file in COSMIC's layout.
fn render(entries: &[Shortcut]) -> Result<String, String> {
    let mut text = String::from("{\n");
    for entry in entries {
        text.push_str("    ");
        text.push_str(&entry.binding.render()?);
        text.push_str(": ");
        text.push_str(&entry.action);
        text.push_str(",\n");
    }
    text.push('}');
    Ok(text)
}

fn quote(text: &str) -> Result<String, String> {
    ron::to_string(text).map_err(|error| format!("cannot write \"{text}\": {error}"))
}

#[cfg(test)]
mod tests;

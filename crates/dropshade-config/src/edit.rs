//! Changes single settings in the configuration file while keeping the user's comments and
//! formatting, for the in-app settings panel.

use std::path::Path;

use crate::{ConfigError, parse};

/// A value the settings panel writes.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Float(f64),
    Integer(i64),
    Text(String),
    List(Vec<String>),
    /// An inline table of strings, such as `{ send = "clear\r" }`.
    Pairs(Vec<(String, String)>),
}

/// Sets `key` in `section` of the file at `path`, creating the file and the section when
/// missing. `section` may be dotted (`"terminal.env"`) to reach a nested table. The result is
/// validated first, so an invalid value never reaches the file.
///
/// # Errors
///
/// Fails when the file cannot be read, is not valid TOML, the new value is invalid, or the file
/// cannot be written.
pub fn set_value(
    path: &Path,
    section: &str,
    key: &str,
    value: SettingValue,
) -> Result<(), ConfigError> {
    edit(path, |document| {
        let table = section_mut(document, section)?;
        let item = toml_edit::value(to_toml(value));
        // Replacing only the value keeps comments attached to the key.
        match table.get_mut(key) {
            Some(existing) => *existing = item,
            None => {
                table.insert(key, item);
            }
        }
        Ok(())
    })
}

/// Removes `key` from `section`, so the built-in default applies again. Removing a key that is
/// not set does nothing.
///
/// # Errors
///
/// Fails like [`set_value`].
pub fn remove_value(path: &Path, section: &str, key: &str) -> Result<(), ConfigError> {
    edit(path, |document| {
        section_mut(document, section)?.remove(key);
        Ok(())
    })
}

fn edit(
    path: &Path,
    change: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<(), String>,
) -> Result<(), ConfigError> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(ConfigError::read(path, &error)),
    };
    let mut document: toml_edit::DocumentMut =
        original.parse().map_err(|error: toml_edit::TomlError| {
            ConfigError::at(path, &original, None, error.message().trim())
        })?;
    change(&mut document).map_err(|message| ConfigError::at(path, &original, None, message))?;
    let updated = document.to_string();
    parse(&updated, path)?;
    write_atomically(path, &updated).map_err(|error| ConfigError::read(path, &error))
}

fn section_mut<'a>(
    document: &'a mut toml_edit::DocumentMut,
    section: &str,
) -> Result<&'a mut dyn toml_edit::TableLike, String> {
    let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
    for name in section.split('.') {
        table = table
            .entry(name)
            .or_insert_with(toml_edit::table)
            .as_table_like_mut()
            .ok_or_else(|| format!("[{section}] is not a table"))?;
    }
    Ok(table)
}

fn to_toml(value: SettingValue) -> toml_edit::Value {
    match value {
        SettingValue::Bool(value) => value.into(),
        // Sliders produce values like 0.8999999; two decimals is plenty for every setting.
        SettingValue::Float(value) => ((value * 100.0).round() / 100.0).into(),
        SettingValue::Integer(value) => value.into(),
        SettingValue::Text(value) => value.into(),
        SettingValue::List(items) => items.iter().collect::<toml_edit::Array>().into(),
        SettingValue::Pairs(pairs) => pairs
            .into_iter()
            .map(|(key, value)| (key, toml_edit::Value::from(value)))
            .collect::<toml_edit::InlineTable>()
            .into(),
    }
}

fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, content: Option<&str>) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dropshade-edit-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.toml");
        match content {
            Some(content) => std::fs::write(&path, content).expect("write"),
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
        path
    }

    #[test]
    fn comments_and_other_values_survive() {
        let path = temp_file(
            "keep",
            Some("# my settings\n[font]\n# the size\nsize = 15.0\nfamily = \"monospace\"\n"),
        );
        set_value(&path, "font", "size", SettingValue::Float(18.0)).expect("set");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            text.contains("# my settings") && text.contains("# the size"),
            "{text}"
        );
        assert!(text.contains("size = 18.0"), "{text}");
        assert!(text.contains("family = \"monospace\""), "{text}");
    }

    #[test]
    fn missing_files_and_sections_are_created() {
        let path = temp_file("create", None);
        set_value(
            &path,
            "appearance",
            "theme",
            SettingValue::Text("dawn".into()),
        )
        .expect("set");
        set_value(&path, "session", "restore", SettingValue::Bool(false)).expect("set");
        let config = crate::load(&path).expect("valid");
        assert_eq!(config.appearance.theme.name(false), "dawn");
        assert!(!config.session.restore);
    }

    #[test]
    fn invalid_values_are_rejected_and_the_file_is_untouched() {
        let original = "[font]\nsize = 15.0\n";
        let path = temp_file("invalid", Some(original));
        let error =
            set_value(&path, "font", "size", SettingValue::Float(1.0)).expect_err("too small");
        assert!(error.to_string().contains("font.size"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
    }

    #[test]
    fn a_light_dark_theme_pair_is_replaced_by_a_single_theme() {
        let path = temp_file(
            "theme",
            Some("[appearance]\ntheme = { dark = \"dusk\", light = \"dawn\" }\n"),
        );
        set_value(
            &path,
            "appearance",
            "theme",
            SettingValue::Text("dawn".into()),
        )
        .expect("set");
        assert_eq!(
            crate::load(&path).expect("valid").appearance.theme,
            crate::ThemeChoice::Single("dawn".into())
        );
    }

    #[test]
    fn nested_tables_lists_and_inline_tables_are_written() {
        let path = temp_file("nested", Some("[terminal]\nshell = \"/bin/zsh\"\n"));
        set_value(
            &path,
            "terminal",
            "shell_args",
            SettingValue::List(vec!["-l".into(), "-i".into()]),
        )
        .expect("list");
        set_value(&path, "terminal", "scrollback", SettingValue::Integer(500)).expect("integer");
        set_value(
            &path,
            "terminal.env",
            "EDITOR",
            SettingValue::Text("nvim".into()),
        )
        .expect("nested");
        set_value(
            &path,
            "keybindings",
            "ctrl+alt+l",
            SettingValue::Pairs(vec![("send".into(), "clear\r".into())]),
        )
        .expect("inline table");
        let config = crate::load(&path).expect("valid");
        assert_eq!(config.terminal.shell_args, ["-l", "-i"]);
        assert_eq!(config.terminal.scrollback.lines(), 500);
        assert_eq!(config.terminal.env["EDITOR"], "nvim");
        assert_eq!(
            config.keybindings["ctrl+alt+l"],
            crate::Binding::Send {
                send: "clear\r".into()
            }
        );
    }

    #[test]
    fn removed_keys_fall_back_to_the_defaults() {
        let path = temp_file(
            "remove",
            Some("[terminal.env]\nEDITOR = \"nvim\"\n[font]\nsize = 20.0\n"),
        );
        remove_value(&path, "terminal.env", "EDITOR").expect("remove");
        remove_value(&path, "font", "size").expect("remove");
        remove_value(&path, "font", "never-set").expect("absent keys are fine");
        let config = crate::load(&path).expect("valid");
        assert!(config.terminal.env.is_empty());
        assert_eq!(config.font, crate::Font::default());
    }
}

//! Color themes: two built-ins plus any Alacritty theme file in the themes directory.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    #[allow(clippy::cast_possible_truncation)]
    pub const fn hex(value: u32) -> Self {
        Self {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        }
    }

    /// Parses `#rrggbb` or `0xrrggbb`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let digits = text.strip_prefix('#').or_else(|| text.strip_prefix("0x"))?;
        if digits.len() != 6 {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Self::hex)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub foreground: Rgb8,
    pub background: Rgb8,
    pub cursor: Rgb8,
    pub normal: [Rgb8; 8],
    pub bright: [Rgb8; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("unknown theme \"{0}\": not built in and no themes/{0}.toml file")]
    NotFound(String),
    #[error("theme name \"{0}\" must not contain path separators")]
    InvalidName(String),
    #[error("theme \"{name}\": {message}")]
    Invalid { name: String, message: String },
}

pub const BUILT_IN: [&str; 2] = ["dusk", "dawn"];

/// Dropshade's default dark theme: a deep blue-gray with a warm amber cursor.
pub const DUSK: ThemeColors = ThemeColors {
    foreground: Rgb8::hex(0xd6_db_e4),
    background: Rgb8::hex(0x16_18_1d),
    cursor: Rgb8::hex(0xf2_a6_5a),
    normal: [
        Rgb8::hex(0x23_26_2e),
        Rgb8::hex(0xef_6b_73),
        Rgb8::hex(0x8f_d1_8b),
        Rgb8::hex(0xf2_c4_6d),
        Rgb8::hex(0x6a_a8_f7),
        Rgb8::hex(0xc7_92_ea),
        Rgb8::hex(0x5f_d4_d0),
        Rgb8::hex(0xc7_cc_d6),
    ],
    bright: [
        Rgb8::hex(0x4a_50_5c),
        Rgb8::hex(0xff_8a_91),
        Rgb8::hex(0xa9_e3_a5),
        Rgb8::hex(0xff_d8_8a),
        Rgb8::hex(0x8c_bc_ff),
        Rgb8::hex(0xdb_b0_ff),
        Rgb8::hex(0x86_ec_e8),
        Rgb8::hex(0xf1_f3_f7),
    ],
};

/// The light counterpart: warm paper with ink-dark text and the same amber cursor.
pub const DAWN: ThemeColors = ThemeColors {
    foreground: Rgb8::hex(0x2b_2f_36),
    background: Rgb8::hex(0xf7_f5_f0),
    cursor: Rgb8::hex(0xd9_82_2b),
    normal: [
        Rgb8::hex(0x2b_2f_36),
        Rgb8::hex(0xc8_41_4b),
        Rgb8::hex(0x3f_8f_4a),
        Rgb8::hex(0xa7_7a_12),
        Rgb8::hex(0x2f_6f_c6),
        Rgb8::hex(0x94_46_b8),
        Rgb8::hex(0x1f_8a_8a),
        Rgb8::hex(0xd8_d4_cc),
    ],
    bright: [
        Rgb8::hex(0x6b_70_78),
        Rgb8::hex(0xe0_52_5c),
        Rgb8::hex(0x4f_a8_5b),
        Rgb8::hex(0xc2_8f_1c),
        Rgb8::hex(0x4a_86_dc),
        Rgb8::hex(0xad_5f_d0),
        Rgb8::hex(0x2c_a3_a3),
        Rgb8::hex(0xf2_ef_e8),
    ],
};

pub fn built_in(name: &str) -> Option<ThemeColors> {
    match name {
        "dusk" => Some(DUSK),
        "dawn" => Some(DAWN),
        _ => None,
    }
}

/// Finds a theme by name: built-ins first, then `<themes_dir>/<name>.toml` in Alacritty format.
///
/// # Errors
///
/// Fails when the name contains path separators, no such theme exists, or its file is malformed.
pub fn resolve(name: &str, themes_dir: Option<&Path>) -> Result<ThemeColors, ThemeError> {
    if let Some(theme) = built_in(name) {
        return Ok(theme);
    }
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(ThemeError::InvalidName(name.to_owned()));
    }
    let path = themes_dir
        .ok_or_else(|| ThemeError::NotFound(name.to_owned()))?
        .join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ThemeError::NotFound(name.to_owned()),
        _ => ThemeError::Invalid {
            name: name.to_owned(),
            message: error.to_string(),
        },
    })?;
    parse_alacritty(&text).map_err(|message| ThemeError::Invalid {
        name: name.to_owned(),
        message,
    })
}

const COLOR_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

type ColorTable = BTreeMap<String, toml::Value>;

#[derive(Deserialize)]
struct AlacrittyFile {
    colors: Option<AlacrittyColors>,
}

#[derive(Deserialize)]
struct AlacrittyColors {
    primary: Option<ColorTable>,
    cursor: Option<ColorTable>,
    normal: Option<ColorTable>,
    bright: Option<ColorTable>,
}

/// Reads the colors of an Alacritty theme file (`[colors.primary]`, `[colors.normal]`, …).
///
/// # Errors
///
/// Fails with a description of the first missing or malformed color.
pub fn parse_alacritty(text: &str) -> Result<ThemeColors, String> {
    let file: AlacrittyFile =
        toml::from_str(text).map_err(|error| error.message().trim().to_owned())?;
    let colors = file.colors.ok_or("the file has no [colors] section")?;
    let empty = ColorTable::new();
    let primary = colors.primary.as_ref().unwrap_or(&empty);
    let foreground = required(primary, "primary", "foreground")?;
    let background = required(primary, "primary", "background")?;
    let cursor = match colors.cursor.as_ref() {
        Some(table) if table.contains_key("cursor") => required(table, "cursor", "cursor")?,
        _ => foreground,
    };
    let normal = palette(colors.normal.as_ref().unwrap_or(&empty), "normal")?;
    let bright = match colors.bright.as_ref() {
        Some(table) => palette(table, "bright")?,
        None => normal,
    };
    Ok(ThemeColors {
        foreground,
        background,
        cursor,
        normal,
        bright,
    })
}

fn palette(table: &ColorTable, section: &str) -> Result<[Rgb8; 8], String> {
    let mut colors = [Rgb8::hex(0); 8];
    for (slot, name) in colors.iter_mut().zip(COLOR_NAMES) {
        *slot = required(table, section, name)?;
    }
    Ok(colors)
}

fn required(table: &ColorTable, section: &str, key: &str) -> Result<Rgb8, String> {
    let value = table
        .get(key)
        .ok_or_else(|| format!("colors.{section}.{key} is missing"))?;
    value
        .as_str()
        .and_then(Rgb8::parse)
        .ok_or_else(|| format!("colors.{section}.{key} is not a #rrggbb color"))
}

#[cfg(test)]
#[allow(clippy::unreadable_literal, reason = "colors match the theme files")]
mod tests {
    use super::*;

    const CATPPUCCIN_LIKE: &str = r##"
        [colors.primary]
        background = "#1e1e2e"
        foreground = "#cdd6f4"

        [colors.cursor]
        text = "#1e1e2e"
        cursor = "#f5e0dc"

        [colors.normal]
        black = "#45475a"
        red = "#f38ba8"
        green = "#a6e3a1"
        yellow = "#f9e2af"
        blue = "#89b4fa"
        magenta = "#f5c2e7"
        cyan = "#94e2d5"
        white = "#bac2de"

        [colors.bright]
        black = "0x585b70"
        red = "#f38ba8"
        green = "#a6e3a1"
        yellow = "#f9e2af"
        blue = "#89b4fa"
        magenta = "#f5c2e7"
        cyan = "#94e2d5"
        white = "#a6adc8"

        [colors.selection]
        text = "CellForeground"
    "##;

    #[test]
    fn hex_colors_parse() {
        assert_eq!(
            Rgb8::parse("#1e1e2e"),
            Some(Rgb8 {
                r: 0x1e,
                g: 0x1e,
                b: 0x2e
            })
        );
        assert_eq!(
            Rgb8::parse("0xFFa500"),
            Some(Rgb8 {
                r: 0xff,
                g: 0xa5,
                b: 0x00
            })
        );
        assert_eq!(Rgb8::parse("  #000000 "), Some(Rgb8 { r: 0, g: 0, b: 0 }));
        assert_eq!(Rgb8::parse("#12345"), None);
        assert_eq!(Rgb8::parse("red"), None);
    }

    #[test]
    fn alacritty_themes_are_imported() {
        let theme = parse_alacritty(CATPPUCCIN_LIKE).expect("valid theme");
        assert_eq!(theme.background, Rgb8::hex(0x1e1e2e));
        assert_eq!(theme.foreground, Rgb8::hex(0xcdd6f4));
        assert_eq!(theme.cursor, Rgb8::hex(0xf5e0dc));
        assert_eq!(theme.normal[1], Rgb8::hex(0xf38ba8));
        assert_eq!(theme.bright[0], Rgb8::hex(0x585b70));
    }

    #[test]
    fn missing_cursor_and_bright_colors_fall_back() {
        let text = r##"
            [colors.primary]
            background = "#000000"
            foreground = "#ffffff"
            [colors.normal]
            black = "#000000"
            red = "#ff0000"
            green = "#00ff00"
            yellow = "#ffff00"
            blue = "#0000ff"
            magenta = "#ff00ff"
            cyan = "#00ffff"
            white = "#eeeeee"
        "##;
        let theme = parse_alacritty(text).expect("valid theme");
        assert_eq!(theme.cursor, theme.foreground);
        assert_eq!(theme.bright, theme.normal);
    }

    #[test]
    fn incomplete_themes_name_the_missing_color() {
        let error =
            parse_alacritty("[colors.primary]\nbackground = \"#000000\"\n").expect_err("no fg");
        assert!(error.contains("foreground"), "{error}");
        let error = parse_alacritty(
            "[colors.primary]\nbackground = \"#000000\"\nforeground = \"#ffffff\"\n[colors.normal]\nblack = \"nope\"\n",
        )
        .expect_err("bad color");
        assert!(error.contains("black"), "{error}");
    }

    #[test]
    fn built_ins_resolve_without_a_directory() {
        assert_eq!(resolve("dusk", None), Ok(DUSK));
        assert_eq!(resolve("dawn", None), Ok(DAWN));
        assert!(BUILT_IN.iter().all(|name| built_in(name).is_some()));
    }

    #[test]
    fn theme_files_are_found_by_name() {
        let dir = std::env::temp_dir().join(format!("dropshade-theme-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("mocha.toml"), CATPPUCCIN_LIKE).expect("write theme");
        let theme = resolve("mocha", Some(&dir));
        std::fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(theme.map(|t| t.background), Ok(Rgb8::hex(0x1e1e2e)));
    }

    #[test]
    fn names_cannot_escape_the_themes_directory() {
        let dir = Path::new("/tmp");
        assert!(matches!(
            resolve("../etc/passwd", Some(dir)),
            Err(ThemeError::InvalidName(_))
        ));
        assert!(matches!(
            resolve("a/b", Some(dir)),
            Err(ThemeError::InvalidName(_))
        ));
        assert!(matches!(
            resolve("missing-theme", Some(dir)),
            Err(ThemeError::NotFound(_))
        ));
    }
}

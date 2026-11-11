//! The colors the Colors page edits, and where each one lives in the config file.

use dropshade_config::theme::Rgb8;
use dropshade_config::{AnsiColors, ColorOverrides};
use dropshade_term::Palette;
use dropshade_term::alacritty_terminal::vte::ansi::Rgb;
use dropshade_term::colors::CellColors;

/// One editable color. The index of `Normal` and `Bright` is the palette position (0..8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSlot {
    Foreground,
    Background,
    Cursor,
    CursorText,
    SelectionText,
    SelectionBackground,
    Normal(usize),
    Bright(usize),
}

impl ColorSlot {
    pub const PRIMARY: [Self; 6] = [
        Self::Foreground,
        Self::Background,
        Self::Cursor,
        Self::CursorText,
        Self::SelectionText,
        Self::SelectionBackground,
    ];

    /// Every slot: the primary colors, then the normal and bright palette.
    pub fn all() -> impl Iterator<Item = Self> {
        Self::PRIMARY
            .into_iter()
            .chain((0..8).map(Self::Normal))
            .chain((0..8).map(Self::Bright))
    }

    /// The table and key the color is written to.
    pub fn key(self) -> (&'static str, &'static str) {
        match self {
            Self::Foreground => ("colors.primary", "foreground"),
            Self::Background => ("colors.primary", "background"),
            Self::Cursor => ("colors.cursor", "cursor"),
            Self::CursorText => ("colors.cursor", "text"),
            Self::SelectionText => ("colors.selection", "text"),
            Self::SelectionBackground => ("colors.selection", "background"),
            Self::Normal(index) => ("colors.normal", AnsiColors::NAMES[index]),
            Self::Bright(index) => ("colors.bright", AnsiColors::NAMES[index]),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Foreground => "Text".into(),
            Self::Background => "Background".into(),
            Self::Cursor => "Accent and cursor".into(),
            Self::CursorText => "Cursor text".into(),
            Self::SelectionText => "Selection text".into(),
            Self::SelectionBackground => "Selection background".into(),
            Self::Normal(index) => capitalize(AnsiColors::NAMES[index]),
            Self::Bright(index) => format!("Bright {}", AnsiColors::NAMES[index]),
        }
    }

    /// The color set for this slot in the config file, if any.
    pub fn configured(self, colors: &ColorOverrides) -> Option<Rgb8> {
        match self {
            Self::Foreground => colors.primary.foreground,
            Self::Background => colors.primary.background,
            Self::Cursor => colors.cursor.cursor,
            Self::CursorText => colors.cursor.text,
            Self::SelectionText => colors.selection.text,
            Self::SelectionBackground => colors.selection.background,
            Self::Normal(index) => colors.normal.as_array()[index],
            Self::Bright(index) => colors.bright.as_array()[index],
        }
    }

    /// Whether neither the theme nor the config sets this color, so it is derived from others.
    pub fn automatic(self, palette: &Palette) -> bool {
        match self {
            Self::CursorText => palette.cursor_text.is_none(),
            Self::SelectionText => palette.selection_foreground.is_none(),
            Self::SelectionBackground => palette.selection_background.is_none(),
            _ => false,
        }
    }

    /// The color in use, for plain text where a color is derived.
    pub fn effective(self, palette: &Palette) -> Rgb8 {
        let plain = CellColors {
            fg: palette.foreground,
            bg: palette.background,
        };
        rgb8(match self {
            Self::Foreground => palette.foreground,
            Self::Background => palette.background,
            Self::Cursor => palette.cursor,
            Self::CursorText => palette.cursor_text(),
            Self::SelectionText => palette.selected(plain).fg,
            Self::SelectionBackground => palette.selected(plain).bg,
            Self::Normal(index) => palette.normal[index],
            Self::Bright(index) => palette.bright[index],
        })
    }
}

pub fn rgb8(color: Rgb) -> Rgb8 {
    Rgb8 {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropshade_config::edit::{SettingValue, set_value};

    #[test]
    fn every_slot_is_written_where_the_config_reads_it() {
        let dir = std::env::temp_dir().join(format!("dropshade-slots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.toml");
        let _ = std::fs::remove_file(&path);
        let slots: Vec<ColorSlot> = ColorSlot::all().collect();
        assert_eq!(slots.len(), 22);
        for (index, slot) in slots.iter().enumerate() {
            let (section, key) = slot.key();
            let color = Rgb8::hex(u32::try_from(index).expect("small") + 1);
            set_value(&path, section, key, SettingValue::Text(color.to_hex()))
                .unwrap_or_else(|error| panic!("{slot:?}: {error}"));
        }
        let config = dropshade_config::load(&path).expect("valid");
        std::fs::remove_dir_all(&dir).expect("clean up");
        for (index, slot) in slots.iter().enumerate() {
            let color = Rgb8::hex(u32::try_from(index).expect("small") + 1);
            assert_eq!(slot.configured(&config.colors), Some(color), "{slot:?}");
        }
    }

    #[test]
    fn derived_colors_are_shown_as_they_are_drawn() {
        let palette = Palette::DUSK;
        assert!(ColorSlot::SelectionBackground.automatic(&palette));
        assert!(!ColorSlot::Background.automatic(&palette));
        assert_eq!(
            ColorSlot::SelectionBackground.effective(&palette),
            rgb8(palette.foreground)
        );
        assert_eq!(
            ColorSlot::CursorText.effective(&palette),
            rgb8(palette.background)
        );
        assert_eq!(
            ColorSlot::Bright(1).effective(&palette),
            rgb8(palette.bright[1])
        );
    }

    #[test]
    fn labels_name_the_palette_colors() {
        assert_eq!(ColorSlot::Normal(1).label(), "Red");
        assert_eq!(ColorSlot::Bright(7).label(), "Bright white");
        assert_eq!(ColorSlot::SelectionText.label(), "Selection text");
        assert_eq!(ColorSlot::Cursor.label(), "Accent and cursor");
    }
}

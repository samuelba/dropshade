//! Resolves terminal colors (named, indexed, true color) against a palette.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Rgb};

/// The base colors a theme defines; the 256-color table is derived from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    /// Text under a block cursor; `None` uses the background.
    pub cursor_text: Option<Rgb>,
    /// Selected text and its background; `None` swaps the cell's own colors.
    pub selection_foreground: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub normal: [Rgb; 8],
    pub bright: [Rgb; 8],
}

/// Foreground and background of a cell after flags are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellColors {
    pub fg: Rgb,
    pub bg: Rgb,
}

const CUBE_START: usize = 16;
const GRAY_START: usize = 232;
const FOREGROUND: usize = NamedColor::Foreground as usize;
const BACKGROUND: usize = NamedColor::Background as usize;
const CURSOR: usize = NamedColor::Cursor as usize;
const DIM_START: usize = NamedColor::DimBlack as usize;
const BRIGHT_FOREGROUND: usize = NamedColor::BrightForeground as usize;
const DIM_FOREGROUND: usize = NamedColor::DimForeground as usize;

impl Palette {
    /// The palette color at an alacritty color index (0..269).
    pub fn indexed(&self, index: usize) -> Rgb {
        match index {
            0..8 => self.normal[index],
            8..CUBE_START => self.bright[index - 8],
            CUBE_START..GRAY_START => cube_color(index - CUBE_START),
            GRAY_START..FOREGROUND => {
                let level = gray_level(index - GRAY_START);
                Rgb {
                    r: level,
                    g: level,
                    b: level,
                }
            }
            FOREGROUND | BRIGHT_FOREGROUND => self.foreground,
            BACKGROUND => self.background,
            CURSOR => self.cursor,
            DIM_START..BRIGHT_FOREGROUND => dim(self.normal[index - DIM_START]),
            DIM_FOREGROUND => dim(self.foreground),
            _ => self.foreground,
        }
    }

    /// Resolves a cell color, preferring colors the application set via OSC 4/10/11.
    pub fn resolve(&self, color: AnsiColor, overrides: &Colors) -> Rgb {
        let index = match color {
            AnsiColor::Spec(rgb) => return rgb,
            AnsiColor::Named(named) => named as usize,
            AnsiColor::Indexed(index) => usize::from(index),
        };
        overrides[index].unwrap_or_else(|| self.indexed(index))
    }

    /// Resolves both colors of a cell and applies the dim, inverse and hidden flags.
    pub fn cell_colors(
        &self,
        fg: AnsiColor,
        bg: AnsiColor,
        flags: Flags,
        overrides: &Colors,
    ) -> CellColors {
        let mut fg = self.resolve(fg, overrides);
        if flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        let bg = self.resolve(bg, overrides);
        let (fg, bg) = if flags.contains(Flags::INVERSE) {
            (bg, fg)
        } else {
            (fg, bg)
        };
        let fg = if flags.contains(Flags::HIDDEN) {
            bg
        } else {
            fg
        };
        CellColors { fg, bg }
    }

    /// How a selected cell with these colors looks.
    pub fn selected(&self, colors: CellColors) -> CellColors {
        CellColors {
            fg: self.selection_foreground.unwrap_or(colors.bg),
            bg: self.selection_background.unwrap_or(colors.fg),
        }
    }

    /// The color of text under a block cursor.
    pub fn cursor_text(&self) -> Rgb {
        self.cursor_text.unwrap_or(self.background)
    }
}

#[allow(clippy::cast_possible_truncation)]
fn cube_color(offset: usize) -> Rgb {
    let level = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
    Rgb {
        r: level(offset / 36),
        g: level(offset % 36 / 6),
        b: level(offset % 6),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn gray_level(offset: usize) -> u8 {
    (8 + offset * 10) as u8
}

#[allow(clippy::cast_possible_truncation)]
fn dim(rgb: Rgb) -> Rgb {
    let scale = |v: u8| (u16::from(v) * 2 / 3) as u8;
    Rgb {
        r: scale(rgb.r),
        g: scale(rgb.g),
        b: scale(rgb.b),
    }
}

/// Converts a terminal color into an iced color with the given opacity.
pub fn to_iced(rgb: Rgb, alpha: f32) -> iced::Color {
    iced::Color::from_rgba8(rgb.r, rgb.g, rgb.b, alpha)
}

#[allow(clippy::cast_possible_truncation)]
const fn hex(value: u32) -> Rgb {
    Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

impl Palette {
    /// Dropshade's default theme, "Dusk": a deep blue-gray with a warm amber cursor.
    pub const DUSK: Self = Self {
        foreground: hex(0xd6_db_e4),
        background: hex(0x16_18_1d),
        cursor: hex(0xf2_a6_5a),
        cursor_text: None,
        selection_foreground: None,
        selection_background: None,
        normal: [
            hex(0x23_26_2e),
            hex(0xef_6b_73),
            hex(0x8f_d1_8b),
            hex(0xf2_c4_6d),
            hex(0x6a_a8_f7),
            hex(0xc7_92_ea),
            hex(0x5f_d4_d0),
            hex(0xc7_cc_d6),
        ],
        bright: [
            hex(0x4a_50_5c),
            hex(0xff_8a_91),
            hex(0xa9_e3_a5),
            hex(0xff_d8_8a),
            hex(0x8c_bc_ff),
            hex(0xdb_b0_ff),
            hex(0x86_ec_e8),
            hex(0xf1_f3_f7),
        ],
    };
}

impl Default for Palette {
    fn default() -> Self {
        Self::DUSK
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    fn palette() -> Palette {
        Palette {
            foreground: rgb(200, 200, 200),
            background: rgb(10, 10, 10),
            cursor: rgb(255, 128, 0),
            cursor_text: None,
            selection_foreground: None,
            selection_background: None,
            normal: std::array::from_fn(|i| rgb(i as u8, 0, 0)),
            bright: std::array::from_fn(|i| rgb(0, i as u8, 0)),
        }
    }

    #[test]
    fn indexed_covers_base_cube_and_grayscale() {
        let p = palette();
        assert_eq!(p.indexed(1), rgb(1, 0, 0));
        assert_eq!(p.indexed(9), rgb(0, 1, 0));
        assert_eq!(p.indexed(16), rgb(0, 0, 0));
        assert_eq!(p.indexed(21), rgb(0, 0, 255));
        assert_eq!(p.indexed(196), rgb(255, 0, 0));
        assert_eq!(p.indexed(110), rgb(135, 175, 215));
        assert_eq!(p.indexed(232), rgb(8, 8, 8));
        assert_eq!(p.indexed(255), rgb(238, 238, 238));
    }

    #[test]
    fn named_special_colors_come_from_palette() {
        let p = palette();
        let none = Colors::default();
        assert_eq!(
            p.resolve(AnsiColor::Named(NamedColor::Foreground), &none),
            p.foreground
        );
        assert_eq!(
            p.resolve(AnsiColor::Named(NamedColor::Background), &none),
            p.background
        );
        assert_eq!(
            p.resolve(AnsiColor::Named(NamedColor::Cursor), &none),
            p.cursor
        );
        assert_eq!(
            p.resolve(AnsiColor::Named(NamedColor::BrightRed), &none),
            rgb(0, 1, 0)
        );
    }

    #[test]
    fn terminal_overrides_win() {
        let mut overrides = Colors::default();
        overrides[NamedColor::Foreground] = Some(rgb(1, 2, 3));
        overrides[5] = Some(rgb(4, 5, 6));
        let p = palette();
        assert_eq!(
            p.resolve(AnsiColor::Named(NamedColor::Foreground), &overrides),
            rgb(1, 2, 3)
        );
        assert_eq!(p.resolve(AnsiColor::Indexed(5), &overrides), rgb(4, 5, 6));
    }

    #[test]
    fn true_color_is_passed_through() {
        let p = palette();
        assert_eq!(
            p.resolve(AnsiColor::Spec(rgb(9, 8, 7)), &Colors::default()),
            rgb(9, 8, 7)
        );
    }

    #[test]
    fn cell_flags_adjust_colors() {
        let p = palette();
        let none = Colors::default();
        let fg = AnsiColor::Named(NamedColor::Foreground);
        let bg = AnsiColor::Named(NamedColor::Background);

        let plain = p.cell_colors(fg, bg, Flags::empty(), &none);
        assert_eq!(
            plain,
            CellColors {
                fg: p.foreground,
                bg: p.background
            }
        );

        let inverse = p.cell_colors(fg, bg, Flags::INVERSE, &none);
        assert_eq!(
            inverse,
            CellColors {
                fg: p.background,
                bg: p.foreground
            }
        );

        let hidden = p.cell_colors(fg, bg, Flags::HIDDEN, &none);
        assert_eq!(hidden.fg, p.background);

        let dim = p.cell_colors(fg, bg, Flags::DIM, &none);
        assert_eq!(dim.fg, rgb(133, 133, 133));
    }

    #[test]
    fn selections_swap_colors_unless_the_theme_sets_them() {
        let cell = CellColors {
            fg: rgb(1, 1, 1),
            bg: rgb(2, 2, 2),
        };
        let swapped = CellColors {
            fg: cell.bg,
            bg: cell.fg,
        };
        assert_eq!(palette().selected(cell), swapped);
        let themed = Palette {
            selection_background: Some(rgb(9, 9, 9)),
            ..palette()
        };
        assert_eq!(
            themed.selected(cell),
            CellColors {
                fg: cell.bg,
                bg: rgb(9, 9, 9)
            }
        );
        let both = Palette {
            selection_foreground: Some(rgb(7, 7, 7)),
            ..themed
        };
        assert_eq!(both.selected(cell).fg, rgb(7, 7, 7));
    }

    #[test]
    fn cursor_text_defaults_to_the_background() {
        assert_eq!(palette().cursor_text(), palette().background);
        let themed = Palette {
            cursor_text: Some(rgb(3, 3, 3)),
            ..palette()
        };
        assert_eq!(themed.cursor_text(), rgb(3, 3, 3));
    }
}

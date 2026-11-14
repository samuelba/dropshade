//! Kitty's Unicode placeholders: cells holding U+10EEEE that stand for one cell of an image.
//!
//! The cell's foreground colour names the image (or Dropshade's placement key), and combining
//! diacritics give the image row, the image column and the key's most significant byte.

use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, Rgb};

pub const PLACEHOLDER: char = '\u{10EEEE}';

/// The diacritics that encode numbers, in order: the n-th one means n.
const DIACRITICS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

/// The largest row or column a placeholder can name.
pub const MAX_INDEX: usize = DIACRITICS.len() - 1;

pub fn diacritic(index: usize) -> Option<char> {
    DIACRITICS.get(index).copied()
}

fn index_of(c: char) -> Option<usize> {
    DIACRITICS.binary_search(&c).ok()
}

/// What one placeholder cell says, before missing parts are filled in from its left neighbour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parsed {
    pub key: u32,
    pub row: Option<usize>,
    pub column: Option<usize>,
}

/// Reads a placeholder cell, or `None` for any other cell.
pub fn parse(cell: &Cell) -> Option<Parsed> {
    if cell.c != PLACEHOLDER {
        return None;
    }
    let low = match cell.fg {
        Color::Spec(Rgb { r, g, b }) => u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        Color::Indexed(index) => u32::from(index),
        Color::Named(_) => return None,
    };
    let mut marks = cell
        .zerowidth()
        .unwrap_or_default()
        .iter()
        .map(|&c| index_of(c));
    let row = marks.next().flatten();
    let column = marks.next().flatten();
    let high = marks
        .next()
        .flatten()
        .and_then(|high| u32::try_from(high).ok())
        .unwrap_or(0);
    Some(Parsed {
        key: high << 24 | low,
        row,
        column,
    })
}

/// The foreground colour that names `key`, whose top byte must be zero.
pub fn key_color(key: u32) -> Color {
    let [_, r, g, b] = key.to_be_bytes();
    Color::Spec(Rgb { r, g, b })
}

/// Makes `cell` the placeholder for `row` and `column` of placement `key`.
pub fn fill(cell: &mut Cell, key: u32, row: usize, column: usize) {
    *cell = Cell::default();
    cell.c = PLACEHOLDER;
    cell.fg = key_color(key);
    if let (Some(row), Some(column)) = (diacritic(row), diacritic(column)) {
        cell.push_zerowidth(row);
        cell.push_zerowidth(column);
    }
}

/// `text` without placeholders and the diacritics that follow them, for copying.
pub fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_placeholder = false;
    for c in text.chars() {
        if c == PLACEHOLDER {
            in_placeholder = true;
        } else if !(in_placeholder && index_of(c).is_some()) {
            in_placeholder = false;
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_text_leaves_images_out() {
        let text = "a\u{10EEEE}\u{305}\u{305}\u{10EEEE}\u{305}\u{30D}b\u{305}";
        assert_eq!(strip(text), "ab\u{305}");
    }

    #[test]
    fn diacritics_are_sorted_so_they_can_be_searched() {
        assert!(DIACRITICS.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(index_of('\u{0305}'), Some(0));
        assert_eq!(index_of('\u{1D244}'), Some(296));
        assert_eq!(index_of('a'), None);
    }

    #[test]
    fn a_filled_cell_reads_back() {
        let mut cell = Cell::default();
        fill(&mut cell, 0x12_3456, 3, 7);
        assert_eq!(
            parse(&cell),
            Some(Parsed {
                key: 0x12_3456,
                row: Some(3),
                column: Some(7)
            })
        );
    }

    #[test]
    fn an_application_may_leave_out_diacritics_and_use_indexed_colours() {
        let cell = Cell {
            c: PLACEHOLDER,
            fg: Color::Indexed(42),
            ..Cell::default()
        };
        assert_eq!(
            parse(&cell),
            Some(Parsed {
                key: 42,
                row: None,
                column: None
            })
        );
    }

    #[test]
    fn the_third_diacritic_is_the_top_byte_of_the_key() {
        let mut cell = Cell::default();
        fill(&mut cell, 1, 0, 0);
        cell.push_zerowidth(DIACRITICS[2]);
        assert_eq!(parse(&cell).map(|parsed| parsed.key), Some(0x0200_0001));
    }

    #[test]
    fn other_cells_are_not_placeholders() {
        assert_eq!(parse(&Cell::default()), None);
    }
}

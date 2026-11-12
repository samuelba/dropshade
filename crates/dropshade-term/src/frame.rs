//! Turns the visible part of a terminal grid into styled text runs for drawing.

use alacritty_terminal::Term;
use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::point_to_viewport;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Rgb};

use crate::colors::{CellColors, Palette};

/// Text attributes that change how a run is drawn.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    /// Drawn with the color emoji font, in a run of its own.
    pub emoji: bool,
}

/// Consecutive cells on one row that share colors and style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub row: usize,
    pub column: usize,
    /// Width in cells.
    pub width: usize,
    pub text: String,
    pub colors: CellColors,
    pub style: RunStyle,
}

/// Where and how to draw the cursor, in viewport cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub column: usize,
    pub shape: CursorShape,
    pub wide: bool,
}

/// Everything needed to draw one frame of a terminal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frame {
    pub runs: Vec<Run>,
    pub cursor: Option<Cursor>,
}

impl From<Flags> for RunStyle {
    fn from(flags: Flags) -> Self {
        Self {
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strikeout: flags.contains(Flags::STRIKEOUT),
            emoji: false,
        }
    }
}

/// Builds the frame for the current viewport of `term`.
pub fn build<T: EventListener>(term: &Term<T>, palette: &Palette) -> Frame {
    let content = term.renderable_content();
    let default_bg = palette.resolve(AnsiColor::Named(NamedColor::Background), content.colors);
    let mut runs = RunCollector::new(default_bg);

    for indexed in content.display_iter {
        let cell = indexed.cell;
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let Some(point) = point_to_viewport(content.display_offset, indexed.point) else {
            continue;
        };
        let mut colors = palette.cell_colors(cell.fg, cell.bg, cell.flags, content.colors);
        if content
            .selection
            .is_some_and(|selection| selection.contains(indexed.point))
        {
            colors = palette.selected(colors);
        }
        let zerowidth = cell.zerowidth().unwrap_or_default();
        let wide = cell.flags.contains(Flags::WIDE_CHAR);
        runs.push(CellInput {
            row: point.line,
            column: point.column.0,
            c: cell.c,
            zerowidth,
            colors,
            style: RunStyle {
                emoji: is_emoji(cell.c, zerowidth, wide),
                ..RunStyle::from(cell.flags)
            },
            wide,
        });
    }

    let cursor = (content.cursor.shape != CursorShape::Hidden)
        .then(|| point_to_viewport(content.display_offset, content.cursor.point))
        .flatten()
        .map(|point| Cursor {
            row: point.line,
            column: point.column.0,
            shape: content.cursor.shape,
            wide: term.grid()[content.cursor.point]
                .flags
                .contains(Flags::WIDE_CHAR),
        });

    Frame {
        runs: runs.finish(),
        cursor,
    }
}

/// Variation selector 16: show the preceding character as an emoji.
const EMOJI_PRESENTATION: char = '\u{FE0F}';
/// Variation selector 15: show the preceding character as text.
const TEXT_PRESENTATION: char = '\u{FE0E}';

/// Whether a cell holds an emoji: a character the program asked to show as one (followed by
/// VS16), or a double-width pictograph. A narrow symbol on its own, like a plain ⚠, is text.
fn is_emoji(c: char, zerowidth: &[char], wide: bool) -> bool {
    if zerowidth.contains(&TEXT_PRESENTATION) {
        return false;
    }
    if zerowidth.contains(&EMOJI_PRESENTATION) {
        return true;
    }
    wide && matches!(
        u32::from(c),
        0x1F000..=0x1FAFF | 0x2300..=0x23FF | 0x2600..=0x27BF | 0x2B00..=0x2BFF
    )
}

#[derive(Clone, Copy)]
struct CellInput<'a> {
    row: usize,
    column: usize,
    c: char,
    zerowidth: &'a [char],
    colors: CellColors,
    style: RunStyle,
    wide: bool,
}

/// Accumulates cells into runs, dropping blank cells on the default background.
struct RunCollector {
    default_bg: Rgb,
    runs: Vec<Run>,
    current: Option<(Run, bool)>,
}

impl RunCollector {
    fn new(default_bg: Rgb) -> Self {
        Self {
            default_bg,
            runs: Vec::new(),
            current: None,
        }
    }

    fn push(&mut self, cell: CellInput<'_>) {
        let alone = cell.wide || cell.style.emoji;
        if let Some((run, false)) = &mut self.current
            && !alone
            && run.row == cell.row
            && run.column + run.width == cell.column
            && run.colors == cell.colors
            && run.style == cell.style
        {
            run.text.push(cell.c);
            run.text.extend(cell.zerowidth);
            run.width += 1;
            return;
        }
        self.flush();
        let mut text = String::from(cell.c);
        text.extend(cell.zerowidth);
        let run = Run {
            row: cell.row,
            column: cell.column,
            width: if cell.wide { 2 } else { 1 },
            text,
            colors: cell.colors,
            style: cell.style,
        };
        self.current = Some((run, alone));
    }

    fn flush(&mut self) {
        let Some((run, _)) = self.current.take() else {
            return;
        };
        let decorated = run.style.underline || run.style.strikeout;
        let run = if run.colors.bg == self.default_bg && !decorated {
            let text = run.text.trim_end_matches(' ').to_owned();
            if text.is_empty() {
                return;
            }
            let width = run.width - (run.text.len() - text.len());
            Run { width, text, ..run }
        } else {
            run
        };
        self.runs.push(run);
    }

    fn finish(mut self) -> Vec<Run> {
        self.flush();
        self.runs
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::size::GridSize;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    fn palette() -> Palette {
        let gray = |v| Rgb { r: v, g: v, b: v };
        Palette {
            foreground: gray(200),
            background: gray(10),
            cursor: gray(255),
            cursor_text: None,
            selection_foreground: None,
            selection_background: None,
            normal: std::array::from_fn(|i| Rgb {
                r: 100 + i as u8,
                g: 0,
                b: 0,
            }),
            bright: std::array::from_fn(|i| Rgb {
                r: 0,
                g: 100 + i as u8,
                b: 0,
            }),
        }
    }

    fn frame_for(bytes: &[u8]) -> Frame {
        let size = GridSize {
            columns: 10,
            lines: 3,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, bytes);
        build(&term, &palette())
    }

    fn selected_frame(palette: &Palette) -> Frame {
        use alacritty_terminal::index::{Column, Line, Point, Side};
        use alacritty_terminal::selection::{Selection, SelectionType};
        let size = GridSize {
            columns: 10,
            lines: 3,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, b"ab");
        let start = Point::new(Line(0), Column(0));
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(Point::new(Line(0), Column(0)), Side::Right);
        term.selection = Some(selection);
        build(&term, palette)
    }

    fn texts(frame: &Frame) -> Vec<(usize, usize, &str)> {
        frame
            .runs
            .iter()
            .map(|r| (r.row, r.column, r.text.as_str()))
            .collect()
    }

    #[test]
    fn plain_text_is_one_run_without_trailing_blanks() {
        let frame = frame_for(b"hi");
        assert_eq!(texts(&frame), vec![(0, 0, "hi")]);
        assert_eq!(frame.runs[0].width, 2);
        assert_eq!(frame.runs[0].colors.fg, palette().foreground);
    }

    #[test]
    fn inner_spaces_stay_in_the_run() {
        assert_eq!(texts(&frame_for(b"a   b")), vec![(0, 0, "a   b")]);
    }

    #[test]
    fn color_changes_split_runs() {
        let frame = frame_for(b"a\x1b[31mb");
        assert_eq!(texts(&frame), vec![(0, 0, "a"), (0, 1, "b")]);
        assert_eq!(frame.runs[1].colors.fg, palette().normal[1]);
    }

    #[test]
    fn style_flags_are_carried() {
        let frame = frame_for(b"\x1b[1;3;4;9mX");
        let style = frame.runs[0].style;
        assert_eq!(
            style,
            RunStyle {
                bold: true,
                italic: true,
                underline: true,
                strikeout: true,
                emoji: false,
            }
        );
    }

    #[test]
    fn colored_background_blanks_are_kept() {
        let frame = frame_for(b"\x1b[41m  ");
        assert_eq!(texts(&frame), vec![(0, 0, "  ")]);
        assert_eq!(frame.runs[0].colors.bg, palette().normal[1]);
    }

    #[test]
    fn wide_characters_get_their_own_two_cell_run() {
        let frame = frame_for("日x".as_bytes());
        assert_eq!(texts(&frame), vec![(0, 0, "日"), (0, 2, "x")]);
        assert_eq!(frame.runs[0].width, 2);
    }

    #[test]
    fn emoji_presentation_gets_its_own_run() {
        let frame = frame_for("a\u{26A0}\u{FE0F}b".as_bytes());
        assert_eq!(
            texts(&frame),
            vec![(0, 0, "a"), (0, 1, "\u{26A0}\u{FE0F}"), (0, 2, "b")]
        );
        assert!(frame.runs[1].style.emoji);
        assert!(!frame.runs[0].style.emoji && !frame.runs[2].style.emoji);
    }

    #[test]
    fn wide_pictographs_are_emoji_but_plain_symbols_and_cjk_are_text() {
        let frame = frame_for("\u{1F600}\u{26A0}\u{26A0}\u{FE0E}日".as_bytes());
        let emoji: Vec<bool> = frame.runs.iter().map(|r| r.style.emoji).collect();
        assert_eq!(texts(&frame)[0].2, "\u{1F600}");
        assert_eq!(emoji, vec![true, false, false]);
    }

    #[test]
    fn rows_are_separate() {
        assert_eq!(texts(&frame_for(b"a\r\nb")), vec![(0, 0, "a"), (1, 0, "b")]);
    }

    #[test]
    fn cursor_follows_output() {
        let cursor = frame_for(b"hi").cursor.expect("cursor visible");
        assert_eq!((cursor.row, cursor.column), (0, 2));
        assert_eq!(cursor.shape, CursorShape::Block);
    }

    #[test]
    fn hidden_cursor_is_not_drawn() {
        assert_eq!(frame_for(b"\x1b[?25l").cursor, None);
    }

    #[test]
    fn selections_use_the_theme_colors_or_swap() {
        let plain = selected_frame(&palette());
        assert_eq!(texts(&plain), vec![(0, 0, "a"), (0, 1, "b")]);
        assert_eq!(plain.runs[0].colors.fg, palette().background);
        assert_eq!(plain.runs[0].colors.bg, palette().foreground);

        let gray = |v| Rgb { r: v, g: v, b: v };
        let themed = Palette {
            selection_foreground: Some(gray(1)),
            selection_background: Some(gray(2)),
            ..palette()
        };
        let frame = selected_frame(&themed);
        assert_eq!(frame.runs[0].colors.fg, gray(1));
        assert_eq!(frame.runs[0].colors.bg, gray(2));
        assert_eq!(frame.runs[1].colors.fg, themed.foreground);
    }
}

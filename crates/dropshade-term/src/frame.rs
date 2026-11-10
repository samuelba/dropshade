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
            colors = CellColors {
                fg: colors.bg,
                bg: colors.fg,
            };
        }
        runs.push(CellInput {
            row: point.line,
            column: point.column.0,
            c: cell.c,
            zerowidth: cell.zerowidth().unwrap_or_default(),
            colors,
            style: RunStyle::from(cell.flags),
            wide: cell.flags.contains(Flags::WIDE_CHAR),
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
        if let Some((run, false)) = &mut self.current
            && !cell.wide
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
        self.current = Some((run, cell.wide));
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
                strikeout: true
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
}

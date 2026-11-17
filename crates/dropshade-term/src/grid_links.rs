//! Finds the link under a grid cell, following soft wraps so a link longer than a row is found
//! whole.

use std::ops::Range;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};

use crate::hints::WrappedLine;
use crate::links;

/// How far a link is followed in each direction, so huge unbroken output stays cheap to hover.
const MAX_LINK_CELLS: usize = 16 * 1024;

/// How many rows above and below the viewport a soft-wrapped line is followed.
const MAX_HIDDEN_ROWS: i32 = 256;

/// A link in the grid: the columns it covers on each of its rows, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridLink {
    pub rows: Vec<(Line, Range<usize>)>,
    pub uri: String,
}

/// The link covering `point`: the run of cells sharing its OSC 8 hyperlink, or else a URL in
/// the text, both continuing over soft-wrapped rows.
pub fn link_at<T>(term: &Term<T>, point: Point) -> Option<GridLink> {
    let grid = term.grid();
    if let Some(link) = grid[point].hyperlink() {
        let cells = run(term, point, |cell| cell.hyperlink().as_ref() == Some(&link));
        return Some(GridLink {
            rows: rows(term, &cells),
            uri: link.uri().to_owned(),
        });
    }
    let cells: Vec<Point> = run(term, point, |cell| {
        is_spacer(cell) || links::is_url_char(cell.c)
    })
    .into_iter()
    .filter(|&p| !is_spacer(&grid[p]))
    .collect();
    let chars: Vec<char> = cells.iter().map(|&p| grid[p].c).collect();
    let index = cells.iter().position(|&p| p == point)?;
    let span = links::url_span_at(&chars, index)?;
    Some(GridLink {
        rows: rows(term, &cells[span.clone()]),
        uri: chars[span].iter().collect(),
    })
}

/// The lines of text on the `rows` grid rows from `top` on, each joined over its soft wraps,
/// including the wrapped parts above and below those rows.
pub fn wrapped_lines<T>(term: &Term<T>, top: Line, rows: usize) -> Vec<WrappedLine> {
    let grid = term.grid();
    let bottom = top + rows.saturating_sub(1);
    let first = (1..=MAX_HIDDEN_ROWS)
        .map(|up| top - up)
        .take_while(|&line| line >= term.topmost_line() && wraps(term, line))
        .last()
        .unwrap_or(top);
    let last = (0..MAX_HIDDEN_ROWS)
        .map(|down| bottom + down)
        .take_while(|&line| line < term.bottommost_line() && wraps(term, line))
        .last()
        .map_or(bottom, |line| line + 1);
    let mut lines: Vec<WrappedLine> = Vec::new();
    let mut joined = false;
    for line in (first.0..=last.0).map(Line) {
        let chars = (0..term.columns()).map(|c| grid[line][Column(c)].c);
        match lines.last_mut() {
            Some(current) if joined => current.chars.extend(chars),
            _ => lines.push(WrappedLine {
                first_row: isize::try_from(line.0 - top.0).unwrap_or_default(),
                chars: chars.collect(),
            }),
        }
        joined = wraps(term, line);
    }
    lines
}

/// The cells around `point`, in order, that are `part` of the same thing as it.
fn run<T>(term: &Term<T>, point: Point, part: impl Fn(&Cell) -> bool) -> Vec<Point> {
    let grid = term.grid();
    let before: Vec<Point> = std::iter::successors(previous(term, point), |&p| previous(term, p))
        .take_while(|&p| part(&grid[p]))
        .take(MAX_LINK_CELLS)
        .collect();
    let after = std::iter::successors(next(term, point), |&p| next(term, p))
        .take_while(|&p| part(&grid[p]))
        .take(MAX_LINK_CELLS);
    before
        .into_iter()
        .rev()
        .chain(std::iter::once(point))
        .chain(after)
        .collect()
}

fn previous<T>(term: &Term<T>, point: Point) -> Option<Point> {
    if point.column.0 > 0 {
        return Some(Point::new(point.line, Column(point.column.0 - 1)));
    }
    let line = Line(point.line.0 - 1);
    (line >= term.topmost_line() && wraps(term, line)).then(|| Point::new(line, term.last_column()))
}

fn next<T>(term: &Term<T>, point: Point) -> Option<Point> {
    if point.column < term.last_column() {
        return Some(Point::new(point.line, Column(point.column.0 + 1)));
    }
    let line = Line(point.line.0 + 1);
    (line <= term.bottommost_line() && wraps(term, point.line)).then(|| Point::new(line, Column(0)))
}

fn wraps<T>(term: &Term<T>, line: Line) -> bool {
    term.grid()[line][term.last_column()]
        .flags
        .contains(Flags::WRAPLINE)
}

fn is_spacer(cell: &Cell) -> bool {
    cell.flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

/// Groups consecutive cells into the columns they cover on each row.
fn rows<T>(term: &Term<T>, cells: &[Point]) -> Vec<(Line, Range<usize>)> {
    let grid = term.grid();
    let mut rows: Vec<(Line, Range<usize>)> = Vec::new();
    for &p in cells {
        let width = if grid[p].flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        let end = p.column.0 + width;
        match rows.last_mut() {
            Some((line, columns)) if *line == p.line => columns.end = end,
            _ => rows.push((p.line, p.column.0..end)),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::size::GridSize;

    fn term(bytes: &[u8]) -> Term<VoidListener> {
        let size = GridSize {
            columns: 20,
            lines: 4,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        crate::cluster::advance(&mut Processor::new(), &mut term, bytes);
        term
    }

    fn at(term: &Term<VoidListener>, line: i32, column: usize) -> Option<GridLink> {
        link_at(term, Point::new(Line(line), Column(column)))
    }

    #[test]
    fn osc8_links_cover_their_cells() {
        let term = term(b"ab \x1b]8;;https://x.test\x1b\\link\x1b]8;;\x1b\\ cd");
        let link = Some(GridLink {
            rows: vec![(Line(0), 3..7)],
            uri: "https://x.test".to_owned(),
        });
        assert_eq!(at(&term, 0, 3), link);
        assert_eq!(at(&term, 0, 6), link);
        assert_eq!(at(&term, 0, 2), None);
        assert_eq!(at(&term, 0, 7), None);
    }

    #[test]
    fn urls_in_the_text_are_links_too() {
        let term = term(b"see https://x.test/a.");
        assert_eq!(
            at(&term, 0, 8),
            Some(GridLink {
                rows: vec![(Line(0), 4..20)],
                uri: "https://x.test/a".to_owned(),
            })
        );
        assert_eq!(at(&term, 0, 2), None);
    }

    #[test]
    fn urls_continue_over_soft_wrapped_rows() {
        let term = term(b"see https://example.test/a/very/long/path?q=1 end");
        let link = Some(GridLink {
            rows: vec![(Line(0), 4..20), (Line(1), 0..20), (Line(2), 0..5)],
            uri: "https://example.test/a/very/long/path?q=1".to_owned(),
        });
        assert_eq!(at(&term, 0, 5), link);
        assert_eq!(at(&term, 1, 10), link);
        assert_eq!(at(&term, 2, 4), link);
        assert_eq!(at(&term, 2, 7), None);
    }

    #[test]
    fn hard_line_breaks_end_a_url() {
        let term = term(b"https://a.test/x\r\nmore");
        assert_eq!(
            at(&term, 0, 2).map(|link| link.uri),
            Some("https://a.test/x".to_owned())
        );
        assert_eq!(at(&term, 1, 1), None);
    }

    #[test]
    fn wrapped_lines_join_soft_wrapped_rows() {
        let term = term(b"https://example.test/a/very/long/path\r\nnext");
        let lines = wrapped_lines(&term, Line(1), 2);
        let text = |line: &WrappedLine| line.chars.iter().collect::<String>();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].first_row, -1);
        assert!(text(&lines[0]).starts_with("https://example.test/a/very/long/path"));
        assert_eq!(lines[0].chars.len(), 40);
        assert_eq!(lines[1].first_row, 1);
        assert!(text(&lines[1]).starts_with("next"));
    }

    #[test]
    fn osc8_links_continue_over_soft_wrapped_rows() {
        let term = term(b"0123456789 \x1b]8;;https://x.test\x1b\\a long label\x1b]8;;\x1b\\ cd");
        let link = Some(GridLink {
            rows: vec![(Line(0), 11..20), (Line(1), 0..3)],
            uri: "https://x.test".to_owned(),
        });
        assert_eq!(at(&term, 0, 11), link);
        assert_eq!(at(&term, 1, 2), link);
        assert_eq!(at(&term, 1, 4), None);
    }
}

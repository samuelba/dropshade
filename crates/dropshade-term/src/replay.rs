//! A terminal's text as the bytes that draw it again, to show it after a restart.

use alacritty_terminal::grid::{Dimensions, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Ends the replayed text, above the new shell's first prompt.
const MARKER: &str = "\x1b[0;2m── restored ──\x1b[0m\r\n";

const STYLE_FLAGS: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

/// The main screen's last `max_lines` lines up to the cursor, as text and SGR sequences, ending
/// with a marker; empty when there is no text.
///
/// Leaves the alternate screen first, so call it only on a terminal that is about to go away.
pub(crate) fn dump_main_screen<T>(term: &mut Term<T>, max_lines: usize) -> Vec<u8> {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        term.swap_alt();
    }
    dump(term, max_lines)
}

fn dump<T>(term: &Term<T>, max_lines: usize) -> Vec<u8> {
    let grid = term.grid();
    let columns = term.columns();
    let last = grid.cursor.point.line.0;
    let count = i32::try_from(max_lines).unwrap_or(i32::MAX);
    let first = term.topmost_line().0.max(last.saturating_sub(count) + 1);
    let mut out = String::new();
    let mut style = Style::default();
    for line in first..=last {
        let row = &grid[Line(line)];
        let wraps = line != last && row[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        let end = if wraps {
            columns
        } else {
            content_end(row, columns)
        };
        if out.is_empty() && end == 0 {
            continue;
        }
        for cell in (0..end).map(|column| &row[Column(column)]) {
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let next = Style::of(cell);
            if next != style {
                next.write_sgr(&mut out);
                style = next;
            }
            out.push(cell.c);
            out.extend(cell.zerowidth().into_iter().flatten());
        }
        if !wraps {
            if style != Style::default() {
                out.push_str("\x1b[0m");
                style = Style::default();
            }
            out.push_str("\r\n");
        }
    }
    if out.trim().is_empty() {
        return Vec::new();
    }
    out.push_str(MARKER);
    out.into_bytes()
}

/// One past the last cell of `row` that shows something.
fn content_end(row: &Row<Cell>, columns: usize) -> usize {
    (0..columns)
        .rev()
        .find(|&column| {
            let cell = &row[Column(column)];
            cell.c != ' '
                || cell.bg != Color::Named(NamedColor::Background)
                || cell
                    .flags
                    .intersects(Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT)
        })
        .map_or(0, |column| column + 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: Color,
    bg: Color,
    flags: Flags,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
        }
    }
}

impl Style {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & STYLE_FLAGS,
        }
    }

    fn write_sgr(&self, out: &mut String) {
        out.push_str("\x1b[0");
        let attributes = [
            (Flags::BOLD, "1"),
            (Flags::DIM, "2"),
            (Flags::ITALIC, "3"),
            (Flags::UNDERLINE, "4"),
            (Flags::DOUBLE_UNDERLINE, "21"),
            (Flags::UNDERCURL, "4:3"),
            (Flags::DOTTED_UNDERLINE, "4:4"),
            (Flags::DASHED_UNDERLINE, "4:5"),
            (Flags::INVERSE, "7"),
            (Flags::HIDDEN, "8"),
            (Flags::STRIKEOUT, "9"),
        ];
        for (flag, code) in attributes {
            if self.flags.contains(flag) {
                out.push(';');
                out.push_str(code);
            }
        }
        write_color(out, self.fg, 30, 90, 38);
        write_color(out, self.bg, 40, 100, 48);
        out.push('m');
    }
}

/// `color` as an SGR parameter: `normal` + n for the 8 colors, `bright` + n for their bright
/// versions, `extended` for 256-color and truecolor. Default colors need none.
fn write_color(out: &mut String, color: Color, normal: u16, bright: u16, extended: u16) {
    let code = match color {
        Color::Named(named) => {
            let index = named as u16;
            let dim_black = NamedColor::DimBlack as u16;
            match index {
                0..8 => normal + index,
                8..16 => bright + index - 8,
                _ if (dim_black..dim_black + 8).contains(&index) => normal + index - dim_black,
                _ => return,
            }
            .to_string()
        }
        Color::Indexed(index) => format!("{extended};5;{index}"),
        Color::Spec(rgb) => format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    };
    out.push(';');
    out.push_str(&code);
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;
    use crate::size::GridSize;

    fn term(columns: usize, lines: usize) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: 100,
            ..Config::default()
        };
        Term::new(config, &GridSize { columns, lines }, VoidListener)
    }

    fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
        Processor::<StdSyncHandler>::new().advance(term, bytes);
    }

    fn text(term: &Term<VoidListener>) -> Vec<String> {
        (term.topmost_line().0..=term.bottommost_line().0)
            .map(|line| {
                let row = &term.grid()[Line(line)];
                (0..term.columns())
                    .map(|column| &row[Column(column)])
                    .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                    .flat_map(|cell| {
                        std::iter::once(cell.c)
                            .chain(cell.zerowidth().into_iter().flatten().copied())
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .filter(|line| !line.is_empty())
            .collect()
    }

    fn replayed(
        source: &mut Term<VoidListener>,
        columns: usize,
        max_lines: usize,
    ) -> Term<VoidListener> {
        let bytes = dump_main_screen(source, max_lines);
        let mut target = term(columns, 10);
        feed(&mut target, &bytes);
        target
    }

    #[test]
    fn colors_and_attributes_survive_a_round_trip() {
        let mut source = term(40, 5);
        feed(
            &mut source,
            b"\x1b[1;31mred\x1b[0m \x1b[3;4;38;5;200mpink\x1b[0m \x1b[38;2;1;2;3;48;5;17mrgb\x1b[0m \x1b[7;95minv\x1b[0m\r\n$ ",
        );
        let target = replayed(&mut source, 40, 100);
        let (from, to) = (&source.grid()[Line(0)], &target.grid()[Line(0)]);
        for column in (0..19).map(Column) {
            assert_eq!(
                Style::of(&from[column]),
                Style::of(&to[column]),
                "{column:?}"
            );
            assert_eq!(from[column].c, to[column].c);
        }
    }

    #[test]
    fn a_wrapped_line_comes_back_as_one_line() {
        let mut source = term(10, 5);
        feed(&mut source, b"abcdefghijklmno\r\n$ ");
        let target = replayed(&mut source, 20, 100);
        assert_eq!(text(&target), ["abcdefghijklmno", "$", "── restored ──"]);
    }

    #[test]
    fn wide_and_combining_characters_survive() {
        let mut source = term(20, 5);
        feed(&mut source, "日本 e\u{301}\r\n".as_bytes());
        let target = replayed(&mut source, 20, 100);
        assert_eq!(text(&target)[0], "日本 e\u{301}");
    }

    #[test]
    fn keeps_only_the_last_lines() {
        let mut source = term(20, 5);
        let lines = (1..=30).fold(String::new(), |lines, n| lines + &format!("line {n}\r\n"));
        feed(&mut source, lines.as_bytes());
        feed(&mut source, b"$ ");
        let target = replayed(&mut source, 20, 3);
        assert_eq!(text(&target), ["line 29", "line 30", "$", "── restored ──"]);
    }

    #[test]
    fn an_empty_terminal_dumps_nothing() {
        assert_eq!(dump_main_screen(&mut term(20, 5), 100), b"");
    }

    #[test]
    fn the_main_screen_is_saved_instead_of_a_full_screen_app() {
        let mut source = term(20, 5);
        feed(
            &mut source,
            b"before vim\r\n$ vim\r\n\x1b[?1049h\x1b[Hinside vim",
        );
        let target = replayed(&mut source, 20, 100);
        let lines = text(&target);
        assert_eq!(lines[0], "before vim");
        assert!(!lines.iter().any(|line| line.contains("inside")));
    }
}

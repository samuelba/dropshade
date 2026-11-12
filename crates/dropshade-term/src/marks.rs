//! Where the shell's prompts, commands and command output are, from OSC 133 marks.
//!
//! Marks are kept in grid coordinates (negative lines are scrollback) and follow the text as it
//! scrolls. `alacritty_terminal` has no count of scrolled lines, but while the viewport is
//! scrolled up it moves the viewport along with every line that scrolls into the history. So
//! [`PromptMarks::begin`] scrolls the viewport up one line before output is parsed, and
//! [`PromptMarks::finish`] reads how far it moved and puts it back. Both run under the terminal
//! lock, so the UI never sees the probe.

use std::collections::VecDeque;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};

use crate::osc133::MarkKind;

/// Upper bound on remembered marks, a few per command.
const MAX_MARKS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    kind: MarkKind,
    point: Point,
}

/// How the scrolling of one parse is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    /// The alternate screen was active; the primary grid does not move.
    Alt,
    /// The viewport was moved to one line above the live end; `user` is where it was.
    Offset { user: usize },
    /// No scrollback yet, so its growth is the number of scrolled lines.
    History { before: usize },
}

/// A mark's place in the text, which survives reflow: the number of logical lines between its
/// own and the cursor's, and how many cells into its logical line it is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Anchor {
    lines_above_cursor: isize,
    offset: usize,
}

/// The OSC 133 marks of one terminal.
#[derive(Debug, Default)]
pub struct PromptMarks {
    marks: VecDeque<Mark>,
    /// The kind of the newest mark, kept even when its position is not.
    last: Option<MarkKind>,
    /// A probe left on the primary grid when the alternate screen was entered mid-parse.
    pending: Option<Probe>,
    /// The scrollback limit; a viewport move this large may have been cut short.
    max_history: usize,
}

impl PromptMarks {
    pub fn new(max_history: usize) -> Self {
        Self {
            max_history,
            ..Self::default()
        }
    }

    pub fn set_max_history(&mut self, max_history: usize) {
        self.max_history = max_history;
    }

    pub fn clear(&mut self) {
        self.marks.clear();
        self.pending = None;
    }

    /// Whether a command is running, or `None` until the shell sent its first mark.
    pub fn command_running(&self) -> Option<bool> {
        self.last.map(|kind| kind == MarkKind::OutputStart)
    }

    /// Prepares to measure how far the output about to be parsed scrolls the primary grid.
    pub(crate) fn begin<T>(term: &mut Term<T>) -> Probe {
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return Probe::Alt;
        }
        let grid = term.grid_mut();
        let history = grid.history_size();
        if history == 0 {
            return Probe::History { before: 0 };
        }
        let user = grid.display_offset();
        grid.scroll_display(Scroll::Delta(1 - to_i32(user)));
        Probe::Offset { user }
    }

    /// Moves the marks along with the lines that scrolled since [`Self::begin`], and gives the
    /// viewport back.
    pub(crate) fn finish<T>(&mut self, term: &mut Term<T>, probe: Probe) {
        let probe = match probe {
            Probe::Alt => match self.pending.take() {
                Some(pending) => pending,
                None => return,
            },
            probe => probe,
        };
        if term.mode().contains(TermMode::ALT_SCREEN) {
            self.pending = Some(probe);
            return;
        }
        let grid = term.grid_mut();
        let history = grid.history_size();
        match probe {
            Probe::Alt => {}
            Probe::History { before } => match history.checked_sub(before) {
                Some(scrolled) => self.shift(scrolled, history),
                None => self.marks.clear(),
            },
            Probe::Offset { user } => {
                let offset = grid.display_offset();
                if offset == 0 {
                    // The scrollback was cleared or the terminal reset.
                    self.marks.clear();
                    return;
                }
                let scrolled = offset - 1;
                let restored = if user == 0 {
                    0
                } else {
                    (user + scrolled).min(history)
                };
                grid.scroll_display(Scroll::Delta(to_i32(restored) - to_i32(offset)));
                if self.max_history > 0 && offset >= self.max_history {
                    // The move may have been cut short; anything older scrolled out anyway.
                    self.marks.clear();
                } else {
                    self.shift(scrolled, history);
                }
            }
        }
    }

    fn shift(&mut self, scrolled: usize, history: usize) {
        if scrolled == 0 {
            return;
        }
        let scrolled = to_i32(scrolled);
        let top = -to_i32(history);
        for mark in &mut self.marks {
            mark.point.line -= scrolled;
        }
        self.marks.retain(|mark| mark.point.line >= top);
    }

    /// Records a mark at the cursor.
    pub(crate) fn record<T>(&mut self, term: &Term<T>, kind: MarkKind) {
        self.last = Some(kind);
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let point = term.grid().cursor.point;
        // A prompt drawn again in place (after a resize, say) replaces the earlier one.
        let redrawn = |mark: &Mark| match kind {
            MarkKind::PromptStart => {
                matches!(mark.kind, MarkKind::PromptStart | MarkKind::CommandStart)
                    && mark.point.line >= point.line
            }
            MarkKind::CommandStart => {
                mark.kind == MarkKind::CommandStart && mark.point.line >= point.line
            }
            MarkKind::OutputStart | MarkKind::CommandEnd(_) => false,
        };
        while self.marks.back().is_some_and(redrawn) {
            self.marks.pop_back();
        }
        self.marks.push_back(Mark { kind, point });
        if self.marks.len() > MAX_MARKS {
            self.marks.pop_front();
        }
    }

    /// The rows where prompts begin, oldest first.
    fn prompt_lines(&self) -> impl Iterator<Item = Line> + '_ {
        let mut previous = None;
        self.marks.iter().filter_map(move |mark| {
            let after = previous.replace(mark.kind);
            match mark.kind {
                MarkKind::PromptStart => Some(mark.point.line),
                MarkKind::CommandStart if after != Some(MarkKind::PromptStart) => {
                    Some(mark.point.line)
                }
                _ => None,
            }
        })
    }

    /// The nearest prompt that starts above `line`.
    pub fn previous_prompt(&self, line: Line) -> Option<Line> {
        self.prompt_lines().filter(|&prompt| prompt < line).max()
    }

    /// The nearest prompt that starts below `line`.
    pub fn next_prompt(&self, line: Line) -> Option<Line> {
        self.prompt_lines().filter(|&prompt| prompt > line).min()
    }

    /// Each command's output as its first and last cell, oldest first. A running command's
    /// output ends at the cursor.
    fn outputs(&self, cursor: Point, last_column: Column) -> impl Iterator<Item = (Point, Point)> {
        self.marks
            .iter()
            .enumerate()
            .filter_map(move |(index, mark)| {
                if mark.kind != MarkKind::OutputStart {
                    return None;
                }
                let end = self
                    .marks
                    .iter()
                    .skip(index + 1)
                    .find(|next| {
                        matches!(
                            next.kind,
                            MarkKind::CommandEnd(_) | MarkKind::PromptStart | MarkKind::OutputStart
                        )
                    })
                    .map_or(cursor, |next| next.point);
                let last = cell_before(end, last_column);
                (last >= mark.point).then_some((mark.point, last))
            })
    }

    /// The output that covers `line`.
    pub fn output_at(
        &self,
        line: Line,
        cursor: Point,
        last_column: Column,
    ) -> Option<(Point, Point)> {
        self.outputs(cursor, last_column)
            .filter(|(start, end)| start.line <= line && line <= end.line)
            .last()
    }

    /// The newest command's output.
    pub fn last_output(&self, cursor: Point, last_column: Column) -> Option<(Point, Point)> {
        self.outputs(cursor, last_column).last()
    }

    /// Notes where each mark is in the text before the grid is resized, or `None` while the
    /// primary grid is out of reach behind the alternate screen.
    pub(crate) fn anchors<T>(&self, term: &Term<T>) -> Option<Vec<Option<Anchor>>> {
        if term.mode().contains(TermMode::ALT_SCREEN) || self.marks.is_empty() {
            return None;
        }
        let starts = logical_starts(term);
        let columns = term.columns();
        let cursor = logical_index(&starts, term.grid().cursor.point.line)?;
        let anchors = self
            .marks
            .iter()
            .map(|mark| {
                let index = logical_index(&starts, mark.point.line)?;
                let rows = usize::try_from((mark.point.line - starts[index]).0).ok()?;
                Some(Anchor {
                    lines_above_cursor: to_isize(cursor) - to_isize(index),
                    offset: rows * columns + mark.point.column.0,
                })
            })
            .collect();
        Some(anchors)
    }

    /// Puts each mark back on its text after the grid was resized and reflowed.
    pub(crate) fn reanchor<T>(&mut self, term: &Term<T>, anchors: Vec<Option<Anchor>>) {
        let starts = logical_starts(term);
        let columns = term.columns().max(1);
        let Some(cursor) = logical_index(&starts, term.grid().cursor.point.line) else {
            self.marks.clear();
            return;
        };
        let marks = std::mem::take(&mut self.marks);
        self.marks = marks
            .into_iter()
            .zip(anchors)
            .filter_map(|(mark, anchor)| {
                let anchor = anchor?;
                let index = usize::try_from(to_isize(cursor) - anchor.lines_above_cursor).ok()?;
                let start = *starts.get(index)?;
                let point = Point::new(
                    start + to_i32(anchor.offset / columns),
                    Column(anchor.offset % columns),
                );
                (point.line <= term.bottommost_line()).then_some(Mark { point, ..mark })
            })
            .collect();
    }
}

/// The first row of every logical line (rows joined by soft wraps), top to bottom.
fn logical_starts<T>(term: &Term<T>) -> Vec<Line> {
    let grid = term.grid();
    let last_column = term.last_column();
    let mut starts = Vec::new();
    let mut wrapped = false;
    for line in (term.topmost_line().0..=term.bottommost_line().0).map(Line) {
        if !wrapped {
            starts.push(line);
        }
        wrapped = grid[line][last_column].flags.contains(Flags::WRAPLINE);
    }
    starts
}

/// The index of the logical line that contains `line`.
fn logical_index(starts: &[Line], line: Line) -> Option<usize> {
    starts
        .partition_point(|&start| start <= line)
        .checked_sub(1)
}

fn cell_before(point: Point, last_column: Column) -> Point {
    if point.column.0 == 0 {
        Point::new(point.line - 1, last_column)
    } else {
        Point::new(point.line, point.column - 1)
    }
}

fn to_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn to_isize(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;
    use crate::osc133::Osc133Scanner;
    use crate::size::GridSize;

    /// A terminal plus the same scan-parse-record steps the pty loop takes.
    struct Harness {
        term: Term<VoidListener>,
        parser: Processor<StdSyncHandler>,
        scanner: Osc133Scanner,
        marks: PromptMarks,
    }

    impl Harness {
        fn new(columns: usize, lines: usize, history: usize) -> Self {
            let config = Config {
                scrolling_history: history,
                ..Config::default()
            };
            let size = GridSize { columns, lines };
            Self {
                term: Term::new(config, &size, VoidListener),
                parser: Processor::default(),
                scanner: Osc133Scanner::default(),
                marks: PromptMarks::new(history),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut found = Vec::new();
            self.scanner
                .scan(bytes, |end, kind| found.push((end, kind)));
            let mut start = 0;
            for (end, kind) in found {
                self.parse(&bytes[start..end]);
                self.marks.record(&self.term, kind);
                start = end;
            }
            self.parse(&bytes[start..]);
        }

        fn parse(&mut self, bytes: &[u8]) {
            let probe = PromptMarks::begin(&mut self.term);
            self.parser.advance(&mut self.term, bytes);
            self.marks.finish(&mut self.term, probe);
        }

        /// Runs a command the way an integrated shell reports it.
        fn command(&mut self, name: &str, output: &str) {
            let bytes = format!(
                "\x1b]133;A\x07$ \x1b]133;B\x07{name}\r\n\x1b]133;C\x07{output}\x1b]133;D;0\x07"
            );
            self.feed(bytes.as_bytes());
        }

        fn prompt(&mut self) {
            self.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        }

        fn row(&self, line: Line) -> String {
            let row = &self.term.grid()[line];
            (0..self.term.columns())
                .map(|column| row[Column(column)].c)
                .collect::<String>()
                .trim_end()
                .to_owned()
        }

        fn text(&self, (start, end): (Point, Point)) -> Vec<String> {
            (start.line.0..=end.line.0)
                .map(|line| self.row(Line(line)))
                .collect()
        }

        fn last_output(&self) -> Option<(Point, Point)> {
            let cursor = self.term.grid().cursor.point;
            self.marks.last_output(cursor, self.term.last_column())
        }

        fn prompts(&self) -> Vec<String> {
            self.marks
                .prompt_lines()
                .map(|line| self.row(line))
                .collect()
        }
    }

    fn lines(count: usize, prefix: &str) -> String {
        (1..=count)
            .flat_map(|n| [prefix.to_owned(), n.to_string(), "\r\n".to_owned()])
            .collect()
    }

    #[test]
    fn command_output_lies_between_its_marks() {
        let mut h = Harness::new(20, 10, 100);
        h.command("ls", "a\r\nb\r\n");
        h.prompt();
        let output = h.last_output().expect("output");
        assert_eq!(h.text(output), ["a", "b"]);
        assert_eq!(h.prompts(), ["$ ls", "$"]);
    }

    #[test]
    fn whether_a_command_runs_follows_the_marks() {
        let mut h = Harness::new(20, 10, 100);
        assert_eq!(h.marks.command_running(), None);
        h.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07sleep\r\n\x1b]133;C\x07");
        assert_eq!(h.marks.command_running(), Some(true));
        h.feed(b"\x1b]133;D;0\x07\x1b]133;A\x07$ ");
        assert_eq!(h.marks.command_running(), Some(false));
    }

    #[test]
    fn marks_follow_the_text_once_the_scrollback_is_full() {
        let mut h = Harness::new(20, 5, 30);
        h.command("one", &lines(3, "x"));
        h.command("two", &lines(40, "y"));
        h.command("three", "z\r\n");
        h.prompt();
        assert_eq!(h.term.grid().history_size(), 30);
        assert_eq!(h.prompts(), ["$ three", "$"]);
        assert_eq!(h.text(h.last_output().unwrap()), ["z"]);
    }

    #[test]
    fn marks_follow_the_text_while_the_user_is_scrolled_up() {
        let mut h = Harness::new(20, 5, 100);
        h.command("one", &lines(10, "x"));
        h.term.scroll_display(Scroll::Delta(3));
        h.command("two", "y\r\n");
        h.prompt();
        // The viewport stays on the same text, as it would without the probe.
        assert_eq!(h.row(Line(-to_i32(h.term.grid().display_offset()))), "x4");
        assert_eq!(h.prompts(), ["$ one", "$ two", "$"]);
        assert_eq!(h.text(h.last_output().unwrap()), ["y"]);
    }

    #[test]
    fn clearing_the_scrollback_forgets_the_marks_in_it() {
        let mut h = Harness::new(20, 5, 100);
        h.command("one", &lines(10, "x"));
        h.feed(b"\x1b[3J");
        h.prompt();
        assert_eq!(h.prompts(), ["$"]);
    }

    #[test]
    fn a_full_screen_program_does_not_move_the_marks() {
        let mut h = Harness::new(20, 5, 100);
        h.command("one", &lines(10, "x"));
        h.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07vim\r\n\x1b]133;C\x07\x1b[?1049h");
        h.feed(lines(20, "vim").as_bytes());
        h.feed(b"\x1b[?1049l\x1b]133;D;0\x07");
        h.feed(lines(2, "after").as_bytes());
        h.prompt();
        assert_eq!(h.prompts(), ["$ one", "$ vim", "$"]);
    }

    #[test]
    fn marks_survive_a_reflow() {
        let mut h = Harness::new(20, 5, 100);
        h.command("one", "a long line that wraps around\r\n");
        h.command("two", "b\r\n");
        h.prompt();
        let anchors = h.marks.anchors(&h.term).unwrap();
        h.term.resize(GridSize {
            columns: 10,
            lines: 5,
        });
        h.marks.reanchor(&h.term, anchors);
        assert_eq!(h.prompts(), ["$ one", "$ two", "$"]);
        assert_eq!(h.text(h.last_output().unwrap()), ["b"]);
    }

    #[test]
    fn a_redrawn_prompt_replaces_the_old_one() {
        let mut h = Harness::new(20, 5, 100);
        h.prompt();
        h.feed(b"\r\x1b[K");
        h.prompt();
        assert_eq!(h.prompts(), ["$"]);
    }

    #[test]
    fn finds_the_prompts_around_a_line() {
        let mut h = Harness::new(20, 10, 100);
        h.command("one", "a\r\n");
        h.command("two", "b\r\n");
        h.prompt();
        let two = h.marks.prompt_lines().nth(1).unwrap();
        assert_eq!(h.row(h.marks.previous_prompt(two).unwrap()), "$ one");
        assert_eq!(h.row(h.marks.next_prompt(two).unwrap()), "$");
        assert_eq!(h.marks.previous_prompt(Line(0)), None);
    }
}

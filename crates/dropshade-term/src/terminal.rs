//! A shell running in a pty, with its `alacritty_terminal` emulation state.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side as GridSide};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{
    ClipboardType, Config, Term, TermMode, point_to_viewport, viewport_to_point,
};
use alacritty_terminal::tty;
use alacritty_terminal::vi_mode::ViMotion;
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
use iced::futures::Stream;
use iced::futures::channel::mpsc;

use crate::colors::Palette;
use crate::frame::{self, Frame};
use crate::input::paste_bytes;
use crate::links;
use crate::marks::PromptMarks;
use crate::mouse::CellPosition;
use crate::pty_loop::{self, Msg, PtySender};
use crate::reset;
use crate::size::{CellSize, GridSize, Side};
use crate::vi::{ViScroll, ViSelection};
use crate::view::ScrollPosition;

/// Forwards emulator events from the pty thread to the UI.
#[derive(Debug, Clone)]
pub struct EventProxy {
    sender: mpsc::UnboundedSender<Event>,
}

impl EventProxy {
    pub(crate) fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { sender }
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if self.sender.unbounded_send(event).is_err() {
            tracing::trace!("terminal event dropped: the UI side is gone");
        }
    }
}

/// How to start the shell.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Program and arguments. `None` runs the user's login shell.
    pub command: Option<(String, Vec<String>)>,
    pub working_directory: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub scrollback_lines: usize,
    /// Lets applications enable the kitty keyboard protocol (`CSI > flags u`).
    pub kitty_keyboard: bool,
    /// Cursor used until an application asks for another one.
    pub cursor_style: CursorStyle,
    pub size: GridSize,
    pub cell: CellSize,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            command: None,
            working_directory: None,
            env: HashMap::new(),
            scrollback_lines: 10_000,
            kitty_keyboard: true,
            cursor_style: CursorStyle::default(),
            size: GridSize {
                columns: 80,
                lines: 24,
            },
            cell: CellSize {
                width: 8.0,
                height: 16.0,
            },
        }
    }
}

/// The shell's environment: `TERM=xterm-256color` unless the user's `env` says otherwise.
///
/// Not `alacritty`: hosts reached over ssh rarely have that terminfo, and stock shell profiles
/// only enable a colour prompt for `*-256color` terminals.
fn child_env(user: HashMap<String, String>) -> HashMap<String, String> {
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .chain(user)
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("could not open a pty for the shell: {0}")]
    Pty(#[source] std::io::Error),
    #[error("could not start the terminal I/O loop: {0}")]
    EventLoop(#[source] std::io::Error),
}

/// A program running in the foreground of the shell, such as an editor or a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundProcess {
    pub pid: i32,
    pub name: String,
}

/// Which way to look for the next search match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Towards older output (up).
    Older,
    /// Towards newer output (down).
    Newer,
}

/// What the UI should do after an emulator event was handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The shell or an application printed something.
    Output,
    Redraw,
    TitleChanged,
    Bell,
    Exited(Option<i32>),
    /// The application asked to copy text (OSC 52) to the clipboard or primary selection.
    Copy {
        text: String,
        primary: bool,
    },
}

/// How a mouse selection grows: by cell, by word (double-click) or by line (triple-click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
}

impl SelectionKind {
    pub fn from_clicks(clicks: u8) -> Self {
        match clicks {
            0 | 1 => Self::Simple,
            2 => Self::Word,
            _ => Self::Line,
        }
    }
}

pub struct Terminal {
    term: Arc<FairMutex<Term<EventProxy>>>,
    sender: PtySender,
    /// Where the prompts and command output are, from the shell's OSC 133 marks.
    marks: Arc<Mutex<PromptMarks>>,
    size: GridSize,
    cell: CellSize,
    title: Option<String>,
    child_pid: u32,
    /// A duplicate of the pty master, used to ask which process group is in the foreground.
    pty: File,
    search: Option<RegexSearch>,
    search_match: Option<Match>,
    /// Which way searches go in vi mode: the way `/` or `?` started the last one.
    vi_direction: SearchDirection,
    /// Where the vi cursor was when the search bar opened; typing searches from there.
    search_origin: Option<Point>,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("size", &self.size)
            .field("title", &self.title)
            .field("child_pid", &self.child_pid)
            .finish_non_exhaustive()
    }
}

impl Terminal {
    /// Starts the shell and returns the terminal plus the stream of its emulator events.
    ///
    /// # Errors
    ///
    /// Fails when no pty can be opened or the I/O thread cannot be set up.
    pub fn spawn(
        options: SpawnOptions,
    ) -> Result<(Self, impl Stream<Item = Event> + Send + 'static), SpawnError> {
        let (sender, receiver) = mpsc::unbounded();
        let proxy = EventProxy::new(sender);
        let config = term_config(
            options.scrollback_lines,
            options.kitty_keyboard,
            options.cursor_style,
        );
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &options.size,
            proxy.clone(),
        )));

        let pty_options = tty::Options {
            shell: options
                .command
                .map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: options.working_directory,
            drain_on_exit: true,
            env: child_env(options.env),
        };
        let window_size = options.size.window_size(options.cell);
        let pty = tty::new(&pty_options, window_size, 0).map_err(SpawnError::Pty)?;
        let child_pid = pty.child().id();
        let pty_handle = pty.file().try_clone().map_err(SpawnError::Pty)?;
        let marks = Arc::new(Mutex::new(PromptMarks::new(options.scrollback_lines)));
        let loop_sender = pty_loop::spawn(pty, Arc::clone(&term), Arc::clone(&marks), proxy)
            .map_err(SpawnError::EventLoop)?;

        let terminal = Self {
            term,
            sender: loop_sender,
            marks,
            size: options.size,
            cell: options.cell,
            title: None,
            child_pid,
            pty: pty_handle,
            search: None,
            search_match: None,
            vi_direction: SearchDirection::Older,
            search_origin: None,
        };
        Ok((terminal, receiver))
    }

    pub fn size(&self) -> GridSize {
        self.size
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn frame(&self, palette: &Palette) -> Frame {
        frame::build(&self.term.lock(), palette)
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.send(Msg::Input(bytes.into()));
    }

    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        let mut term = self.term.lock();
        let mut marks = lock(&self.marks);
        let anchors = marks.anchors(&term);
        term.resize(size);
        if let Some(anchors) = anchors {
            marks.reanchor(&term, anchors);
        }
        drop((term, marks));
        self.send(Msg::Resize(size.window_size(self.cell)));
    }

    /// Adopts a new cell size, after a font or zoom change. The grid follows on the next layout.
    pub fn set_cell(&mut self, cell: CellSize) {
        if cell == self.cell {
            return;
        }
        self.cell = cell;
        self.send(Msg::Resize(self.size.window_size(cell)));
    }

    /// Resets the emulator state (modes, charsets, the alternate screen) and clears the screen
    /// into the scrollback.
    pub fn reset(&self) {
        {
            let mut term = self.term.lock();
            let mut marks = lock(&self.marks);
            let probe = PromptMarks::begin(&mut term);
            reset::soft(&mut term);
            marks.finish(&mut term, probe);
        }
        self.redraw_prompt();
    }

    /// A full reset: the emulator state, the screen and the scrollback.
    pub fn reset_and_clear(&mut self) {
        self.clear_search();
        reset::full(&mut self.term.lock());
        lock(&self.marks).clear();
        self.redraw_prompt();
    }

    /// Sends Ctrl+L so a shell waiting at its prompt draws it again on the cleared screen. Other
    /// programs get nothing, since many of them would print the control character.
    fn redraw_prompt(&self) {
        if self.foreground_process().is_none() {
            self.write(&b"\x0c"[..]);
        }
    }

    /// Applies changed settings to a running terminal.
    pub fn update_options(&self, scrollback_lines: usize, cursor_style: CursorStyle) {
        let mut term = self.term.lock();
        term.set_options(term_config(scrollback_lines, true, cursor_style));
        lock(&self.marks).set_max_history(scrollback_lines);
    }

    /// Jumps the viewport back to the live end of the output.
    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// Scrolls the viewport through the scrollback; positive values scroll up.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_page(&self, up: bool) {
        self.term
            .lock()
            .scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
    }

    pub fn scroll_to_top(&self) {
        self.term.lock().scroll_display(Scroll::Top);
    }

    /// Scrolls so the viewport is `offset` lines above the live end of the output.
    pub fn scroll_to(&self, offset: usize) {
        let mut term = self.term.lock();
        let grid = term.grid();
        let lines = |value: usize| i32::try_from(value).unwrap_or(i32::MAX);
        let target = lines(offset.min(grid.history_size()));
        let delta = target.saturating_sub(lines(grid.display_offset()));
        term.scroll_display(Scroll::Delta(delta));
    }

    /// How much scrollback there is and how far the viewport is scrolled into it.
    pub fn scroll_position(&self) -> ScrollPosition {
        let term = self.term.lock();
        let grid = term.grid();
        ScrollPosition {
            history: grid.history_size(),
            offset: grid.display_offset(),
            screen: grid.screen_lines(),
        }
    }

    /// Starts a selection at a viewport cell.
    pub fn select(&self, kind: SelectionKind, at: CellPosition, side: Side) {
        let mut term = self.term.lock();
        let point = grid_point(term.grid().display_offset(), at);
        if term.mode().contains(TermMode::VI) {
            term.vi_mode_cursor.point = point;
        }
        let kind = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
        };
        term.selection = Some(Selection::new(kind, point, grid_side(side)));
    }

    /// Moves the end of the current selection to a viewport cell.
    pub fn extend_selection(&self, to: CellPosition, side: Side) {
        let mut term = self.term.lock();
        let point = grid_point(term.grid().display_offset(), to);
        if term.mode().contains(TermMode::VI) {
            term.vi_mode_cursor.point = point;
        }
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, grid_side(side));
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// The selected text, or `None` when nothing (or only empty space) is selected.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .lock()
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    /// The cursor's viewport cell, or `None` while it is scrolled out of view.
    pub fn cursor_position(&self) -> Option<CellPosition> {
        let term = self.term.lock();
        let point = term.grid().cursor.point;
        point_to_viewport(term.grid().display_offset(), point).map(|p| CellPosition {
            column: p.column.0,
            line: p.line,
        })
    }

    /// The shell's current directory, so new tabs can open where the user is working.
    pub fn working_directory(&self) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.child_pid)).ok()
    }

    /// The program in the foreground, or `None` while the shell itself is waiting for input.
    pub fn foreground_process(&self) -> Option<ForegroundProcess> {
        let group = nix::unistd::tcgetpgrp(&self.pty).ok()?;
        if u32::try_from(group.as_raw()).ok()? == self.child_pid {
            return None;
        }
        let name = std::fs::read_to_string(format!("/proc/{group}/comm")).ok()?;
        Some(ForegroundProcess {
            pid: group.as_raw(),
            name: name.trim_end().to_owned(),
        })
    }

    /// Whether the application asked for a blinking cursor (DECSCUSR).
    pub fn cursor_blinks(&self) -> bool {
        self.term.lock().cursor_style().blinking
    }

    /// The link under a viewport cell: an OSC 8 hyperlink, or else a URL in the row's text.
    pub fn link_at(&self, at: CellPosition) -> Option<String> {
        let term = self.term.lock();
        let columns = term.columns();
        if at.column >= columns || at.line >= term.screen_lines() {
            return None;
        }
        let point = grid_point(term.grid().display_offset(), at);
        let row = &term.grid()[point.line];
        if let Some(link) = row[point.column].hyperlink() {
            return Some(link.uri().to_owned());
        }
        let chars: Vec<char> = (0..columns).map(|column| row[Column(column)].c).collect();
        links::url_at(&chars, at.column)
    }

    /// Searches the output and scrollback for `query` as plain text, newest match first. The match
    /// becomes the selection and is scrolled into view. Lowercase queries ignore case.
    ///
    /// In vi mode the match moves the vi cursor instead, searching from where the search began.
    pub fn search(&mut self, query: &str) -> bool {
        if self.search_match.take().is_some() {
            self.term.lock().selection = None;
        }
        self.search = None;
        if query.is_empty() {
            if let Some(origin) = self.search_origin.filter(|_| self.vi_mode()) {
                self.term.lock().vi_goto_point(origin);
            }
            return false;
        }
        let Ok(regex) = RegexSearch::new(&escape_regex(query)) else {
            return false;
        };
        self.search = Some(regex);
        if self.vi_mode() {
            let origin = self.search_origin.unwrap_or_else(|| self.vi_cursor());
            return self.find_from(origin, self.vi_direction);
        }
        let origin = {
            let term = self.term.lock();
            let bottom = i32::try_from(term.screen_lines()).unwrap_or(i32::MAX) - 1;
            Point::new(Line(bottom), term.last_column())
        };
        self.find(origin, Direction::Left, GridSide::Right)
    }

    /// Moves to the next match of the current search, wrapping around at the ends.
    pub fn search_next(&mut self, direction: SearchDirection) -> bool {
        if self.vi_mode() && self.search.is_some() {
            return self.find_from(self.vi_cursor(), direction);
        }
        let Some(current) = self.search_match.clone() else {
            return false;
        };
        let origin = {
            let term = self.term.lock();
            match direction {
                SearchDirection::Older => current.start().sub(&*term, Boundary::None, 1),
                SearchDirection::Newer => current.end().add(&*term, Boundary::None, 1),
            }
        };
        match direction {
            SearchDirection::Older => self.find(origin, Direction::Left, GridSide::Right),
            SearchDirection::Newer => self.find(origin, Direction::Right, GridSide::Left),
        }
    }

    /// Ends the search. In vi mode the query is kept, so `n` and `N` can repeat it.
    pub fn clear_search(&mut self) {
        if self.search_match.take().is_some() {
            self.term.lock().selection = None;
        }
        self.search_origin = None;
        if !self.vi_mode() {
            self.search = None;
        }
    }

    /// Searches from just past `point`, towards older or newer output.
    fn find_from(&mut self, point: Point, direction: SearchDirection) -> bool {
        let origin = {
            let term = self.term.lock();
            match direction {
                SearchDirection::Older => point.sub(&*term, Boundary::None, 1),
                SearchDirection::Newer => point.add(&*term, Boundary::None, 1),
            }
        };
        match direction {
            SearchDirection::Older => self.find(origin, Direction::Left, GridSide::Right),
            SearchDirection::Newer => self.find(origin, Direction::Right, GridSide::Left),
        }
    }

    fn find(&mut self, origin: Point, direction: Direction, side: GridSide) -> bool {
        let Some(regex) = self.search.as_mut() else {
            return false;
        };
        let mut term = self.term.lock();
        let Some(found) = term.search_next(regex, origin, direction, side, None) else {
            return false;
        };
        if term.mode().contains(TermMode::VI) {
            term.vi_goto_point(*found.start());
            return true;
        }
        term.scroll_to_point(*found.start());
        let mut selection = Selection::new(SelectionType::Simple, *found.start(), GridSide::Left);
        selection.update(*found.end(), GridSide::Right);
        term.selection = Some(selection);
        self.search_match = Some(found);
        true
    }

    pub fn vi_mode(&self) -> bool {
        self.mode().contains(TermMode::VI)
    }

    /// Turns vi mode on, with its cursor where the terminal cursor is (or at the top of the view
    /// when that is scrolled away), or off. Either way the selection goes; leaving also ends the
    /// search.
    pub fn toggle_vi_mode(&mut self) {
        if self.vi_mode() {
            self.search = None;
            self.search_match = None;
            self.search_origin = None;
        }
        let mut term = self.term.lock();
        term.selection = None;
        term.toggle_vi_mode();
    }

    pub fn vi_motion(&self, motion: ViMotion) {
        self.term.lock().vi_motion(motion);
    }

    /// Starts a selection of `kind` at the vi cursor, switches the current one to `kind`, or
    /// clears it when it already is one.
    pub fn vi_select(&self, kind: ViSelection) {
        let mut term = self.term.lock();
        let ty = selection_type(kind);
        let current = term
            .selection
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| s.ty);
        match current {
            Some(current) if current == ty => term.selection = None,
            Some(_) => {
                if let Some(selection) = term.selection.as_mut() {
                    selection.ty = ty;
                }
            }
            None => {
                let mut selection = Selection::new(ty, term.vi_mode_cursor.point, GridSide::Left);
                selection.include_all();
                term.selection = Some(selection);
            }
        }
    }

    /// The kind of the vi mode selection, or `None` when nothing is selected.
    pub fn vi_selection(&self) -> Option<ViSelection> {
        let term = self.term.lock();
        let selection = term.selection.as_ref().filter(|s| !s.is_empty())?;
        Some(match selection.ty {
            SelectionType::Simple => ViSelection::Simple,
            SelectionType::Lines => ViSelection::Line,
            SelectionType::Block => ViSelection::Block,
            SelectionType::Semantic => ViSelection::Semantic,
        })
    }

    /// Scrolls by half or whole pages; the vi cursor moves the same number of lines.
    pub fn vi_scroll(&self, scroll: ViScroll) {
        let mut term = self.term.lock();
        let screen = i32::try_from(term.screen_lines()).unwrap_or(i32::MAX);
        let lines = match scroll {
            ViScroll::HalfPageUp => screen / 2,
            ViScroll::HalfPageDown => -(screen / 2),
            ViScroll::PageUp => screen,
            ViScroll::PageDown => -screen,
        };
        let cursor = term.vi_mode_cursor.scroll(&*term, lines);
        term.vi_mode_cursor = cursor;
        term.scroll_display(Scroll::Delta(lines));
    }

    /// Moves the vi cursor to the oldest line of the scrollback.
    pub fn vi_goto_top(&self) {
        let mut term = self.term.lock();
        let point = Point::new(term.topmost_line(), Column(0));
        term.vi_goto_point(point);
    }

    /// Moves the vi cursor to the line the terminal cursor is on: the newest output.
    pub fn vi_goto_bottom(&self) {
        let mut term = self.term.lock();
        let point = Point::new(term.grid().cursor.point.line, Column(0));
        term.vi_goto_point(point);
    }

    /// Opens a vi mode search towards `direction`; it starts at the vi cursor.
    pub fn start_vi_search(&mut self, direction: SearchDirection) {
        self.vi_direction = direction;
        self.search_origin = Some(self.vi_cursor());
    }

    /// Repeats the last vi mode search, the same way or the other way. False without a match.
    pub fn vi_search_next(&mut self, reverse: bool) -> bool {
        if self.search.is_none() {
            return false;
        }
        let direction = match (self.vi_direction, reverse) {
            (direction, false) => direction,
            (SearchDirection::Older, true) => SearchDirection::Newer,
            (SearchDirection::Newer, true) => SearchDirection::Older,
        };
        self.find_from(self.vi_cursor(), direction)
    }

    fn vi_cursor(&self) -> Point {
        self.term.lock().vi_mode_cursor.point
    }

    /// Whether the shell runs a command, as its OSC 133 marks tell, or `None` when the shell
    /// sends no marks.
    pub fn command_running(&self) -> Option<bool> {
        lock(&self.marks).command_running()
    }

    /// Scrolls the previous or next prompt to the top of the viewport. Past the newest prompt it
    /// returns to the live end. False when there is no prompt to go to.
    pub fn jump_to_prompt(&self, direction: SearchDirection) -> bool {
        let mut term = self.term.lock();
        let marks = lock(&self.marks);
        let top = Line(-i32::try_from(term.grid().display_offset()).unwrap_or(i32::MAX));
        let target = match direction {
            SearchDirection::Older => marks.previous_prompt(top),
            SearchDirection::Newer => marks.next_prompt(top),
        };
        let offset = match target {
            Some(line) => -line.0.min(0),
            None if direction == SearchDirection::Newer && top.0 < 0 => 0,
            None => return false,
        };
        term.scroll_display(Scroll::Delta(offset.saturating_add(top.0)));
        true
    }

    /// Selects a command's output: the one shown at `at`, or else the newest. False when there is
    /// none.
    pub fn select_command_output(&self, at: Option<CellPosition>) -> bool {
        let mut term = self.term.lock();
        let marks = lock(&self.marks);
        let cursor = term.grid().cursor.point;
        let last_column = term.last_column();
        let output = match at {
            Some(at) => {
                let line = grid_point(term.grid().display_offset(), at).line;
                marks.output_at(line, cursor, last_column)
            }
            None => marks.last_output(cursor, last_column),
        };
        let Some((start, end)) = output else {
            return false;
        };
        let mut selection = Selection::new(SelectionType::Simple, start, GridSide::Left);
        selection.update(end, GridSide::Right);
        term.selection = Some(selection);
        if at.is_none() {
            term.scroll_to_point(start);
        }
        true
    }

    /// Whether there is a command's output to select: the one shown at `at`, or else any.
    pub fn has_command_output(&self, at: Option<CellPosition>) -> bool {
        let term = self.term.lock();
        let marks = lock(&self.marks);
        let cursor = term.grid().cursor.point;
        let last_column = term.last_column();
        match at {
            Some(at) => {
                let line = grid_point(term.grid().display_offset(), at).line;
                marks.output_at(line, cursor, last_column).is_some()
            }
            None => marks.last_output(cursor, last_column).is_some(),
        }
    }

    /// Sends pasted text, using bracketed paste when the application asked for it.
    pub fn paste(&self, text: &str) {
        let bracketed = self.mode().contains(TermMode::BRACKETED_PASTE);
        self.scroll_to_bottom();
        self.write(paste_bytes(text, bracketed));
    }

    /// Applies an emulator event and tells the UI how to react.
    pub fn handle_event(&mut self, event: Event, palette: &Palette) -> Option<Notice> {
        match event {
            Event::Wakeup => Some(Notice::Output),
            Event::CursorBlinkingChange | Event::MouseCursorDirty => Some(Notice::Redraw),
            Event::Title(title) => {
                self.title = Some(title);
                Some(Notice::TitleChanged)
            }
            Event::ResetTitle => {
                self.title = None;
                Some(Notice::TitleChanged)
            }
            Event::PtyWrite(text) => {
                self.write(text.into_bytes());
                None
            }
            Event::TextAreaSizeRequest(format) => {
                self.write(format(self.size.window_size(self.cell)).into_bytes());
                None
            }
            Event::ColorRequest(index, format) => {
                let overridden = self.term.lock().colors()[index];
                let color = overridden.unwrap_or_else(|| palette.indexed(index));
                self.write(format(color).into_bytes());
                None
            }
            Event::Bell => Some(Notice::Bell),
            Event::ChildExit(status) => Some(Notice::Exited(status.code())),
            Event::Exit => Some(Notice::Exited(None)),
            Event::ClipboardStore(kind, text) => Some(Notice::Copy {
                text,
                primary: kind == ClipboardType::Selection,
            }),
            Event::ClipboardLoad(..) => {
                tracing::debug!("ignoring an OSC 52 clipboard read request");
                None
            }
        }
    }

    fn send(&self, msg: Msg) {
        if let Err(error) = self.sender.send(msg) {
            tracing::warn!(%error, "could not reach the terminal I/O loop");
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.send(Msg::Shutdown);
    }
}

fn lock(marks: &Mutex<PromptMarks>) -> MutexGuard<'_, PromptMarks> {
    marks.lock().unwrap_or_else(PoisonError::into_inner)
}

#[allow(clippy::cast_possible_wrap)]
fn grid_point(display_offset: usize, at: CellPosition) -> Point {
    viewport_to_point(display_offset, Point::new(at.line, Column(at.column)))
}

/// The emulator settings; the vi mode cursor is always a steady block.
fn term_config(scrollback_lines: usize, kitty_keyboard: bool, cursor_style: CursorStyle) -> Config {
    Config {
        scrolling_history: scrollback_lines,
        kitty_keyboard,
        default_cursor_style: cursor_style,
        vi_mode_cursor_style: Some(CursorStyle {
            shape: CursorShape::Block,
            blinking: false,
        }),
        ..Config::default()
    }
}

fn selection_type(kind: ViSelection) -> SelectionType {
    match kind {
        ViSelection::Simple => SelectionType::Simple,
        ViSelection::Line => SelectionType::Lines,
        ViSelection::Block => SelectionType::Block,
        ViSelection::Semantic => SelectionType::Semantic,
    }
}

fn grid_side(side: Side) -> GridSide {
    match side {
        Side::Left => GridSide::Left,
        Side::Right => GridSide::Right,
    }
}

/// Escapes regex syntax so a search query matches literally.
fn escape_regex(query: &str) -> String {
    query
        .chars()
        .flat_map(|c| {
            let special = "\\.+*?()|[]{}^$#&-~".contains(c);
            special
                .then_some('\\')
                .into_iter()
                .chain(std::iter::once(c))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_sees_a_widely_known_terminal_type() {
        let env = child_env(HashMap::new());
        assert_eq!(env["TERM"], "xterm-256color");
        assert_eq!(env["COLORTERM"], "truecolor");
    }

    #[test]
    fn the_users_env_overrides_the_terminal_type() {
        let user = HashMap::from([("TERM".to_owned(), "alacritty".to_owned())]);
        assert_eq!(child_env(user)["TERM"], "alacritty");
    }

    /// A terminal showing `text`, once the shell has printed it.
    fn terminal_showing(text: &str) -> Terminal {
        let options = SpawnOptions {
            command: Some((
                "sh".into(),
                vec!["-c".into(), format!("printf '{text}'; sleep 5")],
            )),
            size: GridSize {
                columns: 40,
                lines: 5,
            },
            ..SpawnOptions::default()
        };
        let terminal = Terminal::spawn(options).expect("spawn sh").0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while terminal.term.lock().grid()[Line(0)][Column(0)].c != text.chars().next().unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "the shell printed nothing"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        terminal
    }

    #[test]
    fn vi_mode_selects_with_motions() {
        let mut terminal = terminal_showing("hello world");
        terminal.toggle_vi_mode();
        assert!(terminal.vi_mode());
        terminal.vi_motion(ViMotion::First);
        terminal.vi_select(ViSelection::Simple);
        assert_eq!(terminal.vi_selection(), Some(ViSelection::Simple));
        terminal.vi_motion(ViMotion::SemanticRightEnd);
        assert_eq!(terminal.selection_text().as_deref(), Some("hello"));

        terminal.vi_select(ViSelection::Line);
        assert_eq!(terminal.vi_selection(), Some(ViSelection::Line));
        terminal.vi_select(ViSelection::Line);
        assert_eq!(terminal.vi_selection(), None);

        terminal.toggle_vi_mode();
        assert!(!terminal.vi_mode());
    }

    #[test]
    fn vi_search_moves_the_cursor_and_keeps_the_query() {
        let mut terminal = terminal_showing("one two one");
        terminal.toggle_vi_mode();
        terminal.vi_motion(ViMotion::First);
        terminal.start_vi_search(SearchDirection::Newer);
        assert!(terminal.search("one"));
        assert_eq!(terminal.vi_cursor(), Point::new(Line(0), Column(8)));
        assert_eq!(terminal.vi_selection(), None);

        terminal.clear_search();
        assert!(terminal.vi_search_next(false));
        assert_eq!(terminal.vi_cursor(), Point::new(Line(0), Column(0)));
    }
}

//! Keeps multi-codepoint emoji in one cell.
//!
//! alacritty gives every character of nonzero width its own cell, so a skin tone modifier
//! (`👋🏽`) or the next person in a ZWJ sequence (`👩‍💻`) lands in a cell of its own and the
//! emoji is drawn in pieces. [`Clustered`] wraps the terminal while the parser runs and folds
//! those characters into the cell of the emoji they belong to, as zero-width characters, so
//! the whole sequence is drawn as one glyph in the emoji's cells.
//!
//! In grapheme mode (DEC private mode 2027) it also widens an emoji that is a narrow symbol
//! made an emoji by U+FE0F or a skin tone, like ❤️, to the two cells Unicode gives it.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use alacritty_terminal::Term;
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Column;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, CursorShape, CursorStyle, Handler, Hyperlink, KeyboardModes,
    KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, PrivateMode, Processor, Rgb,
    ScpCharPath, ScpUpdateMode, StandardCharset, StdSyncHandler, TabulationClearMode,
};

/// Zero width joiner: glues the emoji on either side into one.
const ZWJ: char = '\u{200D}';
/// Variation selector 16: asks for the emoji presentation of the character before it.
const EMOJI_PRESENTATION: char = '\u{FE0F}';
/// Grapheme cluster mode, as proposed for Contour and supported by Ghostty and others.
const GRAPHEME_MODE: u16 = 2027;

/// Whether grapheme mode (DEC private mode 2027) is on. Programs that count emoji widths by
/// grapheme turn it on; the configuration sets what it is until they do.
#[derive(Debug, Default)]
pub struct GraphemeMode {
    default: AtomicBool,
    /// What the program asked for: [`UNSET`], [`ON`] or [`OFF`].
    requested: AtomicU8,
}

const UNSET: u8 = 0;
const ON: u8 = 1;
const OFF: u8 = 2;

impl GraphemeMode {
    pub fn new(default: bool) -> Self {
        Self {
            default: AtomicBool::new(default),
            requested: AtomicU8::new(UNSET),
        }
    }

    pub fn set_default(&self, on: bool) {
        self.default.store(on, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        match self.requested.load(Ordering::Relaxed) {
            ON => true,
            OFF => false,
            _ => self.default.load(Ordering::Relaxed),
        }
    }

    /// Forgets what the program asked for, as a terminal reset does.
    pub fn reset(&self) {
        self.requested.store(UNSET, Ordering::Relaxed);
    }

    fn request(&self, on: bool) {
        self.requested
            .store(if on { ON } else { OFF }, Ordering::Relaxed);
    }
}

/// What the parser needs besides the terminal: the grapheme mode, and where replies to the
/// program's queries about it go.
#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub graphemes: &'a GraphemeMode,
    pub reply: &'a dyn Fn(String),
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("graphemes", &self.graphemes)
            .finish_non_exhaustive()
    }
}

/// Feeds `bytes` to `term` through `parser`, keeping emoji sequences in one cell. Grapheme mode
/// is off unless the bytes turn it on, and queries about it go unanswered.
pub(crate) fn advance<T: EventListener>(
    parser: &mut Processor<StdSyncHandler>,
    term: &mut Term<T>,
    bytes: &[u8],
) {
    let graphemes = GraphemeMode::default();
    let context = Context {
        graphemes: &graphemes,
        reply: &|_| {},
    };
    advance_with(parser, term, context, bytes);
}

/// Feeds `bytes` to `term` through `parser`, keeping emoji sequences in one cell.
pub(crate) fn advance_with<T: EventListener>(
    parser: &mut Processor<StdSyncHandler>,
    term: &mut Term<T>,
    context: Context<'_>,
    bytes: &[u8],
) {
    parser.advance(&mut Clustered { term, context }, bytes);
}

/// Applies a pending synchronized update to `term`, keeping emoji sequences in one cell.
pub(crate) fn stop_sync<T: EventListener>(
    parser: &mut Processor<StdSyncHandler>,
    term: &mut Term<T>,
    context: Context<'_>,
) {
    parser.stop_sync(&mut Clustered { term, context });
}

/// Whether `c` is an Emoji Modifier: one of the five Fitzpatrick skin tones.
pub(crate) fn is_skin_tone(c: char) -> bool {
    matches!(u32::from(c), 0x1F3FB..=0x1F3FF)
}

/// Whether `c` is a pictograph that can take a skin tone or be joined to another by a ZWJ.
/// A close approximation of Unicode's `Extended_Pictographic` property.
fn is_pictographic(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00A9
            | 0x00AE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2194..=0x21AA
            | 0x231A..=0x23FF
            | 0x24C2
            | 0x25AA..=0x25FE
            | 0x2600..=0x27BF
            | 0x2934..=0x2935
            | 0x2B05..=0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F000..=0x1F1E5
            | 0x1F200..=0x1F3FA
            | 0x1F400..=0x1FAFF
            | 0x1FC00..=0x1FFFD
    )
}

/// Whether `c`, written right after `cell`, continues the emoji in it.
fn continues(cell: &Cell, c: char) -> bool {
    let zerowidth = cell.zerowidth().unwrap_or_default();
    // Variation selectors don't change which character a skin tone or ZWJ follows.
    let last = zerowidth
        .iter()
        .rev()
        .copied()
        .find(|&z| !matches!(z, '\u{FE0E}' | '\u{FE0F}'))
        .unwrap_or(cell.c);
    if is_skin_tone(c) {
        return is_pictographic(last);
    }
    last == ZWJ && is_pictographic(c) && is_pictographic(cell.c)
}

/// Whether `cell` holds a one-cell emoji that grapheme mode makes two cells wide: a symbol, or
/// a keycap digit, followed by U+FE0F or a skin tone.
fn widens(cell: &Cell) -> bool {
    let presentation = cell
        .zerowidth()
        .and_then(<[char]>::first)
        .is_some_and(|&z| z == EMOJI_PRESENTATION || is_skin_tone(z));
    presentation
        && !cell
            .flags
            .intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
        && (is_pictographic(cell.c) || matches!(cell.c, '0'..='9' | '#' | '*'))
}

/// The terminal, folding the characters that continue an emoji into its cell.
struct Clustered<'a, T> {
    term: &'a mut Term<T>,
    context: Context<'a>,
}

impl<T: EventListener> Clustered<'_, T> {
    /// The cell the last character was written to, if it is on the cursor's line.
    fn previous_cell(&mut self) -> Option<&mut Cell> {
        let grid = self.term.grid_mut();
        let cursor = &grid.cursor;
        let line = cursor.point.line;
        let mut column = cursor.point.column.0;
        if !cursor.input_needs_wrap {
            column = column.checked_sub(1)?;
        }
        if grid[line][Column(column)]
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
        {
            column = column.checked_sub(1)?;
        }
        Some(&mut grid[line][Column(column)])
    }

    /// Makes the one-cell emoji just written two cells wide, moving it to the next line when
    /// it is in the last column. The terminal writes the spacers, so wrapping, insert mode and
    /// overwritten wide characters work as for any wide character.
    fn widen_previous(&mut self) {
        if !self.previous_cell().is_some_and(|cell| widens(cell)) {
            return;
        }
        let grid = self.term.grid();
        if grid.cursor.input_needs_wrap {
            if !self.term.mode().contains(TermMode::LINE_WRAP) || grid.columns() < 2 {
                return;
            }
            let line = grid.cursor.point.line;
            let column = grid.cursor.point.column;
            let emoji = grid[line][column].clone();
            self.term.grid_mut().cursor.input_needs_wrap = false;
            self.write_spacer(Flags::LEADING_WIDE_CHAR_SPACER);
            self.term.input(' ');
            let point = self.term.grid().cursor.point;
            self.term.grid_mut()[point.line][point.column - 1] = emoji;
        }
        if let Some(cell) = self.previous_cell() {
            cell.flags.insert(Flags::WIDE_CHAR);
        }
        self.write_spacer(Flags::WIDE_CHAR_SPACER);
    }

    /// Writes a blank cell carrying `flag` at the cursor.
    fn write_spacer(&mut self, flag: Flags) {
        self.term.grid_mut().cursor.template.flags.insert(flag);
        self.term.input(' ');
        self.term.grid_mut().cursor.template.flags.remove(flag);
    }
}

/// Forwards each listed [`Handler`] method to the terminal.
macro_rules! forward {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(
            fn $name(&mut self, $($arg: $ty),*) {
                self.term.$name($($arg),*);
            }
        )*
    };
}

impl<T: EventListener> Handler for Clustered<'_, T> {
    fn input(&mut self, c: char) {
        if let Some(cell) = self.previous_cell()
            && continues(cell, c)
        {
            cell.push_zerowidth(c);
        } else {
            self.term.input(c);
        }
        if (c == EMOJI_PRESENTATION || is_skin_tone(c)) && self.context.graphemes.enabled() {
            self.widen_previous();
        }
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Unknown(GRAPHEME_MODE) => self.context.graphemes.request(true),
            mode => self.term.set_private_mode(mode),
        }
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Unknown(GRAPHEME_MODE) => self.context.graphemes.request(false),
            mode => self.term.unset_private_mode(mode),
        }
    }

    fn report_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Unknown(GRAPHEME_MODE) => {
                let state = if self.context.graphemes.enabled() {
                    1
                } else {
                    2
                };
                (self.context.reply)(format!("\x1b[?{GRAPHEME_MODE};{state}$y"));
            }
            mode => self.term.report_private_mode(mode),
        }
    }

    fn reset_state(&mut self) {
        self.context.graphemes.reset();
        self.term.reset_state();
    }

    forward! {
        set_title(title: Option<String>);
        set_cursor_style(style: Option<CursorStyle>);
        set_cursor_shape(shape: CursorShape);
        goto(line: i32, col: usize);
        goto_line(line: i32);
        goto_col(col: usize);
        insert_blank(count: usize);
        move_up(count: usize);
        move_down(count: usize);
        identify_terminal(intermediate: Option<char>);
        device_status(arg: usize);
        move_forward(col: usize);
        move_backward(col: usize);
        move_down_and_cr(row: usize);
        move_up_and_cr(row: usize);
        put_tab(count: u16);
        backspace();
        carriage_return();
        linefeed();
        bell();
        substitute();
        newline();
        set_horizontal_tabstop();
        scroll_up(count: usize);
        scroll_down(count: usize);
        insert_blank_lines(count: usize);
        delete_lines(count: usize);
        erase_chars(count: usize);
        delete_chars(count: usize);
        move_backward_tabs(count: u16);
        move_forward_tabs(count: u16);
        save_cursor_position();
        restore_cursor_position();
        clear_line(mode: LineClearMode);
        clear_screen(mode: ClearMode);
        clear_tabs(mode: TabulationClearMode);
        set_tabs(interval: u16);
        reverse_index();
        terminal_attribute(attr: Attr);
        set_mode(mode: Mode);
        unset_mode(mode: Mode);
        report_mode(mode: Mode);
        set_scrolling_region(top: usize, bottom: Option<usize>);
        set_keypad_application_mode();
        unset_keypad_application_mode();
        set_active_charset(index: CharsetIndex);
        configure_charset(index: CharsetIndex, charset: StandardCharset);
        set_color(index: usize, color: Rgb);
        dynamic_color_sequence(prefix: String, index: usize, terminator: &str);
        reset_color(index: usize);
        clipboard_store(clipboard: u8, base64: &[u8]);
        clipboard_load(clipboard: u8, terminator: &str);
        decaln();
        push_title();
        pop_title();
        text_area_size_pixels();
        text_area_size_chars();
        set_hyperlink(link: Option<Hyperlink>);
        set_mouse_cursor_icon(icon: CursorIcon);
        report_keyboard_mode();
        push_keyboard_mode(mode: KeyboardModes);
        pop_keyboard_modes(to_pop: u16);
        set_keyboard_mode(mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
        set_modify_other_keys(mode: ModifyOtherKeys);
        report_modify_other_keys();
        set_scp(char_path: ScpCharPath, update_mode: ScpUpdateMode);
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::Line;
    use alacritty_terminal::term::Config;

    use super::*;
    use crate::size::GridSize;

    fn term(bytes: &str) -> Term<VoidListener> {
        let size = GridSize {
            columns: 20,
            lines: 3,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        advance(&mut Processor::new(), &mut term, bytes.as_bytes());
        term
    }

    /// The text of each occupied cell on `line`, with its zero-width characters.
    fn cells_on(term: &Term<VoidListener>, line: i32) -> Vec<String> {
        let row = &term.grid()[Line(line)];
        (0..row.len())
            .map(|column| &row[Column(column)])
            .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER) && cell.c != ' ')
            .map(|cell| {
                let mut text = String::from(cell.c);
                text.extend(cell.zerowidth().unwrap_or_default());
                text
            })
            .collect()
    }

    fn cells(term: &Term<VoidListener>) -> Vec<String> {
        cells_on(term, 0)
    }

    #[test]
    fn skin_tone_stays_with_its_emoji() {
        let term = term("👋🏽x");
        assert_eq!(cells(&term), ["👋🏽", "x"]);
        assert_eq!(term.grid().cursor.point.column, Column(3));
    }

    #[test]
    fn every_skin_tone_folds() {
        let term = term("👋🏻👋🏼👋🏽👋🏾👋🏿");
        assert_eq!(cells(&term), ["👋🏻", "👋🏼", "👋🏽", "👋🏾", "👋🏿"]);
        assert_eq!(term.grid().cursor.point.column, Column(10));
    }

    #[test]
    fn narrow_emoji_takes_a_skin_tone() {
        assert_eq!(cells(&term("✌\u{FE0F}🏾")), ["✌\u{FE0F}🏾"]);
    }

    #[test]
    fn zwj_family_is_one_cell() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let term = term(&format!("{family}!"));
        assert_eq!(cells(&term), [family, "!"]);
        assert_eq!(term.grid().cursor.point.column, Column(3));
    }

    #[test]
    fn zwj_sequence_with_skin_tones_is_one_cell() {
        let couple = "👩🏽\u{200D}🤝\u{200D}👨🏿";
        assert_eq!(cells(&term(couple)), [couple]);
    }

    #[test]
    fn emoji_without_joiner_stay_apart() {
        assert_eq!(cells(&term("👨👩")), ["👨", "👩"]);
    }

    #[test]
    fn skin_tone_after_text_stays_apart() {
        assert_eq!(cells(&term("a🏽")), ["a", "🏽"]);
    }

    #[test]
    fn joiner_between_letters_does_not_glue_emoji() {
        assert_eq!(cells(&term("a\u{200D}👩")), ["a\u{200D}", "👩"]);
    }

    #[test]
    fn skin_tone_at_line_start_stays_apart() {
        let term = term("👋\r\n🏽");
        assert_eq!(cells_on(&term, 0), ["👋"]);
        assert_eq!(cells_on(&term, 1), ["🏽"]);
    }

    #[test]
    fn skin_tone_folds_into_emoji_at_end_of_line() {
        let term = term("abcdefghijklmnopqr👋🏽");
        assert_eq!(cells(&term).last().map(String::as_str), Some("👋🏽"));
        assert!(term.grid().cursor.input_needs_wrap);
    }

    /// `bytes` fed with grapheme mode `wide` by default, plus the replies to the program.
    fn term_in(bytes: &str, wide: bool) -> (Term<VoidListener>, Vec<String>) {
        let replies = std::cell::RefCell::new(Vec::new());
        let reply = |text: String| replies.borrow_mut().push(text);
        let graphemes = GraphemeMode::new(wide);
        let size = GridSize {
            columns: 20,
            lines: 3,
        };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let context = Context {
            graphemes: &graphemes,
            reply: &reply,
        };
        advance_with(&mut Processor::new(), &mut term, context, bytes.as_bytes());
        (term, replies.into_inner())
    }

    fn flags_at(term: &Term<VoidListener>, line: i32, column: usize) -> Flags {
        term.grid()[Line(line)][Column(column)].flags
    }

    #[test]
    fn emoji_presentation_of_a_narrow_symbol_keeps_one_cell_by_default() {
        let term = term("\u{2764}\u{FE0F}x");
        assert_eq!(cells(&term), ["\u{2764}\u{FE0F}", "x"]);
        assert_eq!(term.grid().cursor.point.column, Column(2));
    }

    #[test]
    fn grapheme_mode_widens_emoji_presentation_to_two_cells() {
        let term = term("\x1b[?2027h\u{2764}\u{FE0F}x");
        assert_eq!(cells(&term), ["\u{2764}\u{FE0F}", "x"]);
        assert!(flags_at(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags_at(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(term.grid().cursor.point.column, Column(3));
    }

    #[test]
    fn grapheme_mode_widens_skin_toned_symbols_and_keycaps() {
        let term = term("\x1b[?2027h\u{270C}\u{1F3FE}1\u{FE0F}\u{20E3}");
        assert_eq!(cells(&term), ["\u{270C}\u{1F3FE}", "1\u{FE0F}\u{20E3}"]);
        assert_eq!(term.grid().cursor.point.column, Column(4));
    }

    #[test]
    fn grapheme_mode_leaves_text_presentation_narrow() {
        let term = term("\x1b[?2027h\u{2764}x\u{2764}\u{FE0E}y");
        assert_eq!(term.grid().cursor.point.column, Column(4));
    }

    #[test]
    fn widened_emoji_in_the_last_column_moves_to_the_next_line() {
        let term = term(&format!("\x1b[?2027h{}\u{2764}\u{FE0F}x", "a".repeat(19)));
        assert!(flags_at(&term, 0, 19).contains(Flags::LEADING_WIDE_CHAR_SPACER));
        assert!(flags_at(&term, 0, 19).contains(Flags::WRAPLINE));
        assert_eq!(cells_on(&term, 1), ["\u{2764}\u{FE0F}", "x"]);
        assert!(flags_at(&term, 1, 0).contains(Flags::WIDE_CHAR));
        assert_eq!(term.grid().cursor.point.column, Column(3));
    }

    #[test]
    fn widened_emoji_fills_the_last_two_columns() {
        let term = term(&format!("\x1b[?2027h{}\u{2764}\u{FE0F}", "a".repeat(18)));
        assert!(flags_at(&term, 0, 18).contains(Flags::WIDE_CHAR));
        assert!(flags_at(&term, 0, 19).contains(Flags::WIDE_CHAR_SPACER));
        assert!(term.grid().cursor.input_needs_wrap);
    }

    #[test]
    fn the_configured_default_applies_until_the_program_switches() {
        let (term, _) = term_in("\u{2764}\u{FE0F}", true);
        assert_eq!(term.grid().cursor.point.column, Column(2));
        let (term, _) = term_in("\x1b[?2027l\u{2764}\u{FE0F}", true);
        assert_eq!(term.grid().cursor.point.column, Column(1));
    }

    #[test]
    fn grapheme_mode_is_reported() {
        let (_, replies) = term_in("\x1b[?2027$p\x1b[?2027h\x1b[?2027$p", false);
        assert_eq!(replies, ["\x1b[?2027;2$y", "\x1b[?2027;1$y"]);
    }

    #[test]
    fn a_reset_returns_to_the_configured_default() {
        let (_, replies) = term_in("\x1b[?2027l\x1bc\x1b[?2027$p", true);
        assert_eq!(replies, ["\x1b[?2027;1$y"]);
    }
}

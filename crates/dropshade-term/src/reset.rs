//! Resetting the emulator. Escape sequences written to the pty reach the shell, not the emulator,
//! so these act on the emulator state directly.

use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, Handler, Mode, NamedMode, NamedPrivateMode, PrivateMode,
    StandardCharset,
};

use crate::terminal::EventProxy;

const CLEARED_MODES: [NamedPrivateMode; 11] = [
    NamedPrivateMode::CursorKeys,
    NamedPrivateMode::Origin,
    NamedPrivateMode::ReportMouseClicks,
    NamedPrivateMode::ReportCellMouseMotion,
    NamedPrivateMode::ReportAllMouseMotion,
    NamedPrivateMode::ReportFocusInOut,
    NamedPrivateMode::Utf8Mouse,
    NamedPrivateMode::SgrMouse,
    NamedPrivateMode::AlternateScroll,
    NamedPrivateMode::UrgencyHints,
    NamedPrivateMode::BracketedPaste,
];

const RESTORED_MODES: [NamedPrivateMode; 2] =
    [NamedPrivateMode::LineWrap, NamedPrivateMode::ShowCursor];

/// Undoes what a program can leave behind after it crashed or was killed: the alternate screen,
/// mouse reporting, bracketed paste, line-drawing charsets, a hidden cursor. The screen is
/// cleared into the scrollback, which stays.
pub fn soft(term: &mut Term<EventProxy>) {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        term.unset_private_mode(PrivateMode::Named(
            NamedPrivateMode::SwapScreenAndSetRestoreCursor,
        ));
    }
    for mode in CLEARED_MODES {
        term.unset_private_mode(PrivateMode::Named(mode));
    }
    for mode in RESTORED_MODES {
        term.set_private_mode(PrivateMode::Named(mode));
    }
    term.unset_mode(Mode::Named(NamedMode::Insert));
    term.unset_mode(Mode::Named(NamedMode::LineFeedNewLine));
    for index in [
        CharsetIndex::G0,
        CharsetIndex::G1,
        CharsetIndex::G2,
        CharsetIndex::G3,
    ] {
        term.configure_charset(index, StandardCharset::Ascii);
    }
    term.set_active_charset(CharsetIndex::G0);
    term.terminal_attribute(Attr::Reset);
    term.set_cursor_style(None);
    term.set_scrolling_region(1, None);
    term.unset_keypad_application_mode();
    term.pop_keyboard_modes(u16::MAX);
    term.clear_screen(ClearMode::All);
    term.goto(0, 0);
}

/// A full reset (RIS): all of [`soft`], plus a blank screen and no scrollback.
pub fn full(term: &mut Term<EventProxy>) {
    term.reset_state();
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    use iced::futures::channel::mpsc;

    use super::*;
    use crate::size::GridSize;

    fn term() -> Term<EventProxy> {
        let (sender, _receiver) = mpsc::unbounded();
        let size = GridSize {
            columns: 20,
            lines: 5,
        };
        Term::new(Config::default(), &size, EventProxy::new(sender))
    }

    fn feed(term: &mut Term<EventProxy>, bytes: &[u8]) {
        Processor::<StdSyncHandler>::default().advance(term, bytes);
    }

    fn first_cell(term: &Term<EventProxy>) -> char {
        term.grid()[Line(0)][Column(0)].c
    }

    #[test]
    fn soft_reset_turns_off_modes_a_program_left_on() {
        let mut term = term();
        feed(
            &mut term,
            b"\x1b[?1000h\x1b[?1006h\x1b[?2004h\x1b[?1h\x1b[?25l\x1b[?7l",
        );
        let left_on = TermMode::MOUSE_REPORT_CLICK
            | TermMode::SGR_MOUSE
            | TermMode::BRACKETED_PASTE
            | TermMode::APP_CURSOR;
        assert!(term.mode().contains(left_on));
        assert!(!term.mode().contains(TermMode::SHOW_CURSOR));

        soft(&mut term);

        assert!(!term.mode().intersects(left_on));
        assert!(
            term.mode()
                .contains(TermMode::SHOW_CURSOR | TermMode::LINE_WRAP)
        );
    }

    #[test]
    fn soft_reset_leaves_the_alternate_screen() {
        let mut term = term();
        feed(&mut term, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));

        soft(&mut term);

        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn soft_reset_restores_the_ascii_charset() {
        let mut term = term();
        feed(&mut term, b"\x1b(0");
        soft(&mut term);
        feed(&mut term, b"q");
        assert_eq!(first_cell(&term), 'q');
    }

    #[test]
    fn soft_reset_moves_the_screen_into_the_scrollback() {
        let mut term = term();
        feed(&mut term, b"first\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        let history = term.grid().history_size();
        assert!(history > 0);

        soft(&mut term);

        assert!(term.grid().history_size() > history);
        assert_eq!(first_cell(&term), ' ');
    }

    #[test]
    fn soft_reset_puts_the_cursor_at_the_top_left() {
        let mut term = term();
        feed(&mut term, b"one\r\ntwo\r\nthree");

        soft(&mut term);

        let cursor = term.grid().cursor.point;
        assert_eq!((cursor.line, cursor.column), (Line(0), Column(0)));
    }

    #[test]
    fn full_reset_clears_the_screen_and_the_scrollback() {
        let mut term = term();
        feed(&mut term, b"first\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        assert!(term.grid().history_size() > 0);

        full(&mut term);

        assert_eq!(term.grid().history_size(), 0);
        assert_eq!(first_cell(&term), ' ');
    }
}

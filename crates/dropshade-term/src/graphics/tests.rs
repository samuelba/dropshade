use std::sync::{Arc, Mutex};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::placeholder::{self, PLACEHOLDER};
use super::table::{ImageFit, PlacementTable};
use super::{Graphics, Prepared, lock};
use crate::size::GridSize;

const CELL_WIDTH: u16 = 10;
const CELL_HEIGHT: u16 = 20;

struct Screen {
    term: Term<VoidListener>,
    parser: Processor<StdSyncHandler>,
    graphics: Graphics,
    table: Arc<Mutex<PlacementTable>>,
}

impl Screen {
    fn new() -> Self {
        let size = GridSize {
            columns: 10,
            lines: 5,
        };
        let table = Arc::new(Mutex::new(PlacementTable::default()));
        Self {
            term: Term::new(Config::default(), &size, VoidListener),
            parser: Processor::new(),
            graphics: Graphics::new(Arc::clone(&table), CELL_WIDTH, CELL_HEIGHT),
            table,
        }
    }

    fn print(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    /// Runs a kitty command; returns its reply.
    fn kitty(&mut self, control: &str, payload: &[u8]) -> Option<String> {
        let body = format!("{control};{}", STANDARD.encode(payload));
        let Prepared { op, reply } = self.graphics.prepare_kitty(body.as_bytes());
        if let Some(op) = op {
            let parser = &mut self.parser;
            self.graphics.apply(&mut self.term, op, |term, bytes| {
                parser.advance(term, bytes);
            });
        }
        reply.map(|reply| String::from_utf8(reply).unwrap())
    }

    /// Places a `width` × `height` pixel RGBA image.
    fn image(&mut self, control: &str, width: u32, height: u32) -> Option<String> {
        let pixels = vec![0x80; usize::try_from(width * height * 4).unwrap()];
        self.kitty(&format!("{control},s={width},v={height}"), &pixels)
    }

    /// `(row, column)` of the placeholder at a screen cell.
    fn tile(&self, line: i32, column: usize) -> Option<(usize, usize)> {
        let parsed = placeholder::parse(&self.term.grid()[Line(line)][Column(column)])?;
        Some((parsed.row?, parsed.column?))
    }

    fn cursor(&self) -> Point {
        self.term.grid().cursor.point
    }
}

#[test]
fn an_image_covers_its_cells_and_the_cursor_ends_right_of_it() {
    let mut screen = Screen::new();
    screen.image("a=T", 20, 40);
    assert_eq!(screen.tile(0, 0), Some((0, 0)));
    assert_eq!(screen.tile(0, 1), Some((0, 1)));
    assert_eq!(screen.tile(1, 0), Some((1, 0)));
    assert_eq!(screen.tile(1, 1), Some((1, 1)));
    assert_eq!(screen.tile(0, 2), None);
    assert_eq!(screen.tile(2, 0), None);
    assert_eq!(screen.cursor(), Point::new(Line(1), Column(2)));
}

#[test]
fn a_partly_filled_cell_still_counts() {
    let mut screen = Screen::new();
    screen.image("a=T", 21, 1);
    assert_eq!(screen.tile(0, 2), Some((0, 2)));
    assert_eq!(screen.tile(1, 0), None);
}

#[test]
fn the_cursor_can_stay_where_it_was() {
    let mut screen = Screen::new();
    screen.print(b"ab");
    screen.image("a=T,C=1", 20, 40);
    assert_eq!(screen.cursor(), Point::new(Line(0), Column(2)));
}

#[test]
fn an_image_at_the_bottom_scrolls_the_screen() {
    let mut screen = Screen::new();
    screen.print(b"top\r\n\n\n\n");
    screen.image("a=T", 10, 60);
    assert_eq!(screen.tile(2, 0), Some((0, 0)));
    assert_eq!(screen.tile(4, 0), Some((2, 0)));
    assert_eq!(screen.term.grid().history_size(), 2);
    assert_eq!(screen.cursor(), Point::new(Line(4), Column(1)));
}

#[test]
fn an_image_is_cut_at_the_right_edge() {
    let mut screen = Screen::new();
    screen.print(b"\x1b[1;9H");
    screen.image("a=T", 50, 20);
    assert_eq!(screen.tile(0, 8), Some((0, 0)));
    assert_eq!(screen.tile(0, 9), Some((0, 1)));
    assert_eq!(screen.cursor(), Point::new(Line(1), Column(0)));
}

#[test]
fn text_after_a_full_width_image_does_not_overwrite_it() {
    let mut screen = Screen::new();
    screen.image("a=T", 100, 20);
    screen.print(b"x");
    assert_eq!(screen.tile(0, 9), Some((0, 9)));
    assert_eq!(screen.term.grid()[Line(1)][Column(0)].c, 'x');
}

#[test]
fn a_tall_image_keeps_the_cursor_on_the_screen() {
    let mut screen = Screen::new();
    screen.image("a=T,C=1", 10, 200);
    assert_eq!(screen.cursor(), Point::new(Line(0), Column(0)));
}

#[test]
fn a_cut_wide_character_is_blanked() {
    let mut screen = Screen::new();
    screen.print("日本".as_bytes());
    screen.print(b"\x1b[1;2H");
    screen.image("a=T", 20, 20);
    let row = &screen.term.grid()[Line(0)];
    assert_eq!(row[Column(0)].c, ' ');
    assert_eq!(screen.tile(0, 1), Some((0, 0)));
    assert_eq!(screen.tile(0, 2), Some((0, 1)));
    assert!(
        !row[Column(3)]
            .flags
            .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER)
    );
}

#[test]
fn asked_for_cells_stretch_the_image() {
    let mut screen = Screen::new();
    screen.image("a=T,c=3,r=2", 5, 5);
    assert_eq!(screen.tile(1, 2), Some((1, 2)));
    let parsed = placeholder::parse(&screen.term.grid()[Line(0)][Column(0)]).unwrap();
    let table = lock(&screen.table);
    let placement = table.get(parsed.key).unwrap();
    assert_eq!((placement.columns, placement.rows), (3, 2));
    assert_eq!(placement.fit, ImageFit::Stretch);
}

#[test]
fn clearing_the_screen_removes_the_image() {
    let mut screen = Screen::new();
    screen.image("a=T", 20, 20);
    screen.print(b"\x1b[2J");
    assert_eq!(screen.tile(0, 0), None);
}

#[test]
fn a_chunked_transmission_is_put_together() {
    let mut screen = Screen::new();
    let pixels = STANDARD.encode([0u8; 4 * 4]);
    let (first, rest) = pixels.split_at(8);
    let prepared = screen
        .graphics
        .prepare_kitty(format!("a=T,i=5,s=2,v=2,m=1;{first}").as_bytes());
    assert!(prepared.op.is_none() && prepared.reply.is_none());
    let prepared = screen
        .graphics
        .prepare_kitty(format!("m=0;{rest}").as_bytes());
    assert!(prepared.op.is_some());
    assert_eq!(prepared.reply.unwrap(), b"\x1b_Gi=5;OK\x1b\\");
}

#[test]
fn a_new_command_abandons_an_unfinished_chunked_transmission() {
    let mut screen = Screen::new();
    let prepared = screen.graphics.prepare_kitty(b"a=T,i=5,s=1,v=1,m=1;AAAA");
    assert!(prepared.reply.is_none());
    let reply = screen.kitty("i=31,s=1,v=1,a=q,t=d,f=24", &[0, 0, 0]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=31;OK\x1b\\"));
}

#[test]
fn deleting_without_an_id_deletes_nothing() {
    let mut screen = Screen::new();
    screen.image("a=T", 10, 20);
    screen.kitty("a=d,d=i", b"");
    assert_eq!(screen.tile(0, 0), Some((0, 0)));
}

#[test]
fn a_stored_image_can_be_put_again_by_id() {
    let mut screen = Screen::new();
    assert_eq!(
        screen.image("a=t,i=9", 10, 20).as_deref(),
        Some("\x1b_Gi=9;OK\x1b\\")
    );
    assert_eq!(screen.tile(0, 0), None);
    assert_eq!(
        screen.kitty("a=p,i=9", b"").as_deref(),
        Some("\x1b_Gi=9;OK\x1b\\")
    );
    assert_eq!(screen.tile(0, 0), Some((0, 0)));
    assert_eq!(
        screen.kitty("a=p,i=10", b"").as_deref(),
        Some("\x1b_Gi=10;ENOENT:no such image\x1b\\")
    );
}

#[test]
fn a_query_answers_without_placing_anything() {
    let mut screen = Screen::new();
    let reply = screen.kitty("i=31,s=1,v=1,a=q,t=d,f=24", &[0, 0, 0]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=31;OK\x1b\\"));
    assert_eq!(screen.tile(0, 0), None);
    let reply = screen.kitty("i=32,a=q,t=f", b"/etc/passwd");
    assert!(reply.unwrap().contains("EINVAL"));
}

#[test]
fn deleting_an_image_blanks_its_cells() {
    let mut screen = Screen::new();
    screen.image("a=T,i=1,q=2", 10, 20);
    screen.image("a=T,i=2,q=2", 10, 20);
    screen.kitty("a=d,d=i,i=1", b"");
    assert_eq!(screen.tile(0, 0), None);
    assert_eq!(screen.term.grid()[Line(0)][Column(0)].c, ' ');
    assert_eq!(screen.tile(0, 1), Some((0, 0)));
    screen.kitty("a=d", b"");
    assert_eq!(screen.tile(0, 1), None);
}

#[test]
fn deleting_visible_placements_keeps_those_in_the_scrollback() {
    let mut screen = Screen::new();
    screen.image("a=T,q=2", 10, 20);
    screen.print(b"\r\n\n\n\n\n");
    screen.image("a=T,q=2", 10, 20);
    let old = placeholder::parse(&screen.term.grid()[Line(-1)][Column(0)]).unwrap();
    screen.kitty("a=d", b"");
    assert_eq!(screen.tile(4, 0), None);
    assert!(lock(&screen.table).get(old.key).is_some());
}

#[test]
fn a_virtual_placement_is_found_by_its_image_id() {
    let mut screen = Screen::new();
    screen.image("a=T,U=1,i=77,c=4,r=2,q=2", 30, 30);
    assert_eq!(screen.tile(0, 0), None);
    let table = lock(&screen.table);
    let placement = table.get(77).unwrap();
    assert_eq!((placement.columns, placement.rows), (4, 2));
    assert_eq!(placement.fit, ImageFit::Contain);
    drop(table);
    screen.print("\x1b[38;5;77m\u{10EEEE}\u{0305}\u{0305}".as_bytes());
    let cell = &screen.term.grid()[Line(0)][Column(0)];
    assert_eq!(cell.c, PLACEHOLDER);
    assert_eq!(placeholder::parse(cell).unwrap().key, 77);
}

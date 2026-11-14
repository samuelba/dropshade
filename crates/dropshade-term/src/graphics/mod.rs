//! Inline images, sent with the kitty graphics protocol or as sixels. An image is placed by
//! writing kitty Unicode placeholder cells over the cells it covers, so it scrolls, is cleared
//! and leaves the scrollback with the text around it.
//!
//! Commands are decoded on the pty thread before the terminal is locked ([`Graphics::prepare_kitty`]),
//! then applied where the emulator reached them ([`Graphics::apply`]).

pub mod kitty;
pub mod placeholder;
pub mod sixel;
pub mod table;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use alacritty_terminal::grid::{Dimensions, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};

use kitty::{Action, Command};
use table::{BUDGET, Image, ImageFit, ImagePlacement, PlacementTable, cost};

/// Most base64 bytes gathered for one chunked transmission.
const MAX_CHUNKED: usize = 4 * kitty::MAX_IMAGE_BYTES / 3 + 4;
/// Most transmitted images kept for later placement, however small.
const MAX_IMAGES: usize = 4096;

/// Where the cursor goes once an image is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorAfter {
    /// Kitty: just right of the image, on its last row.
    Right,
    /// Kitty with `C=1`: where it was.
    Stays,
    /// Sixel: on the line below the image, in the column it started at.
    Below,
}

/// A change to the screen, decided before the terminal is locked.
#[derive(Debug, Clone)]
pub enum Op {
    /// Writes an image's placeholders at the cursor.
    Place {
        placement: ImagePlacement,
        image_id: u32,
        cursor: CursorAfter,
    },
    /// Registers a kitty virtual placement; the application prints its placeholders.
    Virtual {
        placement: ImagePlacement,
        image_id: u32,
    },
    /// Removes placements.
    Delete(Target),
}

/// Which placements a delete removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Those with cells on the screen.
    Visible,
    /// All placements of one image.
    Image(u32),
}

#[derive(Debug, Default)]
pub struct Prepared {
    pub op: Option<Op>,
    pub reply: Option<Vec<u8>>,
}

#[derive(Debug)]
struct Stored {
    image: Image,
    order: u64,
}

/// The image state of one terminal, owned by its pty thread.
#[derive(Debug)]
pub struct Graphics {
    table: Arc<Mutex<PlacementTable>>,
    images: HashMap<u32, Stored>,
    /// What `images` costs, by [`cost`].
    stored: usize,
    numbers: HashMap<u32, u32>,
    /// A chunked transmission in progress: its first command and the payload so far.
    chunked: Option<(Command, Vec<u8>)>,
    cell_width: u32,
    cell_height: u32,
    next_id: u32,
    order: u64,
}

impl Graphics {
    pub fn new(table: Arc<Mutex<PlacementTable>>, cell_width: u16, cell_height: u16) -> Self {
        let mut graphics = Self {
            table,
            images: HashMap::new(),
            stored: 0,
            numbers: HashMap::new(),
            chunked: None,
            cell_width: 1,
            cell_height: 1,
            next_id: 0,
            order: 0,
        };
        graphics.set_cell(cell_width, cell_height);
        graphics
    }

    /// Adopts the cell size in pixels that the application is told, which sizes images.
    pub fn set_cell(&mut self, width: u16, height: u16) {
        self.cell_width = u32::from(width.max(1));
        self.cell_height = u32::from(height.max(1));
    }

    /// Handles the data of a sixel image; returns what to do on screen.
    pub fn prepare_sixel(&self, data: &[u8]) -> Prepared {
        match sixel::decode(data) {
            Ok(image) => {
                let placement = self.placement(&Command::default(), image);
                Prepared {
                    op: Some(Op::Place {
                        placement,
                        image_id: 0,
                        cursor: CursorAfter::Below,
                    }),
                    reply: None,
                }
            }
            Err(error) => {
                tracing::debug!(
                    code = error.code,
                    message = error.message,
                    "dropped a sixel image"
                );
                Prepared::default()
            }
        }
    }

    /// Handles a kitty graphics command body; returns what to do on screen and what to answer.
    pub fn prepare_kitty(&mut self, body: &[u8]) -> Prepared {
        let (command, payload) = kitty::parse(body);
        // A command that is more than a chunk abandons the transmission in progress.
        if let Some((first, mut data)) = self.chunked.take().filter(|_| command.continuation) {
            if data.len() + payload.len() > MAX_CHUNKED {
                let too_big = kitty::error("EFBIG", "the image is too large");
                return reply_only(&first, first.id, Err(&too_big));
            }
            data.extend_from_slice(payload);
            if command.more {
                self.chunked = Some((first, data));
                return Prepared::default();
            }
            return self.finish(&first, &data);
        }
        if command.more && matches!(command.action, Action::Transmit | Action::TransmitAndPut) {
            self.chunked = Some((command, payload.to_vec()));
            return Prepared::default();
        }
        self.finish(&command, payload)
    }

    fn finish(&mut self, command: &Command, payload: &[u8]) -> Prepared {
        match command.action {
            Action::Query => {
                let result = kitty::decode(command, payload).map(|_| ());
                reply_only(command, command.id, result.as_ref().copied())
            }
            Action::Transmit | Action::TransmitAndPut => self.transmit(command, payload),
            Action::Put => {
                let id = self.id_of(command);
                if let Some(stored) = self.images.get(&id) {
                    let image = stored.image.clone();
                    Prepared {
                        op: Some(self.put(command, image, id)),
                        reply: kitty::reply(command, id, Ok(())),
                    }
                } else {
                    let missing = kitty::error("ENOENT", "no such image");
                    reply_only(command, id, Err(&missing))
                }
            }
            Action::Delete => self.delete(command),
            Action::Unsupported => {
                let unsupported = kitty::error("EINVAL", "unsupported action");
                reply_only(command, command.id, Err(&unsupported))
            }
        }
    }

    fn transmit(&mut self, command: &Command, payload: &[u8]) -> Prepared {
        let image = match kitty::decode(command, payload) {
            Ok(image) => image,
            Err(error) => return reply_only(command, command.id, Err(&error)),
        };
        let id = if command.id != 0 {
            command.id
        } else if command.number != 0 {
            let id = self.allocate_id();
            self.numbers.insert(command.number, id);
            id
        } else {
            0
        };
        if id != 0 {
            self.store(id, image.clone());
        }
        let op = (command.action == Action::TransmitAndPut).then(|| self.put(command, image, id));
        Prepared {
            op,
            reply: kitty::reply(command, id, Ok(())),
        }
    }

    fn put(&self, command: &Command, image: Image, image_id: u32) -> Op {
        let placement = self.placement(command, image);
        if command.virtual_placement {
            Op::Virtual {
                placement: ImagePlacement {
                    fit: ImageFit::Contain,
                    ..placement
                },
                image_id,
            }
        } else {
            Op::Place {
                placement,
                image_id,
                cursor: if command.cursor_stays {
                    CursorAfter::Stays
                } else {
                    CursorAfter::Right
                },
            }
        }
    }

    /// The cells an image covers: as asked, or enough for its own size.
    fn placement(&self, command: &Command, image: Image) -> ImagePlacement {
        let (width, height) = (u64::from(image.width), u64::from(image.height));
        let (cell_width, cell_height) = (u64::from(self.cell_width), u64::from(self.cell_height));
        let (columns, rows) = match (u64::from(command.columns), u64::from(command.rows)) {
            (0, 0) => (width.div_ceil(cell_width), height.div_ceil(cell_height)),
            (columns, 0) => {
                let scaled = height * columns * cell_width / width;
                (columns, scaled.div_ceil(cell_height))
            }
            (0, rows) => {
                let scaled = width * rows * cell_height / height;
                (scaled.div_ceil(cell_width), rows)
            }
            sized => sized,
        };
        let fit = if command.columns == 0 && command.rows == 0 {
            ImageFit::Native
        } else {
            ImageFit::Stretch
        };
        let clamp = |cells: u64| {
            usize::try_from(cells)
                .unwrap_or(usize::MAX)
                .clamp(1, placeholder::MAX_INDEX + 1)
        };
        ImagePlacement {
            image,
            columns: clamp(columns),
            rows: clamp(rows),
            fit,
        }
    }

    fn delete(&mut self, command: &Command) -> Prepared {
        let free = command.delete.is_ascii_uppercase();
        let target = match command.delete.to_ascii_lowercase() {
            b'a' => Target::Visible,
            b'i' | b'n' => match self.id_of(command) {
                0 => return Prepared::default(),
                id => Target::Image(id),
            },
            _ => return Prepared::default(),
        };
        if free {
            match target {
                Target::Visible => {
                    self.images.clear();
                    self.stored = 0;
                }
                Target::Image(id) => self.unstore(id),
            }
            self.forget_numbers();
        }
        Prepared {
            op: Some(Op::Delete(target)),
            reply: None,
        }
    }

    /// The image a command names, by id or by number.
    fn id_of(&self, command: &Command) -> u32 {
        if command.id == 0 && command.number != 0 {
            self.numbers.get(&command.number).copied().unwrap_or(0)
        } else {
            command.id
        }
    }

    fn allocate_id(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.images.contains_key(&self.next_id) {
                return self.next_id;
            }
        }
    }

    fn store(&mut self, id: u32, image: Image) {
        self.unstore(id);
        self.order += 1;
        self.stored += cost(&image);
        self.images.insert(
            id,
            Stored {
                image,
                order: self.order,
            },
        );
        let mut evicted = false;
        while self.stored > BUDGET || self.images.len() > MAX_IMAGES {
            let Some(oldest) = self
                .images
                .iter()
                .min_by_key(|(_, stored)| stored.order)
                .map(|(&id, _)| id)
            else {
                break;
            };
            self.unstore(oldest);
            evicted = true;
        }
        if evicted {
            self.forget_numbers();
        }
    }

    fn unstore(&mut self, id: u32) {
        if let Some(stored) = self.images.remove(&id) {
            self.stored -= cost(&stored.image);
        }
    }

    /// Drops the numbers of images no longer kept.
    fn forget_numbers(&mut self) {
        let images = &self.images;
        self.numbers.retain(|_, id| images.contains_key(id));
    }

    /// Applies `op` at the cursor. `feed` runs bytes through the emulator, to move the cursor
    /// (and scroll) the way it would.
    pub fn apply<T>(&mut self, term: &mut Term<T>, op: Op, feed: impl FnMut(&mut Term<T>, &[u8])) {
        let alt = term.mode().contains(TermMode::ALT_SCREEN);
        let mut table = lock(&self.table);
        match op {
            Op::Place {
                placement,
                image_id,
                cursor,
            } => {
                let key = table.allocate_key();
                let (columns, rows) = (placement.columns, placement.rows);
                table.insert(key, placement, image_id, alt, false);
                write_placeholders(term, key, columns, rows, cursor, feed);
            }
            Op::Virtual {
                placement,
                image_id,
            } => table.insert(image_id, placement, image_id, alt, true),
            Op::Delete(Target::Visible) => {
                let visible = keys_on_screen(term);
                let written = table.remove(|key, _| visible.contains(&key));
                blank_visible(term, &written);
            }
            Op::Delete(Target::Image(wanted)) => {
                let written = table.remove(|_, image_id| image_id == wanted);
                blank_visible(term, &written);
            }
        }
        if table.over_budget() {
            table.collect(&keys_in_use(term), alt);
        }
    }
}

fn reply_only(command: &Command, id: u32, result: Result<(), &kitty::Error>) -> Prepared {
    Prepared {
        op: None,
        reply: kitty::reply(command, id, result),
    }
}

pub fn lock(table: &Mutex<PlacementTable>) -> MutexGuard<'_, PlacementTable> {
    table.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Covers `columns` × `rows` cells from the cursor with placeholders for `key`, clipped at the
/// right edge, scrolling when the image reaches below the screen. The cursor is moved with
/// escape sequences, so the emulator keeps it on the screen and forgets a pending wrap.
fn write_placeholders<T>(
    term: &mut Term<T>,
    key: u32,
    columns: usize,
    rows: usize,
    cursor: CursorAfter,
    mut feed: impl FnMut(&mut Term<T>, &[u8]),
) {
    let start = term.grid().cursor.point.column.0;
    let width = term.columns();
    let columns = columns.min(width.saturating_sub(start));
    if columns == 0 {
        return;
    }
    let to_start = to_column(start);
    feed(term, &to_start);
    for row in 0..rows {
        if row > 0 {
            feed(term, b"\n");
            feed(term, &to_start);
        }
        let line = term.grid().cursor.point.line;
        let cells = &mut term.grid_mut()[line];
        unsplit_wide(cells, start, start + columns, width);
        for column in 0..columns {
            placeholder::fill(&mut cells[Column(start + column)], key, row, column);
        }
    }
    match cursor {
        CursorAfter::Right if start + columns < width => feed(term, &to_column(start + columns)),
        // Nothing fits right of the image, and text there would overwrite its last cell.
        CursorAfter::Right => feed(term, b"\n\r"),
        CursorAfter::Below => {
            feed(term, b"\n");
            feed(term, &to_start);
        }
        CursorAfter::Stays => {
            feed(term, &to_start);
            if rows > 1 {
                feed(term, format!("\x1b[{}A", rows - 1).as_bytes());
            }
        }
    }
}

/// Moves to `column` of the cursor's line.
fn to_column(column: usize) -> Vec<u8> {
    if column == 0 {
        b"\r".to_vec()
    } else {
        format!("\r\x1b[{column}C").into_bytes()
    }
}

/// Blanks the other half of a wide character cut at either end of `start..end`.
fn unsplit_wide(row: &mut Row<Cell>, start: usize, end: usize, width: usize) {
    if start > 0 && row[Column(start)].flags.contains(Flags::WIDE_CHAR_SPACER) {
        row[Column(start - 1)] = Cell::default();
    }
    if end < width && row[Column(end)].flags.contains(Flags::WIDE_CHAR_SPACER) {
        row[Column(end)] = Cell::default();
    }
}

/// Blanks the on-screen placeholder cells of the given placements.
fn blank_visible<T>(term: &mut Term<T>, keys: &HashSet<u32>) {
    if keys.is_empty() {
        return;
    }
    let columns = term.columns();
    for line in 0..term.screen_lines() {
        let row = &mut term.grid_mut()[Line(i32::try_from(line).unwrap_or(i32::MAX))];
        for column in 0..columns {
            let cell = &mut row[Column(column)];
            if placeholder::parse(cell).is_some_and(|parsed| keys.contains(&parsed.key)) {
                *cell = Cell::default();
            }
        }
    }
}

/// The placement keys of every placeholder cell on the current screen and in its scrollback.
fn keys_in_use<T>(term: &Term<T>) -> HashSet<u32> {
    keys_between(term, term.topmost_line().0, term.bottommost_line().0)
}

/// The placement keys of the placeholder cells on the screen.
fn keys_on_screen<T>(term: &Term<T>) -> HashSet<u32> {
    keys_between(term, 0, term.bottommost_line().0)
}

fn keys_between<T>(term: &Term<T>, top: i32, bottom: i32) -> HashSet<u32> {
    let grid = term.grid();
    let columns = term.columns();
    let mut keys = HashSet::new();
    for line in top..=bottom {
        let row = &grid[Line(line)];
        for column in 0..columns {
            if let Some(parsed) = placeholder::parse(&row[Column(column)]) {
                keys.insert(parsed.key);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests;

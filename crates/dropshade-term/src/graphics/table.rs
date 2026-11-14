//! The images placed in a terminal, shared by the pty thread that places them and the view that
//! draws them.

use std::collections::{HashMap, HashSet};

use iced::widget::image::Handle;

/// Decoded image memory kept, for placements and again for images kept to place later.
pub const BUDGET: usize = 160 * 1024 * 1024;
/// Most placements kept, however small their images.
const MAX_PLACEMENTS: usize = 16_384;
/// What an image is charged at least, for the bookkeeping around it.
const MIN_COST: usize = 4096;

/// What keeping `image` is charged against [`BUDGET`].
pub fn cost(image: &Image) -> usize {
    image.bytes().max(MIN_COST)
}

/// How an image fills its cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFit {
    /// At its own size in pixels, from the top-left corner.
    Native,
    /// Stretched over all of its cells.
    Stretch,
    /// As large as fits in its cells, keeping its aspect ratio, centred.
    Contain,
}

/// A decoded image, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
}

impl Image {
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            handle: Handle::from_rgba(width, height, pixels),
            width,
            height,
        }
    }

    pub fn bytes(&self) -> usize {
        usize::try_from(u64::from(self.width) * u64::from(self.height) * 4).unwrap_or(usize::MAX)
    }
}

/// An image shown over a rectangle of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePlacement {
    pub image: Image,
    pub columns: usize,
    pub rows: usize,
    pub fit: ImageFit,
}

#[derive(Debug, Clone)]
struct Entry {
    placement: ImagePlacement,
    /// The kitty image id it shows, or 0 for an image without one.
    image_id: u32,
    /// Placed on the alternate screen.
    alt: bool,
    /// A kitty virtual placement, whose cells the application prints itself.
    virtual_placement: bool,
    order: u64,
}

/// Placements by the key their placeholder cells carry.
#[derive(Debug, Default)]
pub struct PlacementTable {
    entries: HashMap<u32, Entry>,
    /// What the entries cost, by [`cost`]; an image placed twice counts twice.
    cost: usize,
    next_key: u32,
    order: u64,
}

/// Keys handed out for Dropshade's own placements; they fit in a cell's 24-bit colour.
const FIRST_KEY: u32 = 0x80_0000;
const LAST_KEY: u32 = 0xFF_FFFF;

impl PlacementTable {
    pub fn get(&self, key: u32) -> Option<&ImagePlacement> {
        self.entries.get(&key).map(|entry| &entry.placement)
    }

    /// A key no placement uses yet.
    pub fn allocate_key(&mut self) -> u32 {
        loop {
            self.next_key = if (FIRST_KEY..LAST_KEY).contains(&self.next_key) {
                self.next_key + 1
            } else {
                FIRST_KEY
            };
            if !self.entries.contains_key(&self.next_key) {
                return self.next_key;
            }
        }
    }

    pub fn insert(
        &mut self,
        key: u32,
        placement: ImagePlacement,
        image_id: u32,
        alt: bool,
        virtual_placement: bool,
    ) {
        self.order += 1;
        self.cost += cost(&placement.image);
        let replaced = self.entries.insert(
            key,
            Entry {
                placement,
                image_id,
                alt,
                virtual_placement,
                order: self.order,
            },
        );
        if let Some(replaced) = replaced {
            self.cost -= cost(&replaced.placement.image);
        }
    }

    /// Removes the placements `remove` picks by key and image id; returns the keys of the ones
    /// whose cells Dropshade wrote, which should be blanked.
    pub fn remove(&mut self, mut remove: impl FnMut(u32, u32) -> bool) -> HashSet<u32> {
        let mut written = HashSet::new();
        let mut freed = 0;
        self.entries.retain(|&key, entry| {
            if !remove(key, entry.image_id) {
                return true;
            }
            if !entry.virtual_placement {
                written.insert(key);
            }
            freed += cost(&entry.placement.image);
            false
        });
        self.cost -= freed;
        written
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.cost = 0;
    }

    pub fn over_budget(&self) -> bool {
        self.cost > BUDGET || self.entries.len() > MAX_PLACEMENTS
    }

    /// Drops the placements of the current screen no cell refers to any more, then the oldest
    /// ones, down to three quarters of the budget so that collecting stays rare. Placements
    /// left on the alternate screen are gone once the primary one is back.
    pub fn collect(&mut self, used: &HashSet<u32>, alt: bool) {
        self.remove_where(|key, entry| {
            !(entry.virtual_placement || used.contains(&key) || (entry.alt != alt && alt))
        });
        let (budget, most) = (BUDGET / 4 * 3, MAX_PLACEMENTS / 4 * 3);
        if self.cost <= budget && self.entries.len() <= most {
            return;
        }
        let mut by_age: Vec<(u64, u32)> = self
            .entries
            .iter()
            .map(|(&key, entry)| (entry.order, key))
            .collect();
        by_age.sort_unstable();
        for (_, key) in by_age {
            if self.cost <= budget && self.entries.len() <= most {
                break;
            }
            if let Some(entry) = self.entries.remove(&key) {
                self.cost -= cost(&entry.placement.image);
            }
        }
    }

    fn remove_where(&mut self, mut remove: impl FnMut(u32, &Entry) -> bool) {
        let mut freed = 0;
        self.entries.retain(|&key, entry| {
            let gone = remove(key, entry);
            if gone {
                freed += cost(&entry.placement.image);
            }
            !gone
        });
        self.cost -= freed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(size: u32) -> ImagePlacement {
        let bytes = usize::try_from(size * size * 4).unwrap();
        ImagePlacement {
            image: Image::from_rgba(size, size, vec![0; bytes]),
            columns: 1,
            rows: 1,
            fit: ImageFit::Native,
        }
    }

    #[test]
    fn keys_are_not_reused_while_taken() {
        let mut table = PlacementTable::default();
        let first = table.allocate_key();
        table.insert(first, placement(1), 0, false, false);
        table.next_key = first - 1;
        assert_ne!(table.allocate_key(), first);
        assert!(table.allocate_key() <= LAST_KEY);
    }

    #[test]
    fn removing_an_image_reports_only_the_cells_dropshade_wrote() {
        let mut table = PlacementTable::default();
        table.insert(1, placement(1), 7, false, false);
        table.insert(2, placement(1), 7, false, true);
        table.insert(3, placement(1), 8, false, false);
        assert_eq!(table.remove(|_, id| id == 7), HashSet::from([1]));
        assert!(table.get(2).is_none());
        assert!(table.get(3).is_some());
    }

    #[test]
    fn small_images_cost_a_minimum_and_the_cost_follows_changes() {
        let mut table = PlacementTable::default();
        table.insert(1, placement(1), 7, false, false);
        table.insert(2, placement(64), 8, false, false);
        assert_eq!(table.cost, MIN_COST + 64 * 64 * 4);
        table.insert(2, placement(1), 8, false, false);
        assert_eq!(table.cost, 2 * MIN_COST);
        table.remove(|_, id| id == 7);
        assert_eq!(table.cost, MIN_COST);
        table.clear();
        assert_eq!(table.cost, 0);
    }

    #[test]
    fn too_many_placements_are_over_budget_and_collecting_drops_unused_then_oldest() {
        let mut table = PlacementTable::default();
        for key in 0..=u32::try_from(MAX_PLACEMENTS).unwrap() {
            table.insert(key, placement(1), 0, false, false);
        }
        assert!(table.over_budget());
        let used: HashSet<u32> = (0..u32::try_from(MAX_PLACEMENTS).unwrap()).collect();
        table.collect(&used, false);
        assert!(!table.over_budget());
        assert_eq!(table.entries.len(), MAX_PLACEMENTS / 4 * 3);
        assert!(table.get(0).is_none());
        assert!(
            table
                .get(u32::try_from(MAX_PLACEMENTS).unwrap() - 1)
                .is_some()
        );
        assert_eq!(table.cost, table.entries.len() * MIN_COST);
    }

    #[test]
    fn virtual_placements_survive_collecting_while_unused() {
        let mut table = PlacementTable::default();
        table.insert(1, placement(1), 0, false, true);
        table.insert(2, placement(1), 0, false, false);
        table.collect(&HashSet::new(), false);
        assert!(table.get(1).is_some());
        assert!(table.get(2).is_none());
    }

    #[test]
    fn the_other_screens_placements_survive_only_while_on_the_alternate_screen() {
        let mut table = PlacementTable::default();
        table.insert(1, placement(1), 0, false, false);
        table.collect(&HashSet::new(), true);
        assert!(table.get(1).is_some());
        table.insert(2, placement(1), 0, true, false);
        table.collect(&HashSet::new(), false);
        assert!(table.get(1).is_none());
        assert!(table.get(2).is_none());
    }
}

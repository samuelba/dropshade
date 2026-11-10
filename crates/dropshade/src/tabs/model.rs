//! The ordered set of tabs and which one is active. Generic over the tab content so the rules can
//! be tested without spawning terminals.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(u64);

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct Tab<T> {
    pub id: TabId,
    /// Title the shell or application set (OSC 0/2).
    pub title: Option<String>,
    /// Name the user gave the tab; wins over the title.
    pub custom_name: Option<String>,
    pub content: T,
}

const FALLBACK_LABEL: &str = "Terminal";

impl<T> Tab<T> {
    pub fn label(&self) -> &str {
        self.custom_name
            .as_deref()
            .or(self.title.as_deref())
            .unwrap_or(FALLBACK_LABEL)
    }
}

#[derive(Debug)]
pub struct Tabs<T> {
    entries: Vec<Tab<T>>,
    active: usize,
    next_id: u64,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }
}

impl<T> Tabs<T> {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab<T>> {
        self.entries.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Tab<T>> {
        self.entries.iter_mut()
    }

    pub fn active(&self) -> Option<&Tab<T>> {
        self.entries.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab<T>> {
        self.entries.get_mut(self.active)
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active().map(|tab| tab.id)
    }

    pub fn get(&self, id: TabId) -> Option<&Tab<T>> {
        self.entries.iter().find(|tab| tab.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab<T>> {
        self.entries.iter_mut().find(|tab| tab.id == id)
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.entries.iter().position(|tab| tab.id == id)
    }

    /// Adds a tab after the active one and activates it.
    pub fn insert(&mut self, content: T) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let index = if self.entries.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.entries.insert(
            index,
            Tab {
                id,
                title: None,
                custom_name: None,
                content,
            },
        );
        self.active = index;
        id
    }

    /// Removes a tab. Closing the active tab activates its right neighbour, or the left one when
    /// it was the last tab.
    pub fn close(&mut self, id: TabId) -> Option<Tab<T>> {
        let index = self.index_of(id)?;
        let tab = self.entries.remove(index);
        if index < self.active || self.active >= self.entries.len() {
            self.active = self.active.saturating_sub(1);
        }
        Some(tab)
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        self.index_of(id)
            .is_some_and(|index| self.activate_index(index))
    }

    /// Activates the tab at `index`; out-of-range indexes are ignored.
    pub fn activate_index(&mut self, index: usize) -> bool {
        let exists = index < self.entries.len();
        if exists {
            self.active = index;
        }
        exists
    }

    /// Activates the next (`offset > 0`) or previous tab, wrapping around.
    pub fn cycle(&mut self, offset: isize) {
        let Ok(len) = isize::try_from(self.entries.len()) else {
            return;
        };
        let Ok(active) = isize::try_from(self.active) else {
            return;
        };
        if len > 0 {
            self.active = usize::try_from((active + offset).rem_euclid(len)).unwrap_or(0);
        }
    }

    /// Moves a tab to `index` (clamped), keeping it active if it was.
    pub fn move_to(&mut self, id: TabId, index: usize) {
        let Some(from) = self.index_of(id) else {
            return;
        };
        let active_id = self.active_id();
        let tab = self.entries.remove(from);
        let to = index.min(self.entries.len());
        self.entries.insert(to, tab);
        if let Some(active_index) = active_id.and_then(|active| self.index_of(active)) {
            self.active = active_index;
        }
    }

    /// Moves the active tab one place left (`-1`) or right (`1`), without wrapping.
    pub fn move_active(&mut self, offset: isize) {
        let Some(id) = self.active_id() else { return };
        if let Some(target) = self.active.checked_add_signed(offset)
            && target < self.entries.len()
        {
            self.move_to(id, target);
        }
    }

    /// Sets or clears a tab's custom name; blank names clear it.
    pub fn rename(&mut self, id: TabId, name: &str) {
        if let Some(tab) = self.get_mut(id) {
            let name = name.trim();
            tab.custom_name = (!name.is_empty()).then(|| name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(count: usize) -> (Tabs<usize>, Vec<TabId>) {
        let mut tabs = Tabs::default();
        let ids = (0..count).map(|n| tabs.insert(n)).collect();
        (tabs, ids)
    }

    fn order(tabs: &Tabs<usize>) -> Vec<usize> {
        tabs.iter().map(|tab| tab.content).collect()
    }

    fn active(tabs: &Tabs<usize>) -> Option<usize> {
        tabs.active().map(|tab| tab.content)
    }

    #[test]
    fn inserting_adds_after_the_active_tab_and_activates_it() {
        let (mut tabs, ids) = tabs(3);
        assert_eq!(order(&tabs), vec![0, 1, 2]);
        assert_eq!(active(&tabs), Some(2));
        tabs.activate(ids[0]);
        tabs.insert(9);
        assert_eq!(order(&tabs), vec![0, 9, 1, 2]);
        assert_eq!(active(&tabs), Some(9));
    }

    #[test]
    fn ids_are_unique_even_after_closing() {
        let (mut tabs, ids) = tabs(2);
        tabs.close(ids[1]);
        let new = tabs.insert(5);
        assert!(!ids.contains(&new));
    }

    #[test]
    fn closing_the_active_tab_activates_the_right_neighbour() {
        let (mut tabs, ids) = tabs(3);
        tabs.activate(ids[1]);
        let closed = tabs.close(ids[1]).map(|tab| tab.content);
        assert_eq!(closed, Some(1));
        assert_eq!(active(&tabs), Some(2));
    }

    #[test]
    fn closing_the_last_active_tab_activates_the_left_neighbour() {
        let (mut tabs, ids) = tabs(3);
        tabs.close(ids[2]);
        assert_eq!(active(&tabs), Some(1));
    }

    #[test]
    fn closing_another_tab_keeps_the_active_one() {
        let (mut tabs, ids) = tabs(3);
        tabs.close(ids[0]);
        assert_eq!(active(&tabs), Some(2));
        assert_eq!(order(&tabs), vec![1, 2]);
    }

    #[test]
    fn closing_everything_leaves_no_active_tab() {
        let (mut tabs, ids) = tabs(1);
        tabs.close(ids[0]);
        assert!(tabs.is_empty());
        assert_eq!(tabs.active_id(), None);
        assert!(tabs.close(ids[0]).is_none());
    }

    #[test]
    fn cycling_wraps_around() {
        let (mut tabs, _) = tabs(3);
        tabs.cycle(1);
        assert_eq!(active(&tabs), Some(0));
        tabs.cycle(-1);
        assert_eq!(active(&tabs), Some(2));
        tabs.cycle(-4);
        assert_eq!(active(&tabs), Some(1));
    }

    #[test]
    fn activating_by_index_ignores_missing_tabs() {
        let (mut tabs, _) = tabs(3);
        assert!(tabs.activate_index(0));
        assert_eq!(active(&tabs), Some(0));
        assert!(!tabs.activate_index(7));
        assert_eq!(active(&tabs), Some(0));
    }

    #[test]
    fn moving_reorders_and_keeps_the_active_tab() {
        let (mut tabs, ids) = tabs(4);
        tabs.activate(ids[1]);
        tabs.move_to(ids[1], 3);
        assert_eq!(order(&tabs), vec![0, 2, 3, 1]);
        assert_eq!(active(&tabs), Some(1));
        tabs.move_to(ids[3], 0);
        assert_eq!(order(&tabs), vec![3, 0, 2, 1]);
        assert_eq!(active(&tabs), Some(1));
        tabs.move_to(ids[0], 99);
        assert_eq!(order(&tabs), vec![3, 2, 1, 0]);
    }

    #[test]
    fn moving_the_active_tab_stops_at_the_edges() {
        let (mut tabs, _) = tabs(3);
        tabs.move_active(1);
        assert_eq!(order(&tabs), vec![0, 1, 2]);
        tabs.move_active(-1);
        assert_eq!(order(&tabs), vec![0, 2, 1]);
        tabs.move_active(-1);
        tabs.move_active(-1);
        assert_eq!(order(&tabs), vec![2, 0, 1]);
        assert_eq!(active(&tabs), Some(2));
    }

    #[test]
    fn renaming_trims_and_blank_names_fall_back_to_the_title() {
        let (mut tabs, ids) = tabs(1);
        let id = ids[0];
        assert_eq!(tabs.get(id).map(Tab::label), Some("Terminal"));
        tabs.get_mut(id).expect("tab").title = Some("vim".into());
        assert_eq!(tabs.get(id).map(Tab::label), Some("vim"));
        tabs.rename(id, "  logs  ");
        assert_eq!(tabs.get(id).map(Tab::label), Some("logs"));
        tabs.rename(id, "   ");
        assert_eq!(tabs.get(id).map(Tab::label), Some("vim"));
    }
}

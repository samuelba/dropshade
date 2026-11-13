//! The panes of one tab: terminals in a split layout, and the one that has the keyboard. Generic
//! over the pane content so the rules can be tested without spawning terminals.

use iced::widget::pane_grid::{self, Axis, Direction, Node, Pane, Split};

/// How far one keyboard resize step moves a divider, as a share of the split's size.
const RESIZE_STEP: f32 = 0.05;
/// How far keyboard resizing may push a divider towards either edge.
const MIN_RATIO: f32 = 0.1;
const MAX_RATIO: f32 = 0.9;

/// Where a new pane goes, next to the pane it splits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Right,
    Down,
}

impl SplitDirection {
    fn axis(self) -> Axis {
        match self {
            // A vertical divider (`|`) puts the panes side by side.
            Self::Right => Axis::Vertical,
            Self::Down => Axis::Horizontal,
        }
    }
}

#[derive(Debug)]
pub struct Panes<T> {
    grid: pane_grid::State<T>,
    focused: Pane,
}

impl<T> Panes<T> {
    pub fn new(content: T) -> Self {
        let (grid, focused) = pane_grid::State::new(content);
        Self { grid, focused }
    }

    /// The layout, for the pane grid widget.
    pub fn grid(&self) -> &pane_grid::State<T> {
        &self.grid
    }

    pub fn layout(&self) -> &Node {
        self.grid.layout()
    }

    pub fn is_split(&self) -> bool {
        self.grid.len() > 1
    }

    /// The pane that has the keyboard.
    pub fn focused(&self) -> Pane {
        self.focused
    }

    pub fn focused_content(&self) -> Option<&T> {
        self.grid.get(self.focused)
    }

    pub fn focused_content_mut(&mut self) -> Option<&mut T> {
        self.grid.get_mut(self.focused)
    }

    pub fn get(&self, pane: Pane) -> Option<&T> {
        self.grid.get(pane)
    }

    pub fn get_mut(&mut self, pane: Pane) -> Option<&mut T> {
        self.grid.get_mut(pane)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Pane, &T)> {
        self.grid.iter().map(|(pane, content)| (*pane, content))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Pane, &mut T)> {
        self.grid.iter_mut().map(|(pane, content)| (*pane, content))
    }

    /// Splits the focused pane in half and focuses the new pane. A zoomed pane is unzoomed first.
    pub fn split(&mut self, direction: SplitDirection, content: T) -> Option<Pane> {
        let pane = self.split_pane(self.focused, direction, 0.5, content)?;
        self.focused = pane;
        Some(pane)
    }

    /// Splits `target`, giving the existing pane `ratio` of its space and the new one the rest,
    /// without moving the focus.
    pub fn split_pane(
        &mut self,
        target: Pane,
        direction: SplitDirection,
        ratio: f32,
        content: T,
    ) -> Option<Pane> {
        let (pane, split) = self.grid.split(direction.axis(), target, content)?;
        if (ratio - 0.5).abs() > f32::EPSILON {
            self.grid.resize(split, ratio);
        }
        Some(pane)
    }

    /// Removes a pane; the focus moves to its nearest neighbour when it had it. The last pane
    /// cannot be closed this way (its tab is closed instead), so that returns `None`.
    pub fn close(&mut self, pane: Pane) -> Option<T> {
        let (content, sibling) = self.grid.close(pane)?;
        if self.focused == pane {
            self.focused = sibling;
        }
        Some(content)
    }

    /// Gives `pane` the keyboard; returns whether the focus changed.
    pub fn focus(&mut self, pane: Pane) -> bool {
        let changed = self.focused != pane && self.grid.get(pane).is_some();
        if changed {
            self.focused = pane;
        }
        changed
    }

    /// Moves the focus to the neighbouring pane in `direction`, unzooming to show it. Returns
    /// whether the focus changed.
    pub fn focus_toward(&mut self, direction: Direction) -> bool {
        let Some(pane) = self.grid.adjacent(self.focused, direction) else {
            return false;
        };
        self.grid.restore();
        self.focus(pane)
    }

    /// Moves the nearest divider of the focused pane that runs across `direction` one step that
    /// way. Returns whether there was one.
    pub fn resize_toward(&mut self, direction: Direction) -> bool {
        let (axis, step) = match direction {
            Direction::Left => (Axis::Vertical, -RESIZE_STEP),
            Direction::Right => (Axis::Vertical, RESIZE_STEP),
            Direction::Up => (Axis::Horizontal, -RESIZE_STEP),
            Direction::Down => (Axis::Horizontal, RESIZE_STEP),
        };
        let Some((split, ratio)) = enclosing_split(self.layout(), self.focused, axis) else {
            return false;
        };
        self.grid
            .resize(split, (ratio + step).clamp(MIN_RATIO, MAX_RATIO));
        true
    }

    /// Applies a divider drag.
    pub fn resize(&mut self, split: Split, ratio: f32) {
        self.grid.resize(split, ratio);
    }

    /// Fills the tab with the focused pane, or shows all panes again.
    pub fn toggle_zoom(&mut self) {
        if self.zoomed() {
            self.grid.restore();
        } else if self.is_split() {
            self.grid.maximize(self.focused);
        }
    }

    pub fn zoomed(&self) -> bool {
        self.grid.maximized().is_some()
    }
}

/// The deepest split along `axis` that contains `pane`, with its ratio.
fn enclosing_split(node: &Node, pane: Pane, axis: Axis) -> Option<(Split, f32)> {
    let Node::Split {
        id,
        axis: split_axis,
        ratio,
        a,
        b,
    } = node
    else {
        return None;
    };
    let inner = if contains(a, pane) {
        a
    } else if contains(b, pane) {
        b
    } else {
        return None;
    };
    enclosing_split(inner, pane, axis).or_else(|| (*split_axis == axis).then_some((*id, *ratio)))
}

fn contains(node: &Node, pane: Pane) -> bool {
    match node {
        Node::Pane(leaf) => *leaf == pane,
        Node::Split { a, b, .. } => contains(a, pane) || contains(b, pane),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio_of(node: &Node, wanted: Split) -> Option<f32> {
        match node {
            Node::Pane(_) => None,
            Node::Split {
                id, ratio, a, b, ..
            } => {
                if *id == wanted {
                    Some(*ratio)
                } else {
                    ratio_of(a, wanted).or_else(|| ratio_of(b, wanted))
                }
            }
        }
    }

    fn root_split(panes: &Panes<usize>) -> (Split, f32) {
        match panes.layout() {
            Node::Split { id, ratio, .. } => (*id, *ratio),
            Node::Pane(_) => panic!("not split"),
        }
    }

    #[test]
    fn a_new_tab_has_one_focused_pane() {
        let panes = Panes::new(7);
        assert_eq!(panes.iter().count(), 1);
        assert!(!panes.is_split());
        assert_eq!(panes.focused_content(), Some(&7));
    }

    #[test]
    fn splitting_focuses_the_new_pane() {
        let mut panes = Panes::new(0);
        let first = panes.focused();
        let second = panes.split(SplitDirection::Right, 1).expect("split");
        assert_eq!(panes.focused(), second);
        assert_eq!(panes.focused_content(), Some(&1));
        assert!(panes.is_split());
        assert!(matches!(
            panes.layout(),
            Node::Split { axis: Axis::Vertical, a, b, .. }
                if matches!(**a, Node::Pane(pane) if pane == first)
                    && matches!(**b, Node::Pane(pane) if pane == second)
        ));
    }

    #[test]
    fn splitting_down_stacks_the_panes() {
        let mut panes = Panes::new(0);
        panes.split(SplitDirection::Down, 1);
        assert!(matches!(
            panes.layout(),
            Node::Split {
                axis: Axis::Horizontal,
                ..
            }
        ));
    }

    #[test]
    fn closing_the_focused_pane_focuses_its_neighbour() {
        let mut panes = Panes::new(0);
        let first = panes.focused();
        let second = panes.split(SplitDirection::Right, 1).expect("split");
        assert_eq!(panes.close(second), Some(1));
        assert_eq!(panes.focused(), first);
        assert!(!panes.is_split());
    }

    #[test]
    fn closing_another_pane_keeps_the_focus() {
        let mut panes = Panes::new(0);
        let first = panes.focused();
        let second = panes.split(SplitDirection::Right, 1).expect("split");
        panes.split(SplitDirection::Down, 2);
        panes.focus(second);
        assert_eq!(panes.close(first), Some(0));
        assert_eq!(panes.focused(), second);
    }

    #[test]
    fn the_last_pane_is_not_closed() {
        let mut panes = Panes::new(0);
        let only = panes.focused();
        assert_eq!(panes.close(only), None);
        assert_eq!(panes.iter().count(), 1);
    }

    #[test]
    fn focus_moves_to_the_neighbour_in_a_direction() {
        let mut panes = Panes::new(0);
        let left = panes.focused();
        let right = panes.split(SplitDirection::Right, 1).expect("split");
        let below = panes.split(SplitDirection::Down, 2).expect("split");
        assert!(panes.focus_toward(Direction::Up));
        assert_eq!(panes.focused(), right);
        assert!(panes.focus_toward(Direction::Left));
        assert_eq!(panes.focused(), left);
        assert!(!panes.focus_toward(Direction::Left), "nothing further left");
        assert!(panes.focus_toward(Direction::Right));
        assert!(panes.focused() == right || panes.focused() == below);
    }

    #[test]
    fn focusing_an_unknown_or_the_same_pane_changes_nothing() {
        let mut panes = Panes::new(0);
        let first = panes.focused();
        let second = panes.split(SplitDirection::Right, 1).expect("split");
        panes.close(second);
        assert!(!panes.focus(second));
        assert!(!panes.focus(first));
    }

    #[test]
    fn keyboard_resizing_moves_the_matching_divider() {
        let mut panes = Panes::new(0);
        panes.split(SplitDirection::Right, 1);
        let (split, _) = root_split(&panes);
        assert!(panes.resize_toward(Direction::Right));
        let ratio = ratio_of(panes.layout(), split).expect("ratio");
        assert!((ratio - 0.55).abs() < 1e-5, "{ratio}");
        assert!(panes.resize_toward(Direction::Left));
        assert!(panes.resize_toward(Direction::Left));
        let ratio = ratio_of(panes.layout(), split).expect("ratio");
        assert!((ratio - 0.45).abs() < 1e-5, "{ratio}");
        assert!(
            !panes.resize_toward(Direction::Up),
            "no horizontal divider to move"
        );
    }

    #[test]
    fn keyboard_resizing_picks_the_nearest_enclosing_divider() {
        let mut panes = Panes::new(0);
        panes.split(SplitDirection::Right, 1);
        let (outer, _) = root_split(&panes);
        panes.split(SplitDirection::Down, 2);
        panes.split(SplitDirection::Right, 3);
        // The focused pane sits in a vertical split inside a horizontal one inside the outer
        // vertical split; Left moves the innermost vertical divider.
        assert!(panes.resize_toward(Direction::Left));
        assert!((ratio_of(panes.layout(), outer).expect("outer") - 0.5).abs() < 1e-5);
        assert!(panes.resize_toward(Direction::Up));
        assert!((ratio_of(panes.layout(), outer).expect("outer") - 0.5).abs() < 1e-5);
    }

    #[test]
    fn keyboard_resizing_stops_short_of_the_edges() {
        let mut panes = Panes::new(0);
        panes.split(SplitDirection::Down, 1);
        let (split, _) = root_split(&panes);
        for _ in 0..30 {
            panes.resize_toward(Direction::Down);
        }
        let ratio = ratio_of(panes.layout(), split).expect("ratio");
        assert!((ratio - MAX_RATIO).abs() < 1e-5, "{ratio}");
    }

    #[test]
    fn restoring_a_split_keeps_its_ratio_and_the_focus() {
        let mut panes = Panes::new(0);
        let first = panes.focused();
        let second = panes
            .split_pane(first, SplitDirection::Right, 0.7, 1)
            .expect("split");
        assert_eq!(panes.focused(), first);
        assert_eq!(panes.get(second), Some(&1));
        let (_, ratio) = root_split(&panes);
        assert!((ratio - 0.7).abs() < 1e-5);
    }

    #[test]
    fn zoom_toggles_and_only_applies_to_splits() {
        let mut panes = Panes::new(0);
        panes.toggle_zoom();
        assert!(!panes.zoomed(), "a lone pane has nothing to zoom");
        panes.split(SplitDirection::Right, 1);
        panes.toggle_zoom();
        assert!(panes.zoomed());
        panes.toggle_zoom();
        assert!(!panes.zoomed());
    }

    #[test]
    fn splitting_or_moving_the_focus_ends_the_zoom() {
        let mut panes = Panes::new(0);
        panes.split(SplitDirection::Right, 1);
        panes.toggle_zoom();
        panes.split(SplitDirection::Down, 2);
        assert!(!panes.zoomed());
        panes.toggle_zoom();
        assert!(panes.focus_toward(Direction::Up));
        assert!(!panes.zoomed());
    }

    #[test]
    fn closing_the_zoomed_pane_ends_the_zoom() {
        let mut panes = Panes::new(0);
        let second = panes.split(SplitDirection::Right, 1).expect("split");
        panes.split(SplitDirection::Down, 2);
        panes.focus(second);
        panes.toggle_zoom();
        panes.close(second);
        assert!(!panes.zoomed());
    }
}

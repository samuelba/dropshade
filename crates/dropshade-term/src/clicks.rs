//! Counts consecutive clicks so double- and triple-clicks can select words and lines.

use std::time::{Duration, Instant};

use crate::mouse::CellPosition;

const MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(400);
const MAX_CLICKS: u8 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClickTracker {
    last: Option<(Instant, CellPosition, u8)>,
}

impl ClickTracker {
    /// Records a click and returns its count (1–3) together with the updated tracker.
    pub fn register(self, now: Instant, at: CellPosition) -> (u8, Self) {
        let count = match self.last {
            Some((time, position, count))
                if position == at
                    && count < MAX_CLICKS
                    && now.saturating_duration_since(time) <= MULTI_CLICK_INTERVAL =>
            {
                count + 1
            }
            _ => 1,
        };
        (
            count,
            Self {
                last: Some((now, at, count)),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: CellPosition = CellPosition { column: 3, line: 1 };

    #[test]
    fn quick_clicks_on_the_same_cell_count_up_and_wrap() {
        let start = Instant::now();
        let tracker = ClickTracker::default();
        let (first, tracker) = tracker.register(start, HERE);
        let (second, tracker) = tracker.register(start + Duration::from_millis(150), HERE);
        let (third, tracker) = tracker.register(start + Duration::from_millis(300), HERE);
        let (fourth, _) = tracker.register(start + Duration::from_millis(450), HERE);
        assert_eq!((first, second, third, fourth), (1, 2, 3, 1));
    }

    #[test]
    fn slow_clicks_or_moving_restart_the_count() {
        let start = Instant::now();
        let (_, tracker) = ClickTracker::default().register(start, HERE);
        let (slow, _) = tracker.register(start + Duration::from_secs(1), HERE);
        let elsewhere = CellPosition { column: 9, line: 1 };
        let (moved, _) = tracker.register(start + Duration::from_millis(100), elsewhere);
        assert_eq!((slow, moved), (1, 1));
    }
}

//! An overlay scrollbar for the scrollback. It is shown whenever there is scrollback, and widens
//! with a visible track while hovered or dragged.

use iced::widget::canvas::{self, Path};
use iced::{Color, Point, Size, mouse};

use super::Interaction;

const HIT_WIDTH: f32 = 14.0;
const THICKNESS: f32 = 6.0;
const ACTIVE_THICKNESS: f32 = 8.0;
const MARGIN: f32 = 3.0;
const MIN_THUMB: f32 = 24.0;
const THUMB_ALPHA: f32 = 0.35;
const ACTIVE_THUMB_ALPHA: f32 = 0.6;
const TRACK_ALPHA: f32 = 0.08;

/// Where the viewport is within the scrollback, in lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPosition {
    /// Lines of scrollback above the screen.
    pub history: usize,
    /// How far the viewport is scrolled up from the live end.
    pub offset: usize,
    /// Lines on the screen.
    pub screen: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ScrollbarState {
    /// While the thumb is dragged: where it was grabbed, measured from its top.
    grab: Option<f32>,
}

impl ScrollbarState {
    pub(super) fn dragging(self) -> bool {
        self.grab.is_some()
    }
}

/// The scrollbar's layout inside the terminal, in coordinates relative to its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Geometry {
    right: f32,
    height: f32,
    track_top: f32,
    track_height: f32,
    thumb_top: f32,
    thumb_height: f32,
    history: usize,
}

/// What the scrollbar made of a mouse event.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Outcome {
    /// Not meant for the scrollbar; the terminal handles it.
    Ignored,
    /// Consumed by the scrollbar, optionally scrolling the view.
    Captured(Option<Interaction>),
}

/// Lays out the scrollbar, or `None` when there is no scrollback to scroll through.
#[allow(clippy::cast_precision_loss)]
pub(super) fn geometry(size: Size, position: ScrollPosition) -> Option<Geometry> {
    let track_height = size.height - 2.0 * MARGIN;
    if position.history == 0 || track_height <= 0.0 {
        return None;
    }
    let total = (position.history + position.screen) as f32;
    let thumb_height = (track_height * position.screen as f32 / total)
        .max(MIN_THUMB)
        .min(track_height);
    let offset = position.offset.min(position.history);
    let fraction = (position.history - offset) as f32 / position.history as f32;
    Some(Geometry {
        right: size.width,
        height: size.height,
        track_top: MARGIN,
        track_height,
        thumb_top: MARGIN + (track_height - thumb_height) * fraction,
        thumb_height,
        history: position.history,
    })
}

impl Geometry {
    pub(super) fn contains(&self, point: Point) -> bool {
        point.x >= self.right - HIT_WIDTH
            && point.x <= self.right
            && point.y >= 0.0
            && point.y <= self.height
    }

    fn on_thumb(&self, y: f32) -> bool {
        (self.thumb_top..=self.thumb_top + self.thumb_height).contains(&y)
    }

    /// The scroll offset that puts the thumb's top edge at `thumb_top`.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn offset_at(&self, thumb_top: f32) -> usize {
        let travel = self.track_height - self.thumb_height;
        if travel <= 0.0 {
            return 0;
        }
        let fraction = ((thumb_top - self.track_top) / travel).clamp(0.0, 1.0);
        (self.history as f32 * (1.0 - fraction)).round() as usize
    }
}

pub(super) fn handle(
    state: &mut ScrollbarState,
    event: &mouse::Event,
    geometry: Option<Geometry>,
    pointer: Option<Point>,
    current: usize,
) -> Outcome {
    let Some(geometry) = geometry else {
        state.grab = None;
        return Outcome::Ignored;
    };
    match (event, state.grab, pointer) {
        (mouse::Event::ButtonPressed(mouse::Button::Left), None, Some(point))
            if geometry.contains(point) =>
        {
            let grab = if geometry.on_thumb(point.y) {
                point.y - geometry.thumb_top
            } else {
                geometry.thumb_height / 2.0
            };
            state.grab = Some(grab);
            Outcome::Captured(scroll_to(geometry.offset_at(point.y - grab), current))
        }
        (mouse::Event::CursorMoved { .. }, Some(grab), Some(point)) => {
            Outcome::Captured(scroll_to(geometry.offset_at(point.y - grab), current))
        }
        (mouse::Event::ButtonReleased(mouse::Button::Left), Some(_), _) => {
            state.grab = None;
            Outcome::Captured(None)
        }
        (_, Some(_), _) => Outcome::Captured(None),
        _ => Outcome::Ignored,
    }
}

fn scroll_to(offset: usize, current: usize) -> Option<Interaction> {
    (offset != current).then_some(Interaction::ScrollTo(offset))
}

/// Draws the scrollbar; `active` widens it and adds a track while hovered or dragged.
pub(super) fn draw(canvas: &mut canvas::Frame, geometry: Geometry, active: bool, color: Color) {
    let thickness = if active { ACTIVE_THICKNESS } else { THICKNESS };
    let x = geometry.right - MARGIN - thickness;
    let radius = thickness / 2.0;
    if active {
        let track = Path::rounded_rectangle(
            Point::new(x, geometry.track_top),
            Size::new(thickness, geometry.track_height),
            radius.into(),
        );
        canvas.fill(
            &track,
            Color {
                a: TRACK_ALPHA,
                ..color
            },
        );
    }
    let alpha = if active {
        ACTIVE_THUMB_ALPHA
    } else {
        THUMB_ALPHA
    };
    let thumb = Path::rounded_rectangle(
        Point::new(x, geometry.thumb_top),
        Size::new(thickness, geometry.thumb_height),
        radius.into(),
    );
    canvas.fill(&thumb, Color { a: alpha, ..color });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        width: 400.0,
        height: 206.0,
    };

    fn at(history: usize, offset: usize) -> Geometry {
        let position = ScrollPosition {
            history,
            offset,
            screen: 20,
        };
        geometry(SIZE, position).expect("scrollback exists")
    }

    fn press(state: &mut ScrollbarState, geometry: Geometry, y: f32, current: usize) -> Outcome {
        let event = mouse::Event::ButtonPressed(mouse::Button::Left);
        handle(
            state,
            &event,
            Some(geometry),
            Some(Point::new(395.0, y)),
            current,
        )
    }

    fn moved(state: &mut ScrollbarState, geometry: Geometry, y: f32, current: usize) -> Outcome {
        let event = mouse::Event::CursorMoved {
            position: Point::ORIGIN,
        };
        handle(
            state,
            &event,
            Some(geometry),
            Some(Point::new(0.0, y)),
            current,
        )
    }

    #[test]
    fn there_is_no_scrollbar_without_scrollback() {
        let position = ScrollPosition {
            history: 0,
            offset: 0,
            screen: 20,
        };
        assert_eq!(geometry(SIZE, position), None);
    }

    #[test]
    fn the_thumb_sits_at_the_bottom_at_the_live_end_and_at_the_top_when_fully_scrolled() {
        let live = at(180, 0);
        let bottom = live.thumb_top + live.thumb_height;
        assert!((bottom - (SIZE.height - MARGIN)).abs() < 0.01);
        assert!((at(180, 180).thumb_top - MARGIN).abs() < 0.01);
    }

    #[test]
    fn the_thumb_shows_the_visible_share_of_the_output() {
        let geometry = at(80, 0);
        assert!((geometry.thumb_height - geometry.track_height * 0.2).abs() < 0.01);
    }

    #[test]
    fn long_scrollback_keeps_the_thumb_grabbable() {
        assert!((at(1_000_000, 0).thumb_height - MIN_THUMB).abs() < f32::EPSILON);
    }

    #[test]
    fn thumb_position_maps_back_to_the_same_offset() {
        for offset in [0, 1, 45, 90, 179, 180] {
            let geometry = at(180, offset);
            assert_eq!(geometry.offset_at(geometry.thumb_top), offset);
        }
    }

    #[test]
    fn only_the_right_edge_belongs_to_the_scrollbar() {
        let geometry = at(180, 0);
        assert!(geometry.contains(Point::new(390.0, 100.0)));
        assert!(!geometry.contains(Point::new(380.0, 100.0)));
        assert!(!geometry.contains(Point::new(390.0, 210.0)));
    }

    #[test]
    fn clicking_the_track_jumps_there_and_keeps_dragging() {
        let geometry = at(180, 0);
        let mut state = ScrollbarState::default();
        let outcome = press(&mut state, geometry, MARGIN, 0);
        assert_eq!(outcome, Outcome::Captured(Some(Interaction::ScrollTo(180))));
        assert!(state.dragging());
    }

    #[test]
    fn dragging_the_thumb_scrolls_relative_to_where_it_was_grabbed() {
        let geometry = at(180, 0);
        let mut state = ScrollbarState::default();
        let grab_y = geometry.thumb_top + 5.0;
        assert_eq!(
            press(&mut state, geometry, grab_y, 0),
            Outcome::Captured(None)
        );
        let travel = geometry.track_height - geometry.thumb_height;
        let outcome = moved(&mut state, geometry, grab_y - travel / 2.0, 0);
        assert_eq!(outcome, Outcome::Captured(Some(Interaction::ScrollTo(90))));
    }

    #[test]
    fn release_ends_the_drag() {
        let geometry = at(180, 0);
        let mut state = ScrollbarState::default();
        press(&mut state, geometry, geometry.thumb_top + 1.0, 0);
        let release = mouse::Event::ButtonReleased(mouse::Button::Left);
        let outcome = handle(&mut state, &release, Some(geometry), None, 0);
        assert_eq!(outcome, Outcome::Captured(None));
        assert!(!state.dragging());
        assert_eq!(moved(&mut state, geometry, 10.0, 0), Outcome::Ignored);
    }

    #[test]
    fn presses_over_the_text_are_left_to_the_terminal() {
        let geometry = at(180, 0);
        let mut state = ScrollbarState::default();
        let event = mouse::Event::ButtonPressed(mouse::Button::Left);
        let point = Some(Point::new(100.0, 100.0));
        assert_eq!(
            handle(&mut state, &event, Some(geometry), point, 0),
            Outcome::Ignored
        );
    }

    #[test]
    fn losing_the_scrollback_mid_drag_cancels_it() {
        let geometry = at(180, 0);
        let mut state = ScrollbarState::default();
        press(&mut state, geometry, geometry.thumb_top + 1.0, 0);
        let event = mouse::Event::CursorMoved {
            position: Point::ORIGIN,
        };
        assert_eq!(handle(&mut state, &event, None, None, 0), Outcome::Ignored);
        assert!(!state.dragging());
    }
}

//! Turns mouse events into selections, pastes or mouse reports for the application.

use std::time::Instant;

use alacritty_terminal::term::TermMode;
use iced::keyboard::Modifiers;
use iced::{Point, mouse};

use super::Interaction;
use crate::clicks::ClickTracker;
use crate::mouse::{Button, CellPosition, MouseAction, encode_report, reporting_active};
use crate::size::{CellSize, GridSize};
use crate::terminal::SelectionKind;

/// Mouse state the canvas keeps between events.
#[derive(Debug, Default)]
pub struct PointerState {
    pub(super) modifiers: Modifiers,
    pressed: Option<Button>,
    selecting: bool,
    clicks: ClickTracker,
    last_cell: Option<CellPosition>,
}

/// What the pointer handler needs to know about the terminal and the pointer.
#[derive(Debug, Clone, Copy)]
pub(super) struct PointerContext {
    pub mode: TermMode,
    pub grid: GridSize,
    pub cell: CellSize,
    pub padding: f32,
    /// Pointer position relative to the terminal's top-left corner, even outside it.
    pub position: Option<Point>,
    /// The terminal's top-left corner in window coordinates.
    pub origin: Point,
    pub over: bool,
    pub now: Instant,
}

impl PointerContext {
    fn cell_at(&self) -> Option<(CellPosition, crate::size::Side)> {
        let position = self.position?;
        Some(
            self.grid
                .cell_at(position.x, position.y, self.cell, self.padding),
        )
    }
}

pub(super) fn handle(
    state: &mut PointerState,
    event: &mouse::Event,
    ctx: PointerContext,
) -> Option<Interaction> {
    match event {
        mouse::Event::ButtonPressed(button) if ctx.over => press(state, button_of(*button)?, ctx),
        mouse::Event::CursorMoved { .. } => moved(state, ctx),
        mouse::Event::ButtonReleased(button) => release(state, button_of(*button)?, ctx),
        _ => None,
    }
}

/// Reports wheel movement to the application, if it asked for mouse events.
pub(super) fn wheel_report(
    state: &PointerState,
    lines: i32,
    ctx: PointerContext,
) -> Option<Interaction> {
    if !reporting_active(ctx.mode, state.modifiers) {
        return None;
    }
    let (cell, _) = ctx.cell_at()?;
    let action = if lines > 0 {
        MouseAction::WheelUp
    } else {
        MouseAction::WheelDown
    };
    let report = encode_report(action, cell, state.modifiers, ctx.mode)?;
    Some(Interaction::Input(
        report.repeat(lines.unsigned_abs() as usize),
    ))
}

fn press(state: &mut PointerState, button: Button, ctx: PointerContext) -> Option<Interaction> {
    let (cell, side) = ctx.cell_at()?;
    state.last_cell = Some(cell);
    if button == Button::Left && state.modifiers.control() {
        return Some(Interaction::OpenLink(cell));
    }
    if reporting_active(ctx.mode, state.modifiers) {
        state.pressed = Some(button);
        return report(MouseAction::Press(button), cell, state, ctx);
    }
    match button {
        Button::Left => {
            let (clicks, tracker) = state.clicks.register(ctx.now, cell);
            state.clicks = tracker;
            state.selecting = true;
            state.pressed = Some(Button::Left);
            Some(Interaction::Select {
                kind: SelectionKind::from_clicks(clicks),
                at: cell,
                side,
            })
        }
        Button::Middle => Some(Interaction::PastePrimary),
        Button::Right => ctx.position.map(|p| Interaction::ContextMenu {
            position: Point::new(ctx.origin.x + p.x, ctx.origin.y + p.y),
            cell,
        }),
    }
}

fn moved(state: &mut PointerState, ctx: PointerContext) -> Option<Interaction> {
    if !(state.selecting || state.pressed.is_some() || ctx.over) {
        return None;
    }
    let (cell, side) = ctx.cell_at()?;
    if state.selecting {
        state.last_cell = Some(cell);
        return Some(Interaction::ExtendSelection { at: cell, side });
    }
    if state.last_cell == Some(cell) || !reporting_active(ctx.mode, state.modifiers) {
        return None;
    }
    state.last_cell = Some(cell);
    report(MouseAction::Motion(state.pressed), cell, state, ctx)
}

fn release(state: &mut PointerState, button: Button, ctx: PointerContext) -> Option<Interaction> {
    if state.selecting && button == Button::Left {
        state.selecting = false;
        state.pressed = None;
        return Some(Interaction::FinishSelection);
    }
    if state.pressed != Some(button) {
        return None;
    }
    state.pressed = None;
    let cell = ctx.cell_at().map(|(cell, _)| cell).or(state.last_cell)?;
    report(MouseAction::Release(button), cell, state, ctx)
}

fn report(
    action: MouseAction,
    cell: CellPosition,
    state: &PointerState,
    ctx: PointerContext,
) -> Option<Interaction> {
    encode_report(action, cell, state.modifiers, ctx.mode).map(Interaction::Input)
}

fn button_of(button: mouse::Button) -> Option<Button> {
    match button {
        mouse::Button::Left => Some(Button::Left),
        mouse::Button::Middle => Some(Button::Middle),
        mouse::Button::Right => Some(Button::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size::Side;
    use std::time::Duration;

    const CELL: CellSize = CellSize {
        width: 10.0,
        height: 20.0,
    };
    const GRID: GridSize = GridSize {
        columns: 20,
        lines: 10,
    };

    fn ctx(x: f32, y: f32, mode: TermMode, now: Instant) -> PointerContext {
        PointerContext {
            mode,
            grid: GRID,
            cell: CELL,
            padding: 0.0,
            position: Some(Point::new(x, y)),
            origin: Point::new(100.0, 40.0),
            over: true,
            now,
        }
    }

    fn pressed(button: mouse::Button) -> mouse::Event {
        mouse::Event::ButtonPressed(button)
    }

    fn released(button: mouse::Button) -> mouse::Event {
        mouse::Event::ButtonReleased(button)
    }

    fn moved_to() -> mouse::Event {
        mouse::Event::CursorMoved {
            position: Point::ORIGIN,
        }
    }

    #[test]
    fn click_drag_release_selects_and_finishes() {
        let now = Instant::now();
        let mut state = PointerState::default();
        let mode = TermMode::empty();
        let start = handle(
            &mut state,
            &pressed(mouse::Button::Left),
            ctx(12.0, 5.0, mode, now),
        );
        assert_eq!(
            start,
            Some(Interaction::Select {
                kind: SelectionKind::Simple,
                at: CellPosition { column: 1, line: 0 },
                side: Side::Left,
            })
        );
        let drag = handle(&mut state, &moved_to(), ctx(48.0, 25.0, mode, now));
        assert_eq!(
            drag,
            Some(Interaction::ExtendSelection {
                at: CellPosition { column: 4, line: 1 },
                side: Side::Right,
            })
        );
        let end = handle(
            &mut state,
            &released(mouse::Button::Left),
            ctx(48.0, 25.0, mode, now),
        );
        assert_eq!(end, Some(Interaction::FinishSelection));
        assert_eq!(
            handle(&mut state, &moved_to(), ctx(60.0, 25.0, mode, now)),
            None
        );
    }

    #[test]
    fn double_click_selects_words() {
        let now = Instant::now();
        let mut state = PointerState::default();
        let at = |t| ctx(12.0, 5.0, TermMode::empty(), t);
        handle(&mut state, &pressed(mouse::Button::Left), at(now));
        handle(&mut state, &released(mouse::Button::Left), at(now));
        let second = handle(
            &mut state,
            &pressed(mouse::Button::Left),
            at(now + Duration::from_millis(100)),
        );
        assert!(matches!(
            second,
            Some(Interaction::Select {
                kind: SelectionKind::Word,
                ..
            })
        ));
    }

    #[test]
    fn ctrl_click_opens_the_link_under_the_pointer_even_in_mouse_mode() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        let mut state = PointerState {
            modifiers: Modifiers::CTRL,
            ..PointerState::default()
        };
        let press = handle(
            &mut state,
            &pressed(mouse::Button::Left),
            ctx(12.0, 25.0, mode, Instant::now()),
        );
        assert_eq!(
            press,
            Some(Interaction::OpenLink(CellPosition { column: 1, line: 1 }))
        );
        let release = handle(
            &mut state,
            &released(mouse::Button::Left),
            ctx(12.0, 25.0, mode, Instant::now()),
        );
        assert_eq!(release, None);
    }

    #[test]
    fn right_click_opens_the_context_menu_in_window_coordinates() {
        let mut state = PointerState::default();
        let result = handle(
            &mut state,
            &pressed(mouse::Button::Right),
            ctx(12.0, 5.0, TermMode::empty(), Instant::now()),
        );
        let Some(Interaction::ContextMenu { position, .. }) = result else {
            panic!("expected a context menu, got {result:?}");
        };
        assert_eq!(position, Point::new(112.0, 45.0));
    }

    #[test]
    fn middle_click_pastes_the_primary_selection() {
        let mut state = PointerState::default();
        let result = handle(
            &mut state,
            &pressed(mouse::Button::Middle),
            ctx(0.0, 0.0, TermMode::empty(), Instant::now()),
        );
        assert_eq!(result, Some(Interaction::PastePrimary));
    }

    #[test]
    fn mouse_mode_reports_instead_of_selecting() {
        let now = Instant::now();
        let mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        let mut state = PointerState::default();
        let press = handle(
            &mut state,
            &pressed(mouse::Button::Left),
            ctx(12.0, 5.0, mode, now),
        );
        assert_eq!(press, Some(Interaction::Input(b"\x1b[<0;2;1M".to_vec())));
        let release = handle(
            &mut state,
            &released(mouse::Button::Left),
            ctx(12.0, 5.0, mode, now),
        );
        assert_eq!(release, Some(Interaction::Input(b"\x1b[<0;2;1m".to_vec())));
    }

    #[test]
    fn shift_selects_even_in_mouse_mode() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        let mut state = PointerState {
            modifiers: Modifiers::SHIFT,
            ..PointerState::default()
        };
        let press = handle(
            &mut state,
            &pressed(mouse::Button::Left),
            ctx(12.0, 5.0, mode, Instant::now()),
        );
        assert!(matches!(press, Some(Interaction::Select { .. })));
    }

    #[test]
    fn motion_is_reported_once_per_cell() {
        let now = Instant::now();
        let mode = TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE;
        let mut state = PointerState::default();
        let first = handle(&mut state, &moved_to(), ctx(12.0, 5.0, mode, now));
        assert_eq!(first, Some(Interaction::Input(b"\x1b[<35;2;1M".to_vec())));
        assert_eq!(
            handle(&mut state, &moved_to(), ctx(15.0, 6.0, mode, now)),
            None
        );
    }

    #[test]
    fn wheel_is_reported_per_line_in_mouse_mode() {
        let mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        let state = PointerState::default();
        let report = wheel_report(&state, 2, ctx(0.0, 0.0, mode, Instant::now()));
        assert_eq!(
            report,
            Some(Interaction::Input(b"\x1b[<64;1;1M\x1b[<64;1;1M".to_vec()))
        );
        assert_eq!(
            wheel_report(&state, 1, ctx(0.0, 0.0, TermMode::empty(), Instant::now())),
            None
        );
    }
}

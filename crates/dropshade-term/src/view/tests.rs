//! Tests for the terminal view's event handling.

use super::*;
use crate::terminal::SpawnOptions;
use canvas::Program;
use iced::Size;
use iced::event::Status;
use iced::keyboard::key::{NativeCode, Physical};
use iced::keyboard::{Key, Location, Modifiers};
use std::time::Instant;

const CELL: CellSize = CellSize {
    width: 8.0,
    height: 16.0,
};

fn style() -> ViewStyle {
    ViewStyle {
        palette: Palette::default(),
        font: Font::MONOSPACE,
        font_size: 13.0,
        cell: CELL,
        padding: 10.0,
        background_opacity: 1.0,
        scrollbar: true,
    }
}

fn terminal() -> Terminal {
    let options = SpawnOptions {
        command: Some(("sh".into(), vec!["-c".into(), "sleep 5".into()])),
        size: GridSize {
            columns: 80,
            lines: 24,
        },
        cell: CELL,
        ..SpawnOptions::default()
    };
    Terminal::spawn(options).expect("spawn sh").0
}

fn update(
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<(Interaction, Status)> {
    let terminal = terminal();
    let style = style();
    let input = InputConfig::default();
    let view = TerminalView::new(&terminal, &style, &input, |interaction| interaction);
    let (message, _, status) = view
        .update(&mut ViewState::default(), event, bounds, cursor)?
        .into_inner();
    message.map(|message| (message, status))
}

fn bounds(width: f32, height: f32) -> Rectangle {
    Rectangle::new(Point::ORIGIN, Size::new(width, height))
}

#[test]
fn key_presses_become_captured_input() {
    let event = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("a".into()),
        modified_key: Key::Character("a".into()),
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: Location::Standard,
        modifiers: Modifiers::empty(),
        text: Some("a".into()),
        repeat: false,
    });
    let result = update(&event, bounds(660.0, 404.0), mouse::Cursor::Unavailable);
    assert_eq!(
        result,
        Some((Interaction::Input(b"a".to_vec()), Status::Captured))
    );
}

#[test]
fn redraw_requests_a_resize_when_the_grid_no_longer_fits() {
    let event = Event::Window(window::Event::RedrawRequested(Instant::now()));
    let resized = update(&event, bounds(820.0, 340.0), mouse::Cursor::Unavailable);
    let expected = Interaction::Resize(GridSize {
        columns: 100,
        lines: 20,
    });
    assert_eq!(resized, Some((expected, Status::Ignored)));

    assert_eq!(
        update(&event, bounds(660.0, 404.0), mouse::Cursor::Unavailable),
        None
    );
}

#[test]
fn wheel_scrolls_only_over_the_terminal() {
    let event = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
    });
    let inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
    let scrolled = update(&event, bounds(660.0, 404.0), inside);
    assert_eq!(scrolled, Some((Interaction::Scroll(3), Status::Captured)));

    let outside = mouse::Cursor::Available(Point::new(900.0, 50.0));
    assert_eq!(update(&event, bounds(660.0, 404.0), outside), None);
}

#[test]
fn pixel_scrolling_is_converted_to_lines() {
    let event = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -32.0 },
    });
    let inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
    let scrolled = update(&event, bounds(660.0, 404.0), inside);
    assert_eq!(scrolled, Some((Interaction::Scroll(-2), Status::Captured)));
}

#[test]
fn alternate_scroll_sends_arrows_matching_cursor_mode() {
    assert_eq!(
        alternate_scroll(2, TermMode::empty()),
        b"\x1b[A\x1b[A".to_vec()
    );
    assert_eq!(
        alternate_scroll(-1, TermMode::APP_CURSOR),
        b"\x1bOB".to_vec()
    );
}

fn with_view(check: impl FnOnce(&TerminalView<'_, Interaction>)) {
    let terminal = terminal();
    let style = style();
    let input = InputConfig::default();
    check(&TerminalView::new(
        &terminal,
        &style,
        &input,
        |interaction| interaction,
    ));
}

#[test]
fn window_focus_is_tracked() {
    with_view(|view| {
        let mut state = ViewState::default();
        let unfocused = Event::Window(window::Event::Unfocused);
        view.update(
            &mut state,
            &unfocused,
            bounds(660.0, 404.0),
            mouse::Cursor::Unavailable,
        );
        assert!(!state.focused);
        let focused = Event::Window(window::Event::Focused);
        view.update(
            &mut state,
            &focused,
            bounds(660.0, 404.0),
            mouse::Cursor::Unavailable,
        );
        assert!(state.focused);
    });
}

#[test]
fn input_method_preedit_is_shown_and_commit_is_sent() {
    with_view(|view| {
        let mut state = ViewState::default();
        let area = bounds(660.0, 404.0);
        let preedit = Event::InputMethod(input_method::Event::Preedit("にほ".into(), None));
        view.update(&mut state, &preedit, area, mouse::Cursor::Unavailable);
        assert_eq!(state.preedit.as_deref(), Some("にほ"));

        let commit = Event::InputMethod(input_method::Event::Commit("日本".into()));
        let (message, _, status) = view
            .update(&mut state, &commit, area, mouse::Cursor::Unavailable)
            .expect("commit produces input")
            .into_inner();
        assert_eq!(
            message,
            Some(Interaction::Input("日本".as_bytes().to_vec()))
        );
        assert_eq!(status, Status::Captured);
        assert_eq!(state.preedit, None);
    });
}

#[test]
fn ime_cursor_tracks_the_terminal_cursor() {
    with_view(|view| {
        let area = Rectangle::new(Point::new(100.0, 50.0), Size::new(660.0, 404.0));
        let rect = view.ime_cursor(area).expect("cursor visible");
        assert!(rect.x >= 110.0 && rect.y >= 60.0);
        assert!((rect.width - CELL.width).abs() < f32::EPSILON);
    });
}

#[test]
fn keys_are_ignored_while_input_is_disabled() {
    let terminal = terminal();
    let style = style();
    let input = InputConfig::default();
    let view = TerminalView::new(&terminal, &style, &input, |interaction| interaction)
        .accepts_input(false);
    let event = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("a".into()),
        modified_key: Key::Character("a".into()),
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: Location::Standard,
        modifiers: Modifiers::empty(),
        text: Some("a".into()),
        repeat: false,
    });
    let result = view.update(
        &mut ViewState::default(),
        &event,
        bounds(660.0, 404.0),
        mouse::Cursor::Unavailable,
    );
    assert!(result.is_none());
}

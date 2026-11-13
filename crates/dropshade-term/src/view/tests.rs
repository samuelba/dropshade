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
    update_with(
        &terminal(),
        &mut ViewState::default(),
        event,
        bounds,
        cursor,
    )
}

fn update_with(
    terminal: &Terminal,
    state: &mut ViewState,
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<(Interaction, Status)> {
    let style = style();
    let input = InputConfig::default();
    let view = TerminalView::new(terminal, &style, &input, |interaction| interaction);
    let (message, _, status) = view.update(state, event, bounds, cursor)?.into_inner();
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

#[test]
fn input_after_unfocus_restores_focus() {
    with_view(|view| {
        let area = bounds(660.0, 404.0);
        let unfocused = Event::Window(window::Event::Unfocused);
        let inputs = [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character("a".into()),
                modified_key: keyboard::Key::Character("a".into()),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: Some("a".into()),
                repeat: false,
            }),
        ];
        for input in inputs {
            let mut state = ViewState::default();
            view.update(&mut state, &unfocused, area, mouse::Cursor::Unavailable);
            assert!(!state.focused);
            view.update(&mut state, &input, area, mouse::Cursor::Unavailable);
            assert!(state.focused, "{input:?} should restore focus");
        }
    });
}

fn key_event(key: Key, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

/// A key press in vi mode: what it published, if anything, and whether it was captured.
fn vi_key(
    terminal: &Terminal,
    state: &mut ViewState,
    event: &Event,
) -> (Option<Interaction>, Status) {
    let style = style();
    let input = InputConfig::default();
    let view = TerminalView::new(terminal, &style, &input, |interaction| interaction);
    let action = view
        .update(
            state,
            event,
            bounds(660.0, 404.0),
            mouse::Cursor::Unavailable,
        )
        .expect("vi mode handles every key");
    let (message, _, status) = action.into_inner();
    (message, status)
}

#[test]
fn ctrl_shift_space_toggles_vi_mode() {
    let event = key_event(
        Key::Named(keyboard::key::Named::Space),
        Modifiers::CTRL | Modifiers::SHIFT,
        Some(" "),
    );
    let result = update(&event, bounds(660.0, 404.0), mouse::Cursor::Unavailable);
    assert_eq!(
        result,
        Some((
            Interaction::Shortcut(input::Shortcut::ToggleViMode),
            Status::Captured
        ))
    );
}

#[test]
fn vi_mode_keeps_keys_from_the_shell() {
    let mut terminal = terminal();
    terminal.toggle_vi_mode();
    let mut state = ViewState::default();

    let x = key_event(Key::Character("x".into()), Modifiers::empty(), Some("x"));
    assert_eq!(vi_key(&terminal, &mut state, &x), (None, Status::Captured));

    let j = key_event(Key::Character("j".into()), Modifiers::empty(), Some("j"));
    assert_eq!(
        vi_key(&terminal, &mut state, &j),
        (
            Some(Interaction::Vi(ViAction::Motion(
                alacritty_terminal::vi_mode::ViMotion::Down
            ))),
            Status::Captured
        )
    );

    let g = key_event(Key::Character("g".into()), Modifiers::empty(), Some("g"));
    assert_eq!(vi_key(&terminal, &mut state, &g), (None, Status::Captured));
    assert_eq!(
        vi_key(&terminal, &mut state, &g).0,
        Some(Interaction::Vi(ViAction::Top))
    );
}

#[test]
fn shortcuts_still_work_in_vi_mode() {
    let mut terminal = terminal();
    terminal.toggle_vi_mode();
    let copy = key_event(
        Key::Character("c".into()),
        Modifiers::CTRL | Modifiers::SHIFT,
        Some("C"),
    );
    assert_eq!(
        vi_key(&terminal, &mut ViewState::default(), &copy).0,
        Some(Interaction::Shortcut(input::Shortcut::Copy))
    );
}

fn key_press(key: Key, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

fn press_with(view: &TerminalView<'_, Interaction>, event: &Event) -> Option<Interaction> {
    let bounds = bounds(660.0, 404.0);
    let (message, _, _) = view
        .update(
            &mut ViewState::default(),
            event,
            bounds,
            mouse::Cursor::Unavailable,
        )?
        .into_inner();
    message
}

#[test]
fn pane_keys_reach_the_program_in_a_tab_without_splits() {
    let terminal = terminal();
    let (style, input) = (style(), InputConfig::default());
    let alt_left = key_press(Key::Named(keyboard::key::Named::ArrowLeft), Modifiers::ALT);
    let lone = TerminalView::new(&terminal, &style, &input, |interaction| interaction);
    assert!(
        matches!(press_with(&lone, &alt_left), Some(Interaction::Input(_))),
        "Alt+Left is encoded for the shell"
    );
    let split = TerminalView::new(&terminal, &style, &input, |interaction| interaction).split(true);
    assert_eq!(
        press_with(&split, &alt_left),
        Some(Interaction::Shortcut(Shortcut::FocusPane(
            input::PaneDirection::Left
        )))
    );
    let split_right = key_press(
        Key::Character("r".into()),
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(
        press_with(&lone, &split_right),
        Some(Interaction::Shortcut(Shortcut::SplitRight)),
        "splitting works in a lone pane"
    );
}

#[test]
fn panes_without_the_keyboard_ignore_keys() {
    let terminal = terminal();
    let (style, input) = (style(), InputConfig::default());
    let view =
        TerminalView::new(&terminal, &style, &input, |interaction| interaction).active(false);
    let a = key_press(Key::Character("a".into()), Modifiers::empty());
    assert_eq!(press_with(&view, &a), None);
}

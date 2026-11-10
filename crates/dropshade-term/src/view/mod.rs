//! The iced canvas program that draws a terminal and turns user input into interactions.

mod draw;
mod pointer;
mod widget;

use std::time::Instant;

use alacritty_terminal::term::TermMode;
use iced::advanced::input_method;
use iced::widget::canvas::{self, Action, Event, Geometry};
use iced::{Font, Point, Rectangle, Renderer, Theme, keyboard, mouse, window};

use crate::colors::Palette;
use crate::input::{self, InputConfig, KeyPress, Shortcut};
use crate::mouse::CellPosition;
use crate::size::{CellSize, GridSize, Side};
use crate::terminal::{SelectionKind, Terminal};

pub use pointer::PointerState;
pub use widget::TerminalWidget;

const WHEEL_LINES_PER_NOTCH: f32 = 3.0;

/// Visual settings shared by every terminal view.
#[derive(Debug, Clone)]
pub struct ViewStyle {
    pub palette: Palette,
    pub font: Font,
    pub font_size: f32,
    pub cell: CellSize,
    pub padding: f32,
    pub background_opacity: f32,
}

/// Something the user did that the application should apply to the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interaction {
    Input(Vec<u8>),
    Resize(GridSize),
    Scroll(i32),
    Shortcut(Shortcut),
    Select {
        kind: SelectionKind,
        at: CellPosition,
        side: Side,
    },
    ExtendSelection {
        at: CellPosition,
        side: Side,
    },
    /// The mouse button was released after selecting; the selection becomes the primary one.
    FinishSelection,
    PastePrimary,
    /// Ctrl+click: open the link under this cell, if there is one.
    OpenLink(CellPosition),
}

/// Per-widget state kept by iced between events.
#[derive(Debug)]
pub struct ViewState {
    pointer: PointerState,
    focused: bool,
    /// Text the input method is composing, shown at the cursor until it is committed.
    preedit: Option<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            pointer: PointerState::default(),
            focused: true,
            preedit: None,
        }
    }
}

pub struct TerminalView<'a, Message> {
    terminal: &'a Terminal,
    style: &'a ViewStyle,
    input: &'a InputConfig,
    on_interaction: fn(Interaction) -> Message,
    cursor_visible: bool,
    bell: bool,
    accepts_input: bool,
}

impl<Message> std::fmt::Debug for TerminalView<'_, Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalView")
            .field("terminal", self.terminal)
            .finish_non_exhaustive()
    }
}

impl<'a, Message> TerminalView<'a, Message> {
    pub fn new(
        terminal: &'a Terminal,
        style: &'a ViewStyle,
        input: &'a InputConfig,
        on_interaction: fn(Interaction) -> Message,
    ) -> Self {
        Self {
            terminal,
            style,
            input,
            on_interaction,
            cursor_visible: true,
            bell: false,
            accepts_input: true,
        }
    }

    /// Stops keyboard and IME input from reaching the terminal, e.g. while a dialog is open.
    #[must_use]
    pub fn accepts_input(self, accepts_input: bool) -> Self {
        Self {
            accepts_input,
            ..self
        }
    }

    /// Hides the cursor for the "off" phase of blinking.
    #[must_use]
    pub fn cursor_visible(self, cursor_visible: bool) -> Self {
        Self {
            cursor_visible,
            ..self
        }
    }

    /// Flashes the terminal for the visual bell.
    #[must_use]
    pub fn bell(self, bell: bool) -> Self {
        Self { bell, ..self }
    }

    /// Where the input method should anchor its candidate window, in window coordinates.
    fn ime_cursor(&self, bounds: Rectangle) -> Option<Rectangle> {
        let cursor = self.terminal.cursor_position()?;
        let cell = self.style.cell;
        #[allow(clippy::cast_precision_loss)]
        Some(Rectangle {
            x: bounds.x + self.style.padding + cursor.column as f32 * cell.width,
            y: bounds.y + self.style.padding + cursor.line as f32 * cell.height,
            width: cell.width,
            height: cell.height,
        })
    }

    fn input_method_event(
        &self,
        state: &mut ViewState,
        event: &input_method::Event,
    ) -> Action<Message> {
        match event {
            input_method::Event::Preedit(text, _) => {
                state.preedit = (!text.is_empty()).then(|| text.clone());
                Action::request_redraw().and_capture()
            }
            input_method::Event::Commit(text) => {
                state.preedit = None;
                let input = Interaction::Input(text.as_bytes().to_vec());
                Action::publish((self.on_interaction)(input)).and_capture()
            }
            input_method::Event::Opened | input_method::Event::Closed => {
                state.preedit = None;
                Action::request_redraw()
            }
        }
    }

    fn grid_size_for(&self, bounds: Rectangle) -> GridSize {
        let inner = 2.0 * self.style.padding;
        GridSize::fitting(bounds.width - inner, bounds.height - inner, self.style.cell)
    }

    fn pointer_context(&self, bounds: Rectangle, cursor: mouse::Cursor) -> pointer::PointerContext {
        pointer::PointerContext {
            mode: self.terminal.mode(),
            grid: self.terminal.size(),
            cell: self.style.cell,
            padding: self.style.padding,
            position: cursor
                .position()
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y)),
            over: cursor.is_over(bounds),
            now: Instant::now(),
        }
    }

    fn key_pressed(&self, press: KeyPress<'_>) -> Option<Interaction> {
        if let Some(shortcut) = input::shortcut(press.key, press.modifiers) {
            return Some(Interaction::Shortcut(shortcut));
        }
        input::encode(press, self.terminal.mode(), self.input).map(Interaction::Input)
    }

    fn wheel(
        &self,
        state: &PointerState,
        delta: mouse::ScrollDelta,
        ctx: pointer::PointerContext,
    ) -> Option<Interaction> {
        #[allow(clippy::cast_possible_truncation)]
        let lines = match delta {
            mouse::ScrollDelta::Lines { y, .. } => (y * WHEEL_LINES_PER_NOTCH).round() as i32,
            mouse::ScrollDelta::Pixels { y, .. } => (y / self.style.cell.height).round() as i32,
        };
        if lines == 0 {
            return None;
        }
        if let Some(report) = pointer::wheel_report(state, lines, ctx) {
            return Some(report);
        }
        let mode = self.terminal.mode();
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            return Some(Interaction::Input(alternate_scroll(lines, mode)));
        }
        Some(Interaction::Scroll(lines))
    }
}

impl<Message> canvas::Program<Message> for TerminalView<'_, Message> {
    type State = ViewState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Keyboard(keyboard::Event::KeyPressed { .. }) | Event::InputMethod(_)
                if !self.accepts_input =>
            {
                None
            }
            Event::Window(window::Event::RedrawRequested(_) | window::Event::Resized(_)) => {
                let size = self.grid_size_for(bounds);
                (size != self.terminal.size())
                    .then(|| Action::publish((self.on_interaction)(Interaction::Resize(size))))
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => {
                let press = KeyPress {
                    key,
                    modifiers: *modifiers,
                    text: text.as_deref(),
                };
                let interaction = self.key_pressed(press)?;
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.pointer.modifiers = *modifiers;
                None
            }
            Event::Window(window::Event::Focused | window::Event::Unfocused) => {
                state.focused = matches!(event, Event::Window(window::Event::Focused));
                Some(Action::request_redraw())
            }
            Event::InputMethod(ime_event) => Some(self.input_method_event(state, ime_event)),
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let ctx = self.pointer_context(bounds, cursor);
                let interaction = self.wheel(&state.pointer, *delta, ctx)?;
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
            Event::Mouse(mouse_event) => {
                let ctx = self.pointer_context(bounds, cursor);
                let interaction = pointer::handle(&mut state.pointer, mouse_event, ctx)?;
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if state.pointer.modifiers.control() {
            let (cell, _) = self.terminal.size().cell_at(
                position.x,
                position.y,
                self.style.cell,
                self.style.padding,
            );
            if self.terminal.link_at(cell).is_some() {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::Text
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![self.render(state, renderer, bounds)]
    }
}

fn alternate_scroll(lines: i32, mode: TermMode) -> Vec<u8> {
    let arrow: &[u8] = match (lines > 0, mode.contains(TermMode::APP_CURSOR)) {
        (true, true) => b"\x1bOA",
        (true, false) => b"\x1b[A",
        (false, true) => b"\x1bOB",
        (false, false) => b"\x1b[B",
    };
    arrow.repeat(lines.unsigned_abs() as usize)
}

#[cfg(test)]
mod tests {
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
}

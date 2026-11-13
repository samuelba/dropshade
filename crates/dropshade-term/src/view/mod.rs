//! The iced canvas program that draws a terminal and turns user input into interactions.

mod draw;
mod pointer;
mod scrollbar;
mod widget;

use std::time::Instant;

use alacritty_terminal::term::TermMode;
use iced::advanced::input_method;
use iced::widget::canvas::{self, Action, Event, Geometry};
use iced::{Font, Point, Rectangle, Renderer, Theme, keyboard, mouse, window};

use crate::colors::Palette;
use crate::input::{self, InputConfig, KeyPress, Resolution, Shortcut};
use crate::mouse::CellPosition;
use crate::size::{CellSize, GridSize, Side};
use crate::terminal::{SelectionKind, Terminal};
use crate::vi::{self, ViAction};

pub use pointer::PointerState;
pub use scrollbar::ScrollPosition;
pub use widget::TerminalWidget;

use scrollbar::{Outcome, ScrollbarState};

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
    pub scrollbar: bool,
}

/// Something the user did that the application should apply to the terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum Interaction {
    Input(Vec<u8>),
    Resize(GridSize),
    Scroll(i32),
    /// Scroll so the viewport is this many lines above the live end (from the scrollbar).
    ScrollTo(usize),
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
    /// Right-click outside mouse reporting: open the context menu at this window position, for
    /// the clicked cell.
    ContextMenu {
        position: Point,
        cell: CellPosition,
    },
    /// A key pressed in vi mode.
    Vi(ViAction),
}

/// Per-widget state kept by iced between events.
#[derive(Debug)]
pub struct ViewState {
    pointer: PointerState,
    scrollbar: ScrollbarState,
    focused: bool,
    /// Text the input method is composing, shown at the cursor until it is committed.
    preedit: Option<String>,
    /// The first key of a two-key vi command (`gg`).
    vi_pending: Option<char>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            pointer: PointerState::default(),
            scrollbar: ScrollbarState::default(),
            focused: true,
            preedit: None,
            vi_pending: None,
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
        if self.terminal.vi_mode() {
            state.preedit = None;
            return Action::capture();
        }
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
            origin: bounds.position(),
            over: cursor.is_over(bounds),
            now: Instant::now(),
        }
    }

    fn scrollbar(&self, bounds: Rectangle) -> Option<scrollbar::Geometry> {
        if !self.style.scrollbar {
            return None;
        }
        scrollbar::geometry(bounds.size(), self.terminal.scroll_position())
    }

    fn mouse_event(
        &self,
        state: &mut ViewState,
        event: &mouse::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let ctx = self.pointer_context(bounds, cursor);
        let current = self.terminal.scroll_position().offset;
        let geometry = self.scrollbar(bounds);
        match scrollbar::handle(&mut state.scrollbar, event, geometry, ctx.position, current) {
            Outcome::Captured(Some(interaction)) => {
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
            Outcome::Captured(None) => Some(Action::request_redraw().and_capture()),
            Outcome::Ignored => {
                let interaction = pointer::handle(&mut state.pointer, event, ctx)?;
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
        }
    }

    fn key_pressed(&self, state: &mut ViewState, press: KeyPress<'_>) -> Option<Interaction> {
        let resolution = input::resolve(press.key, press.modifiers, &self.input.bindings);
        if let Resolution::Shortcut(shortcut) = resolution {
            state.vi_pending = None;
            return Some(Interaction::Shortcut(shortcut));
        }
        if self.terminal.vi_mode() {
            return vi::resolve(press, &mut state.vi_pending).map(Interaction::Vi);
        }
        state.vi_pending = None;
        match resolution {
            Resolution::Shortcut(shortcut) => Some(Interaction::Shortcut(shortcut)),
            Resolution::Send(bytes) => Some(Interaction::Input(bytes)),
            Resolution::Encode => {
                input::encode(press, self.terminal.mode(), self.input).map(Interaction::Input)
            }
        }
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

impl<Message> TerminalView<'_, Message> {
    fn handle_event(
        &self,
        state: &mut ViewState,
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
                match self.key_pressed(state, press) {
                    Some(interaction) => {
                        Some(Action::publish((self.on_interaction)(interaction)).and_capture())
                    }
                    // In vi mode every key stays here, even the ones that do nothing.
                    None if self.terminal.vi_mode() => Some(Action::capture()),
                    None => None,
                }
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
            Event::Mouse(mouse_event) => self.mouse_event(state, mouse_event, bounds, cursor),
            _ => None,
        }
    }
}

/// Input only reaches a surface that has the keyboard focus. layershellev 0.19 sends no `Focused`
/// when the keyboard re-enters the surface it last left, so input stands in for it there.
fn implies_focus(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | Event::Mouse(mouse::Event::ButtonPressed(_))
    )
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
        let refocused = !state.focused && implies_focus(event);
        if refocused {
            state.focused = true;
        }
        let action = self.handle_event(state, event, bounds, cursor);
        if refocused {
            action.or_else(|| Some(Action::request_redraw()))
        } else {
            action
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.scrollbar.dragging() {
            return mouse::Interaction::default();
        }
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if self
            .scrollbar(bounds)
            .is_some_and(|geometry| geometry.contains(position))
        {
            return mouse::Interaction::default();
        }
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
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![self.render(state, renderer, bounds, cursor)]
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
mod tests;

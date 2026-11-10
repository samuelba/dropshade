//! The iced canvas program that draws a terminal and turns user input into interactions.

use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;
use iced::widget::canvas::{self, Action, Event, Geometry, Stroke, Text};
use iced::widget::text::{LineHeight, Shaping};
use iced::{
    Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, font, keyboard, mouse, window,
};

use crate::colors::{Palette, to_iced};
use crate::frame::{Cursor, Run};
use crate::input::{self, InputConfig, KeyPress};
use crate::size::{CellSize, GridSize};
use crate::terminal::Terminal;

const WHEEL_LINES_PER_NOTCH: f32 = 3.0;
const BAR_THICKNESS: f32 = 2.0;
const DECORATION_THICKNESS: f32 = 1.0;

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
}

pub struct TerminalView<'a, Message> {
    terminal: &'a Terminal,
    style: &'a ViewStyle,
    input: &'a InputConfig,
    on_interaction: fn(Interaction) -> Message,
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
        }
    }

    fn grid_size_for(&self, bounds: Rectangle) -> GridSize {
        let inner = 2.0 * self.style.padding;
        GridSize::fitting(bounds.width - inner, bounds.height - inner, self.style.cell)
    }

    fn wheel(&self, delta: mouse::ScrollDelta) -> Option<Interaction> {
        #[allow(clippy::cast_possible_truncation)]
        let lines = match delta {
            mouse::ScrollDelta::Lines { y, .. } => (y * WHEEL_LINES_PER_NOTCH).round() as i32,
            mouse::ScrollDelta::Pixels { y, .. } => (y / self.style.cell.height).round() as i32,
        };
        if lines == 0 {
            return None;
        }
        let mode = self.terminal.mode();
        if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            return Some(Interaction::Input(alternate_scroll(lines, mode)));
        }
        Some(Interaction::Scroll(lines))
    }

    fn origin(&self, row: usize, column: usize) -> Point {
        let cell = self.style.cell;
        #[allow(clippy::cast_precision_loss)]
        Point::new(
            self.style.padding + column as f32 * cell.width,
            self.style.padding + row as f32 * cell.height,
        )
    }

    fn draw_run(&self, canvas: &mut canvas::Frame, run: &Run) {
        let cell = self.style.cell;
        let origin = self.origin(run.row, run.column);
        #[allow(clippy::cast_precision_loss)]
        let width = run.width as f32 * cell.width;
        if run.colors.bg != self.style.palette.background {
            canvas.fill_rectangle(
                origin,
                Size::new(width, cell.height),
                to_iced(run.colors.bg, 1.0),
            );
        }
        let fg = to_iced(run.colors.fg, 1.0);
        canvas.fill_text(self.text(&run.text, origin, fg, run.style.bold, run.style.italic));
        if run.style.underline {
            let y = origin.y + cell.height - DECORATION_THICKNESS;
            canvas.fill_rectangle(
                Point::new(origin.x, y),
                Size::new(width, DECORATION_THICKNESS),
                fg,
            );
        }
        if run.style.strikeout {
            let y = origin.y + (cell.height / 2.0).round();
            canvas.fill_rectangle(
                Point::new(origin.x, y),
                Size::new(width, DECORATION_THICKNESS),
                fg,
            );
        }
    }

    fn draw_cursor(&self, canvas: &mut canvas::Frame, cursor: Cursor, runs: &[Run]) {
        let cell = self.style.cell;
        let origin = self.origin(cursor.row, cursor.column);
        let width = if cursor.wide {
            cell.width * 2.0
        } else {
            cell.width
        };
        let color = to_iced(self.style.palette.cursor, 1.0);
        match cursor.shape {
            CursorShape::Block => {
                canvas.fill_rectangle(origin, Size::new(width, cell.height), color);
                if let Some((c, bold, italic)) = char_at(runs, cursor) {
                    let bg = to_iced(self.style.palette.background, 1.0);
                    canvas.fill_text(self.text(&c.to_string(), origin, bg, bold, italic));
                }
            }
            CursorShape::Beam => {
                canvas.fill_rectangle(origin, Size::new(BAR_THICKNESS, cell.height), color);
            }
            CursorShape::Underline => {
                let y = origin.y + cell.height - BAR_THICKNESS;
                canvas.fill_rectangle(
                    Point::new(origin.x, y),
                    Size::new(width, BAR_THICKNESS),
                    color,
                );
            }
            CursorShape::HollowBlock => {
                let stroke = Stroke::default()
                    .with_color(color)
                    .with_width(DECORATION_THICKNESS);
                canvas.stroke_rectangle(origin, Size::new(width, cell.height), stroke);
            }
            CursorShape::Hidden => {}
        }
    }

    fn text(&self, content: &str, position: Point, color: Color, bold: bool, italic: bool) -> Text {
        let font = Font {
            weight: if bold {
                font::Weight::Bold
            } else {
                self.style.font.weight
            },
            style: if italic {
                font::Style::Italic
            } else {
                self.style.font.style
            },
            ..self.style.font
        };
        Text {
            content: content.to_owned(),
            position,
            color,
            size: Pixels(self.style.font_size),
            line_height: LineHeight::Absolute(Pixels(self.style.cell.height)),
            font,
            shaping: Shaping::Advanced,
            ..Text::default()
        }
    }
}

impl<Message> canvas::Program<Message> for TerminalView<'_, Message> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
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
                let bytes = input::encode(press, self.terminal.mode(), self.input)?;
                Some(
                    Action::publish((self.on_interaction)(Interaction::Input(bytes))).and_capture(),
                )
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let interaction = self.wheel(*delta)?;
                Some(Action::publish((self.on_interaction)(interaction)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let snapshot = self.terminal.frame(&self.style.palette);
        let mut canvas = canvas::Frame::new(renderer, bounds.size());
        let background = to_iced(self.style.palette.background, self.style.background_opacity);
        canvas.fill_rectangle(Point::ORIGIN, bounds.size(), background);
        for run in &snapshot.runs {
            self.draw_run(&mut canvas, run);
        }
        if let Some(cursor) = snapshot.cursor {
            self.draw_cursor(&mut canvas, cursor, &snapshot.runs);
        }
        vec![canvas.into_geometry()]
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

fn char_at(runs: &[Run], cursor: Cursor) -> Option<(char, bool, bool)> {
    let run = runs.iter().find(|run| {
        run.row == cursor.row && (run.column..run.column + run.width).contains(&cursor.column)
    })?;
    let c = run.text.chars().nth(cursor.column - run.column)?;
    Some((c, run.style.bold, run.style.italic))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::SpawnOptions;
    use canvas::Program;
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
        let (message, _, status) = view.update(&mut (), event, bounds, cursor)?.into_inner();
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
    fn cursor_character_is_found_inside_its_run() {
        let run = Run {
            row: 1,
            column: 4,
            width: 3,
            text: "abc".into(),
            colors: crate::colors::CellColors {
                fg: Palette::default().foreground,
                bg: Palette::default().background,
            },
            style: crate::frame::RunStyle {
                bold: true,
                ..Default::default()
            },
        };
        let cursor = Cursor {
            row: 1,
            column: 5,
            shape: CursorShape::Block,
            wide: false,
        };
        assert_eq!(char_at(&[run], cursor), Some(('b', true, false)));
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
}

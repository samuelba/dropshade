//! Draws a terminal frame: text runs, the cursor, IME preedit text and the visual bell.

use alacritty_terminal::vte::ansi::CursorShape;
use iced::widget::canvas::{self, Geometry, Stroke, Text};
use iced::widget::text::{LineHeight, Shaping};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, font, mouse};
use unicode_width::UnicodeWidthStr;

use super::{TerminalView, ViewState, scrollbar};
use crate::colors::to_iced;
use crate::frame::{Cursor, Run};

const BAR_THICKNESS: f32 = 2.0;
const DECORATION_THICKNESS: f32 = 1.0;
const BELL_OPACITY: f32 = 0.08;

impl<Message> TerminalView<'_, Message> {
    pub(super) fn render(
        &self,
        state: &ViewState,
        renderer: &Renderer,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Geometry {
        let snapshot = self.terminal.frame(&self.style.palette);
        let mut canvas = canvas::Frame::new(renderer, bounds.size());
        let background = to_iced(self.style.palette.background, self.style.background_opacity);
        canvas.fill_rectangle(Point::ORIGIN, bounds.size(), background);
        for run in &snapshot.runs {
            self.draw_run(&mut canvas, run);
        }
        if let Some(cursor) = snapshot.cursor {
            self.draw_cursor(&mut canvas, cursor, state.focused, &snapshot.runs);
            if let Some(preedit) = &state.preedit {
                self.draw_preedit(&mut canvas, cursor, preedit);
            }
        }
        self.draw_scrollbar(&mut canvas, state, bounds, cursor);
        if self.bell {
            let flash = to_iced(self.style.palette.foreground, BELL_OPACITY);
            canvas.fill_rectangle(Point::ORIGIN, bounds.size(), flash);
        }
        canvas.into_geometry()
    }

    /// Shows the scrollbar whenever there is scrollback, emphasised while hovered or dragged.
    fn draw_scrollbar(
        &self,
        canvas: &mut canvas::Frame,
        state: &ViewState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        let Some(geometry) = self.scrollbar(bounds) else {
            return;
        };
        let hovered = cursor
            .position_in(bounds)
            .is_some_and(|point| geometry.contains(point));
        let active = hovered || state.scrollbar.dragging();
        let color = to_iced(self.style.palette.foreground, 1.0);
        scrollbar::draw(canvas, geometry, active, color);
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

    fn draw_cursor(&self, canvas: &mut canvas::Frame, cursor: Cursor, focused: bool, runs: &[Run]) {
        let shape = match cursor.shape {
            CursorShape::Hidden => return,
            _ if !focused => CursorShape::HollowBlock,
            _ if !self.cursor_visible => return,
            shape => shape,
        };
        let cell = self.style.cell;
        let origin = self.origin(cursor.row, cursor.column);
        let width = if cursor.wide {
            cell.width * 2.0
        } else {
            cell.width
        };
        let color = to_iced(self.style.palette.cursor, 1.0);
        match shape {
            CursorShape::Block => {
                canvas.fill_rectangle(origin, Size::new(width, cell.height), color);
                if let Some((c, bold, italic)) = char_at(runs, cursor) {
                    let bg = to_iced(self.style.palette.cursor_text(), 1.0);
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

    fn draw_preedit(&self, canvas: &mut canvas::Frame, cursor: Cursor, preedit: &str) {
        let cell = self.style.cell;
        let origin = self.origin(cursor.row, cursor.column);
        #[allow(clippy::cast_precision_loss)]
        let width = preedit.width().max(1) as f32 * cell.width;
        let palette = &self.style.palette;
        canvas.fill_rectangle(
            origin,
            Size::new(width, cell.height),
            to_iced(palette.background, 1.0),
        );
        let fg = to_iced(palette.foreground, 1.0);
        canvas.fill_text(self.text(preedit, origin, fg, false, false));
        let underline = Point::new(origin.x, origin.y + cell.height - DECORATION_THICKNESS);
        canvas.fill_rectangle(
            underline,
            Size::new(width, DECORATION_THICKNESS),
            to_iced(palette.cursor, 1.0),
        );
    }
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
    use crate::colors::Palette;

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
}

//! Draws a terminal frame: text runs, the cursor, IME preedit text and the visual bell.

use alacritty_terminal::vte::ansi::CursorShape;
use iced::widget::canvas::{self, Geometry, Stroke, Text};
use iced::widget::text::{LineHeight, Shaping};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, font, mouse};
use unicode_width::UnicodeWidthStr;

use super::{TerminalView, ViewState, scrollbar};
use crate::colors::{CellColors, to_iced};
use crate::frame::{Cursor, Run, RunStyle};
use crate::metrics;
use crate::vi::ViSelection;

const BAR_THICKNESS: f32 = 2.0;
const DECORATION_THICKNESS: f32 = 1.0;
const BELL_OPACITY: f32 = 0.08;
/// Room kept free for the scrollbar at the right edge, and around the vi mode badge.
const BADGE_RIGHT: f32 = 16.0;
const BADGE_TOP: f32 = 4.0;
const BADGE_PADDING_X: f32 = 6.0;
const BADGE_PADDING_Y: f32 = 2.0;
const BADGE_RADIUS: f32 = 4.0;
const BADGE_GAP: f32 = 6.0;

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
        // Emoji may be wider than their cells, so they go on top of the neighbouring runs.
        for run in snapshot.runs.iter().filter(|run| run.style.emoji) {
            let fg = to_iced(run.colors.fg, 1.0);
            let origin = self.origin(run.row, run.column);
            canvas.fill_text(self.text(&run.text, origin, fg, run.style));
        }
        if let Some(cursor) = snapshot.cursor {
            self.draw_cursor(
                &mut canvas,
                cursor,
                state.focused && self.active,
                &snapshot.runs,
            );
            if let Some(preedit) = &state.preedit {
                self.draw_preedit(&mut canvas, cursor, preedit);
            }
        }
        self.draw_scrollbar(&mut canvas, state, bounds, cursor);
        self.draw_badges(&mut canvas, bounds);
        if self.bell {
            let flash = to_iced(self.style.palette.foreground, BELL_OPACITY);
            canvas.fill_rectangle(Point::ORIGIN, bounds.size(), flash);
        }
        if self.dim > 0.0 {
            let fade = to_iced(self.style.palette.background, self.dim.min(1.0));
            canvas.fill_rectangle(Point::ORIGIN, bounds.size(), fade);
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

    /// Labels modes in the top-right corner, in the selection colours: vi mode (`VI`, or the
    /// kind of selection being made) and a zoomed pane (`ZOOM`).
    fn draw_badges(&self, canvas: &mut canvas::Frame, bounds: Rectangle) {
        let mut right = bounds.width - BADGE_RIGHT;
        if self.terminal.vi_mode() {
            let label = vi_label(self.terminal.vi_selection());
            right = self.draw_badge(canvas, right, label) - BADGE_GAP;
        }
        if self.zoomed {
            self.draw_badge(canvas, right, "ZOOM");
        }
    }

    /// Draws a badge that ends at `right`; returns where it starts.
    fn draw_badge(&self, canvas: &mut canvas::Frame, right: f32, label: &str) -> f32 {
        let cell = self.style.cell;
        #[allow(clippy::cast_precision_loss)]
        let size = Size::new(
            label.len() as f32 * cell.width + 2.0 * BADGE_PADDING_X,
            cell.height + 2.0 * BADGE_PADDING_Y,
        );
        let top_left = Point::new(right - size.width, BADGE_TOP);
        let palette = &self.style.palette;
        let colors = palette.selected(CellColors {
            fg: palette.foreground,
            bg: palette.background,
        });
        let badge = canvas::Path::rounded_rectangle(top_left, size, BADGE_RADIUS.into());
        canvas.fill(&badge, to_iced(colors.bg, 1.0));
        let text_origin = Point::new(top_left.x + BADGE_PADDING_X, top_left.y + BADGE_PADDING_Y);
        let style = RunStyle {
            bold: true,
            ..RunStyle::default()
        };
        canvas.fill_text(self.text(label, text_origin, to_iced(colors.fg, 1.0), style));
        top_left.x
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
        if !run.style.emoji {
            canvas.fill_text(self.text(&run.text, origin, fg, run.style));
        }
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
                if let Some((text, style)) = text_at(runs, cursor) {
                    let bg = to_iced(self.style.palette.cursor_text(), 1.0);
                    canvas.fill_text(self.text(&text, origin, bg, style));
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

    fn text(&self, content: &str, position: Point, color: Color, style: RunStyle) -> Text {
        if style.emoji
            && let Some(emoji) = metrics::emoji_font()
        {
            // At most two cells wide, the width terminals give an emoji.
            let fit = 2.0 * self.style.cell.width / emoji.advance;
            return Text {
                size: Pixels(self.style.font_size.min(fit)),
                font: emoji.font,
                ..self.text(content, position, color, RunStyle::default())
            };
        }
        let RunStyle { bold, italic, .. } = style;
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
        canvas.fill_text(self.text(preedit, origin, fg, RunStyle::default()));
        let underline = Point::new(origin.x, origin.y + cell.height - DECORATION_THICKNESS);
        canvas.fill_rectangle(
            underline,
            Size::new(width, DECORATION_THICKNESS),
            to_iced(palette.cursor, 1.0),
        );
    }
}

fn vi_label(selection: Option<ViSelection>) -> &'static str {
    match selection {
        None => "VI",
        Some(ViSelection::Simple | ViSelection::Semantic) => "VISUAL",
        Some(ViSelection::Line) => "V-LINE",
        Some(ViSelection::Block) => "V-BLOCK",
    }
}

/// The text under the cursor and its style: one character, or a whole emoji sequence.
fn text_at(runs: &[Run], cursor: Cursor) -> Option<(String, RunStyle)> {
    let run = runs.iter().find(|run| {
        run.row == cursor.row && (run.column..run.column + run.width).contains(&cursor.column)
    })?;
    let style = RunStyle {
        underline: false,
        strikeout: false,
        ..run.style
    };
    if run.style.emoji {
        return Some((run.text.clone(), style));
    }
    let c = run.text.chars().nth(cursor.column - run.column)?;
    Some((c.to_string(), style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::Palette;

    #[test]
    fn vi_badge_names_the_selection() {
        assert_eq!(vi_label(None), "VI");
        assert_eq!(vi_label(Some(ViSelection::Simple)), "VISUAL");
        assert_eq!(vi_label(Some(ViSelection::Semantic)), "VISUAL");
        assert_eq!(vi_label(Some(ViSelection::Line)), "V-LINE");
        assert_eq!(vi_label(Some(ViSelection::Block)), "V-BLOCK");
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
        let (text, style) = text_at(&[run], cursor).expect("cursor inside the run");
        assert_eq!(text, "b");
        assert!(style.bold && !style.italic);
    }
}

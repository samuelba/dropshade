//! The app icon, drawn on a canvas. It follows `dev.samuelba.Dropshade.svg` (a window with its
//! shade pulled down over a prompt) in that icon's 128-unit space, without the gradients.

use iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, border, mouse};

const VIEW_BOX: f32 = 128.0;
const PROMPT: Color = Color::from_rgb(0.949, 0.651, 0.353);
const SHADE_DARK: Color = Color::from_rgb(0.145, 0.157, 0.186);
const SLATE: Color = Color::from_rgb(0.227, 0.247, 0.294);
const FRAME: Color = Color::from_rgb(0.914, 0.894, 0.855);
const LIGHT: Color = Color::from_rgb(0.992, 0.953, 0.894);

#[derive(Debug, Clone, Copy)]
pub struct Icon;

impl<Message> canvas::Program<Message> for Icon {
    type State = ();

    fn draw(
        &self,
        (): &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.scale(bounds.width / VIEW_BOX);

        rounded(&mut frame, [14.0, 26.0, 100.0, 92.0], 13.0, shadow());
        rounded(&mut frame, [14.0, 22.0, 100.0, 92.0], 13.0, FRAME);
        rounded(&mut frame, [22.0, 30.0, 84.0, 76.0], 6.0, LIGHT);

        let shade = Path::rounded_rectangle(
            Point::new(22.0, 30.0),
            Size::new(84.0, 58.0),
            border::bottom(4.0),
        );
        frame.fill(&shade, SHADE_DARK);
        pill(&mut frame, [22.0, 84.0, 84.0, 5.0], SLATE);

        prompt(&mut frame);

        rounded(&mut frame, [62.75, 89.0, 2.5, 9.0], 0.0, SLATE);
        pill(&mut frame, [56.0, 97.0, 16.0, 9.0], PROMPT);
        pill(&mut frame, [58.0, 98.5, 12.0, 2.5], tab_highlight());

        pill(&mut frame, [8.0, 16.0, 112.0, 16.0], SLATE);
        pill(&mut frame, [14.0, 18.5, 100.0, 3.0], roller_highlight());

        vec![frame.into_geometry()]
    }
}

/// The `>_` prompt on the shade, with a soft glow under it.
fn prompt(frame: &mut Frame) {
    let chevron = Path::new(|builder| {
        builder.move_to(Point::new(34.0, 46.0));
        builder.line_to(Point::new(44.0, 54.0));
        builder.line_to(Point::new(34.0, 62.0));
    });
    let stroke = |width: f32, alpha: f32| Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default().with_color(Color { a: alpha, ..PROMPT })
    };
    frame.stroke(&chevron, stroke(10.0, 0.25));
    frame.stroke(&chevron, stroke(5.5, 1.0));
    pill(frame, [49.0, 59.0, 18.0, 5.5], PROMPT);
}

/// A rectangle given as `[x, y, width, height]`.
fn rounded(frame: &mut Frame, rect: [f32; 4], radius: f32, color: Color) {
    let [x, y, width, height] = rect;
    let path = Path::rounded_rectangle(Point::new(x, y), Size::new(width, height), radius.into());
    frame.fill(&path, color);
}

/// A rectangle whose ends are half circles.
fn pill(frame: &mut Frame, rect: [f32; 4], color: Color) {
    rounded(frame, rect, rect[3] / 2.0, color);
}

fn shadow() -> Color {
    Color::from_rgba(0.0, 0.0, 0.0, 0.18)
}

fn tab_highlight() -> Color {
    Color::from_rgba(1.0, 0.847, 0.659, 0.8)
}

fn roller_highlight() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.18)
}

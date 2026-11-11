//! The color picker: a saturation/brightness square and a hue strip drawn on canvases, a hex
//! field and quick picks from the palette.

use dropshade_config::theme::Rgb8;
use dropshade_term::Palette;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Gradient, Path, Stroke};
use iced::widget::{
    Space, button, canvas as canvas_widget, column, container, row, text, text_input,
};
use iced::{
    Background, Border, Color, Element, Font, Point, Rectangle, Renderer, Size, Theme, mouse,
    window,
};

use super::color_slots::{ColorSlot, rgb8};
use super::hsv::Hsv;
use super::view::{Ctx, TEXT_SIZE, bold, hint, settings};
use super::{PickerState, SettingsMessage};
use crate::app::Message;
use crate::theme::{RADIUS, UiColors};

const SQUARE: Size = Size::new(220.0, 150.0);
const STRIP_WIDTH: f32 = 18.0;
const RING_RADIUS: f32 = 6.0;
const SWATCH: f32 = 18.0;
const HUE_STOPS: usize = 7;

/// Which part of the picker a canvas shows.
#[derive(Debug, Clone, Copy)]
enum Area {
    SaturationValue,
    Hue,
}

/// A canvas that picks part of the color while the left button is held on it.
#[derive(Debug, Clone, Copy)]
struct Plane {
    area: Area,
    hsv: Hsv,
    border: Color,
}

#[derive(Debug, Default)]
struct Drag {
    active: bool,
}

impl Plane {
    fn pick(&self, position: Point, size: Size) -> Hsv {
        let x = (position.x / size.width).clamp(0.0, 1.0);
        let y = (position.y / size.height).clamp(0.0, 1.0);
        match self.area {
            Area::SaturationValue => Hsv::new(self.hsv.h, x, 1.0 - y),
            Area::Hue => Hsv::new(y * 360.0, self.hsv.s, self.hsv.v),
        }
    }

    fn draw_saturation_value(&self, frame: &mut Frame, size: Size) {
        let clear = |color: Color| Color { a: 0.0, ..color };
        frame.fill_rectangle(Point::ORIGIN, size, iced_color(self.hsv.pure_hue()));
        let whiten = canvas::gradient::Linear::new(Point::ORIGIN, Point::new(size.width, 0.0))
            .add_stop(0.0, Color::WHITE)
            .add_stop(1.0, clear(Color::WHITE));
        frame.fill_rectangle(Point::ORIGIN, size, Gradient::Linear(whiten));
        let darken = canvas::gradient::Linear::new(Point::ORIGIN, Point::new(0.0, size.height))
            .add_stop(0.0, clear(Color::BLACK))
            .add_stop(1.0, Color::BLACK);
        frame.fill_rectangle(Point::ORIGIN, size, Gradient::Linear(darken));
        let center = Point::new(self.hsv.s * size.width, (1.0 - self.hsv.v) * size.height);
        let ring = Path::circle(center, RING_RADIUS);
        frame.stroke(
            &ring,
            Stroke::default().with_color(shadow()).with_width(3.5),
        );
        frame.stroke(
            &ring,
            Stroke::default().with_color(Color::WHITE).with_width(2.0),
        );
    }

    #[allow(clippy::cast_precision_loss, reason = "a handful of stops")]
    fn draw_hue(&self, frame: &mut Frame, size: Size) {
        let last = (HUE_STOPS - 1) as f32;
        let rainbow = (0..HUE_STOPS).fold(
            canvas::gradient::Linear::new(Point::ORIGIN, Point::new(0.0, size.height)),
            |gradient, stop| {
                let offset = stop as f32 / last;
                let hue = Hsv::new(offset * 360.0, 1.0, 1.0).to_rgb();
                gradient.add_stop(offset, iced_color(hue))
            },
        );
        frame.fill_rectangle(Point::ORIGIN, size, Gradient::Linear(rainbow));
        let y = (self.hsv.h / 360.0 * size.height).clamp(2.0, size.height - 2.0);
        let marker = Path::rectangle(Point::new(1.0, y - 2.0), Size::new(size.width - 2.0, 4.0));
        frame.stroke(
            &marker,
            Stroke::default().with_color(shadow()).with_width(3.0),
        );
        frame.stroke(
            &marker,
            Stroke::default().with_color(Color::WHITE).with_width(1.5),
        );
    }
}

impl canvas::Program<Message> for Plane {
    type State = Drag;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        // A scrollable shifts `cursor` by its offset but not the positions inside events, so
        // positions come from `cursor`. It keeps reporting while the pointer is outside.
        let picked = |cursor: mouse::Cursor| {
            let position = cursor.land().position_from(bounds.position())?;
            let hsv = self.pick(position, bounds.size());
            Some(settings(SettingsMessage::PickerDrag(hsv)))
        };
        let release = |state: &mut Drag| {
            state.active = false;
            Some(Action::publish(settings(SettingsMessage::PickerRelease)).and_capture())
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.active = true;
                Some(Action::publish(picked(cursor)?).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.active => {
                Some(Action::publish(picked(cursor)?).and_capture())
            }
            Event::Mouse(
                mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
            )
            | Event::Window(window::Event::Unfocused)
                if state.active =>
            {
                release(state)
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
        let size = bounds.size();
        let mut frame = Frame::new(renderer, size);
        match self.area {
            Area::SaturationValue => self.draw_saturation_value(&mut frame, size),
            Area::Hue => self.draw_hue(&mut frame, size),
        }
        let outline = Path::rectangle(Point::ORIGIN, size);
        frame.stroke(
            &outline,
            Stroke::default().with_color(self.border).with_width(1.0),
        );
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.active || cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

/// The open picker for `state.slot`.
pub(super) fn picker(
    ctx: Ctx<'_>,
    state: &PickerState,
    palette: &Palette,
) -> Element<'static, Message> {
    let hsv = state.shown(palette);
    let plane = |area, width, height| {
        canvas_widget(Plane {
            area,
            hsv,
            border: ctx.colors.border,
        })
        .width(width)
        .height(height)
    };
    let hex = text_input("#rrggbb", &state.hex_text(palette))
        .size(TEXT_SIZE)
        .font(Font::MONOSPACE)
        .padding([6, 8])
        .width(100)
        .on_input(|text| settings(SettingsMessage::PickerHex(text)))
        .on_submit(settings(SettingsMessage::PickerHexSubmit));
    let reset: Element<'static, Message> = if state.slot.configured(&ctx.config.colors).is_some() {
        button(text("Reset to theme").size(12))
            .padding([4, 10])
            .style(move |_, status| ctx.colors.ghost_button(ctx.colors.muted, status))
            .on_press(settings(SettingsMessage::ResetColor(state.slot)))
            .into()
    } else {
        Space::new().into()
    };
    let quick = |colors: [dropshade_term::alacritty_terminal::vte::ansi::Rgb; 9]| {
        row(colors.map(|color| {
            let color = rgb8(color);
            swatch(
                color,
                SWATCH,
                false,
                ctx.colors,
                SettingsMessage::QuickPick(color),
            )
        }))
        .spacing(4)
    };
    let mut normal = [palette.foreground; 9];
    normal[..8].copy_from_slice(&palette.normal);
    let mut bright = [palette.background; 9];
    bright[..8].copy_from_slice(&palette.bright);
    let side = column![
        text(state.slot.label())
            .size(TEXT_SIZE)
            .font(bold())
            .color(ctx.colors.text),
        hex,
        hint("Or take a color from the theme:", ctx.colors),
        quick(normal),
        quick(bright),
        reset,
    ]
    .spacing(8);
    container(
        row![
            plane(Area::SaturationValue, SQUARE.width, SQUARE.height),
            plane(Area::Hue, STRIP_WIDTH, SQUARE.height),
            side,
        ]
        .spacing(12),
    )
    .padding(12)
    .style(move |_| panel_style(ctx.colors))
    .into()
}

/// A clickable color sample; `selected` draws an accent ring around it.
pub(super) fn swatch(
    color: Rgb8,
    size: f32,
    selected: bool,
    colors: UiColors,
    message: SettingsMessage,
) -> Element<'static, Message> {
    let fill = iced_color(color);
    button(Space::new().width(size).height(size))
        .padding(0)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: if selected {
                        colors.accent
                    } else if hovered {
                        colors.text
                    } else {
                        colors.border
                    },
                    width: if selected { 2.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                ..button::Style::default()
            }
        })
        .on_press(settings(message))
        .into()
}

pub(super) fn iced_color(color: Rgb8) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}

/// The slot whose picker is open, if it is one of `slots`.
pub(super) fn open_in(
    state: Option<&PickerState>,
    slots: impl IntoIterator<Item = ColorSlot>,
) -> Option<&PickerState> {
    let state = state?;
    slots
        .into_iter()
        .any(|slot| slot == state.slot)
        .then_some(state)
}

fn panel_style(colors: UiColors) -> container::Style {
    container::Style {
        background: Some(Background::Color(colors.bar)),
        border: Border {
            color: colors.border,
            width: 1.0,
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}

fn shadow() -> Color {
    Color {
        a: 0.6,
        ..Color::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(area: Area) -> Plane {
        Plane {
            area,
            hsv: Hsv::new(200.0, 0.5, 0.5),
            border: Color::BLACK,
        }
    }

    #[test]
    fn the_square_picks_saturation_and_value_and_keeps_the_hue() {
        let picked =
            plane(Area::SaturationValue).pick(Point::new(50.0, 25.0), Size::new(100.0, 100.0));
        assert_eq!(picked, Hsv::new(200.0, 0.5, 0.75));
        let corner =
            plane(Area::SaturationValue).pick(Point::new(-20.0, 500.0), Size::new(100.0, 100.0));
        assert_eq!(
            corner,
            Hsv::new(200.0, 0.0, 0.0),
            "positions outside are clamped"
        );
    }

    #[test]
    fn the_strip_picks_the_hue_and_keeps_the_rest() {
        let picked = plane(Area::Hue).pick(Point::new(5.0, 50.0), Size::new(18.0, 100.0));
        assert_eq!(picked, Hsv::new(180.0, 0.5, 0.5));
    }
}

//! Interface colors (tab bar, menus, dialogs) derived from the terminal palette, so the chrome
//! always matches the color scheme.

use dropshade_term::Palette;
use dropshade_term::colors::to_iced;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Vector};

pub const RADIUS: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiColors {
    pub bar: Color,
    pub hover: Color,
    pub active: Color,
    pub surface: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub backdrop: Color,
}

impl UiColors {
    pub fn from_palette(palette: &Palette) -> Self {
        let bg = to_iced(palette.background, 1.0);
        let fg = to_iced(palette.foreground, 1.0);
        Self {
            bar: mix(bg, fg, 0.04),
            hover: mix(bg, fg, 0.08),
            active: mix(bg, fg, 0.13),
            surface: mix(bg, fg, 0.07),
            border: mix(bg, fg, 0.16),
            text: fg,
            muted: mix(fg, bg, 0.45),
            accent: to_iced(palette.cursor, 1.0),
            backdrop: Color {
                a: 0.45,
                ..Color::BLACK
            },
        }
    }

    /// An iced theme in the terminal's colors, so built-in widgets (lists, sliders, toggles)
    /// match the chrome.
    pub fn iced_theme(self, palette: &Palette) -> iced::Theme {
        let color = |rgb: dropshade_term::alacritty_terminal::vte::ansi::Rgb| to_iced(rgb, 1.0);
        iced::Theme::custom(
            "Dropshade",
            iced::theme::Palette {
                background: self.surface,
                text: self.text,
                primary: self.accent,
                success: color(palette.normal[2]),
                warning: color(palette.normal[3]),
                danger: color(palette.normal[1]),
            },
        )
    }

    pub fn bar_style(self) -> container::Style {
        container::Style::default().background(self.bar)
    }

    pub fn tab_style(self, active: bool, hovered: bool) -> container::Style {
        let background = if active {
            Some(self.active)
        } else if hovered {
            Some(self.hover)
        } else {
            None
        };
        container::Style {
            background: background.map(Background::Color),
            text_color: Some(if active || hovered {
                self.text
            } else {
                self.muted
            }),
            border: Border {
                radius: RADIUS.into(),
                ..Border::default()
            },
            ..container::Style::default()
        }
    }

    pub fn surface_style(self) -> container::Style {
        container::Style {
            background: Some(Background::Color(self.surface)),
            text_color: Some(self.text),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: (RADIUS + 2.0).into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.35,
                    ..Color::BLACK
                },
                offset: Vector::new(0.0, 6.0),
                blur_radius: 24.0,
            },
            ..container::Style::default()
        }
    }

    pub fn backdrop_style(self) -> container::Style {
        container::Style::default().background(self.backdrop)
    }

    /// A flat button that only shows a background on hover; `color` is its text color.
    pub fn ghost_button(self, color: Color, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => Some(self.active),
            button::Status::Active | button::Status::Disabled => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: color,
            border: Border {
                radius: (RADIUS - 2.0).into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }

    /// A sidebar entry: highlighted while its page is shown.
    pub fn nav_button(self, active: bool, status: button::Status) -> button::Style {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if active {
            Some(self.active)
        } else if hovered {
            Some(self.hover)
        } else {
            None
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: if active || hovered {
                self.text
            } else {
                self.muted
            },
            border: Border {
                radius: (RADIUS - 2.0).into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }

    pub fn accent_button(self, status: button::Status) -> button::Style {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => mix(self.accent, self.text, 0.2),
            button::Status::Active | button::Status::Disabled => self.accent,
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: Color::BLACK,
            border: Border {
                radius: (RADIUS - 2.0).into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

/// Blends `from` toward `to` by `amount` (0.0 = `from`, 1.0 = `to`).
pub fn mix(from: Color, to: Color, amount: f32) -> Color {
    let blend = |a: f32, b: f32| a + (b - a) * amount;
    Color::from_rgba(
        blend(from.r, to.r),
        blend(from.g, to.g),
        blend(from.b, to.b),
        blend(from.a, to.a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_interpolates_each_channel() {
        let mixed = mix(Color::BLACK, Color::WHITE, 0.25);
        assert!((mixed.r - 0.25).abs() < 1e-6 && (mixed.g - 0.25).abs() < 1e-6);
        assert_eq!(mix(Color::BLACK, Color::WHITE, 0.0), Color::BLACK);
        assert_eq!(mix(Color::BLACK, Color::WHITE, 1.0), Color::WHITE);
    }

    #[test]
    fn colors_follow_the_palette() {
        let palette = Palette::default();
        let colors = UiColors::from_palette(&palette);
        assert_eq!(colors.accent, to_iced(palette.cursor, 1.0));
        assert!((colors.bar.a - 1.0).abs() < f32::EPSILON);
        let luminance = |c: Color| c.r + c.g + c.b;
        assert!(luminance(colors.active) > luminance(colors.hover));
        assert!(luminance(colors.hover) > luminance(to_iced(palette.background, 1.0)));
    }
}

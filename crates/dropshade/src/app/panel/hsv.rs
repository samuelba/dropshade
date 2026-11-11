//! Hue, saturation and value: the coordinates of the color picker.

use dropshade_config::theme::Rgb8;

/// `h` in degrees (0..360), `s` and `v` from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    /// Clamps every component into its range.
    pub fn new(h: f32, s: f32, v: f32) -> Self {
        Self {
            h: h.clamp(0.0, 360.0),
            s: s.clamp(0.0, 1.0),
            v: v.clamp(0.0, 1.0),
        }
    }

    pub fn from_rgb(color: Rgb8) -> Self {
        let [r, g, b] = [color.r, color.g, color.b].map(|c| f32::from(c) / 255.0);
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let hue = if delta <= f32::EPSILON {
            0.0
        } else if (max - r).abs() <= f32::EPSILON {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if (max - g).abs() <= f32::EPSILON {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let saturation = if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        };
        Self::new(hue, saturation, max)
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "channels are rounded values clamped to 0..=255"
    )]
    pub fn to_rgb(self) -> Rgb8 {
        let chroma = self.v * self.s;
        let sector = (self.h % 360.0) / 60.0;
        let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
        let (r, g, b) = match sector {
            s if s < 1.0 => (chroma, second, 0.0),
            s if s < 2.0 => (second, chroma, 0.0),
            s if s < 3.0 => (0.0, chroma, second),
            s if s < 4.0 => (0.0, second, chroma),
            s if s < 5.0 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let lightest = self.v - chroma;
        let channel = |c: f32| ((c + lightest) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgb8 {
            r: channel(r),
            g: channel(g),
            b: channel(b),
        }
    }

    /// The fully saturated, fully bright color of this hue.
    pub fn pure_hue(self) -> Rgb8 {
        Self::new(self.h, 1.0, 1.0).to_rgb()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primaries_and_secondaries_convert_both_ways() {
        for (hex, hue) in [
            (0xff_00_00, 0.0),
            (0xff_ff_00, 60.0),
            (0x00_ff_00, 120.0),
            (0x00_ff_ff, 180.0),
            (0x00_00_ff, 240.0),
            (0xff_00_ff, 300.0),
        ] {
            let color = Rgb8::hex(hex);
            let hsv = Hsv::from_rgb(color);
            assert!((hsv.h - hue).abs() < 0.01, "{hex:06x}: {hsv:?}");
            assert!((hsv.s - 1.0).abs() < 0.01 && (hsv.v - 1.0).abs() < 0.01);
            assert_eq!(hsv.to_rgb(), color);
        }
    }

    #[test]
    fn every_theme_color_survives_a_round_trip() {
        let theme = dropshade_config::theme::DUSK;
        let colors = [theme.foreground, theme.background, theme.cursor]
            .into_iter()
            .chain(theme.normal)
            .chain(theme.bright);
        for color in colors {
            assert_eq!(Hsv::from_rgb(color).to_rgb(), color, "{}", color.to_hex());
        }
    }

    #[test]
    fn grays_have_no_saturation_and_black_no_value() {
        let gray = Hsv::from_rgb(Rgb8::hex(0x80_80_80));
        assert!(gray.s.abs() < f32::EPSILON);
        assert_eq!(gray.to_rgb(), Rgb8::hex(0x80_80_80));
        let black = Hsv::from_rgb(Rgb8::hex(0));
        assert!(black.v.abs() < f32::EPSILON && black.s.abs() < f32::EPSILON);
        assert_eq!(Hsv::new(200.0, 0.7, 0.0).to_rgb(), Rgb8::hex(0));
    }

    #[test]
    fn components_are_clamped_and_hue_360_wraps_to_red() {
        let hsv = Hsv::new(400.0, 2.0, -1.0);
        assert_eq!((hsv.h, hsv.s, hsv.v), (360.0, 1.0, 0.0));
        assert_eq!(Hsv::new(360.0, 1.0, 1.0).to_rgb(), Rgb8::hex(0xff_00_00));
        assert_eq!(Hsv::new(120.0, 0.2, 0.9).pure_hue(), Rgb8::hex(0x00_ff_00));
    }
}

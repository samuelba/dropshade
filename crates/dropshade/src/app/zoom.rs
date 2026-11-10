//! Zoom: the terminal font scaled in steps on top of the configured size. It applies to all tabs
//! and is not saved; the font size in the settings stays what it is.

const STEP: f32 = 1.1;
const MIN_STEPS: i32 = -6;
const MAX_STEPS: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Zoom(i32);

impl Zoom {
    pub fn scale(self) -> f32 {
        STEP.powi(self.0)
    }

    pub fn is_normal(self) -> bool {
        self.0 == 0
    }

    pub fn can_zoom_in(self) -> bool {
        self.0 < MAX_STEPS
    }

    pub fn can_zoom_out(self) -> bool {
        self.0 > MIN_STEPS
    }

    #[must_use]
    pub fn zoom_in(self) -> Self {
        Self((self.0 + 1).min(MAX_STEPS))
    }

    #[must_use]
    pub fn zoom_out(self) -> Self {
        Self((self.0 - 1).max(MIN_STEPS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_normal_size() {
        assert!(Zoom::default().is_normal());
        assert!((Zoom::default().scale() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zooming_in_and_out_scales_the_font() {
        assert!(Zoom::default().zoom_in().scale() > 1.0);
        assert!(Zoom::default().zoom_out().scale() < 1.0);
        assert_eq!(Zoom::default().zoom_in().zoom_out(), Zoom::default());
    }

    #[test]
    fn zooming_stops_at_the_limits() {
        let max = (0..100).fold(Zoom::default(), |zoom, _| zoom.zoom_in());
        assert!(!max.can_zoom_in());
        assert_eq!(max.zoom_in(), max);
        let min = (0..100).fold(Zoom::default(), |zoom, _| zoom.zoom_out());
        assert!(!min.can_zoom_out());
        assert_eq!(min.zoom_out(), min);
    }

    #[test]
    fn the_limits_keep_the_font_readable() {
        let max = (0..100).fold(Zoom::default(), |zoom, _| zoom.zoom_in());
        let min = (0..100).fold(Zoom::default(), |zoom, _| zoom.zoom_out());
        assert!(min.scale() > 0.5 && max.scale() < 4.0);
    }
}

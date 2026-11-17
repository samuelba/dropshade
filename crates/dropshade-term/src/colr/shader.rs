//! The colour a `COLRv1` brush gives each point: solid fills and the three gradient kinds.

use skrifa::color::{Brush, ColorStop, Extend};

/// A premultiplied RGBA colour, each channel from 0 to 1.
pub type Rgba = [f32; 4];

pub const TRANSPARENT: Rgba = [0.0; 4];

/// A brush resolved against the palette, ready to be evaluated per pixel.
#[derive(Debug, Clone)]
pub enum Shader {
    Solid(Rgba),
    Linear {
        start: (f32, f32),
        direction: (f32, f32),
        line: ColorLine,
    },
    Radial {
        c0: (f32, f32),
        r0: f32,
        dc: (f32, f32),
        dr: f32,
        line: ColorLine,
    },
    Sweep {
        center: (f32, f32),
        start_angle: f32,
        end_angle: f32,
        line: ColorLine,
    },
}

/// Colour stops sorted by offset, with how the line continues past them.
#[derive(Debug, Clone)]
pub struct ColorLine {
    stops: Vec<(f32, Rgba)>,
    extend: Extend,
}

impl ColorLine {
    fn new(stops: &[ColorStop], extend: Extend, color: &impl Fn(u16, f32) -> Rgba) -> Self {
        let mut stops: Vec<(f32, Rgba)> = stops
            .iter()
            .map(|stop| (stop.offset, color(stop.palette_index, stop.alpha)))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { stops, extend }
    }

    fn at(&self, t: f32) -> Rgba {
        let t = match self.extend {
            Extend::Repeat => t - t.floor(),
            Extend::Reflect => {
                let u = t.rem_euclid(2.0);
                if u > 1.0 { 2.0 - u } else { u }
            }
            _ => t.clamp(0.0, 1.0),
        };
        let Some(&(first_offset, first)) = self.stops.first() else {
            return TRANSPARENT;
        };
        if t <= first_offset {
            return first;
        }
        for pair in self.stops.windows(2) {
            let ((a, ca), (b, cb)) = (pair[0], pair[1]);
            if t <= b {
                let f = if b > a { (t - a) / (b - a) } else { 1.0 };
                return std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * f);
            }
        }
        self.stops.last().map_or(TRANSPARENT, |&(_, c)| c)
    }
}

impl Shader {
    /// Resolves `brush`, looking palette entries up with `color(palette_index, alpha)`.
    pub fn new(brush: &Brush<'_>, color: impl Fn(u16, f32) -> Rgba) -> Self {
        match *brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => Self::Solid(color(palette_index, alpha)),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => Self::Linear {
                start: (p0.x, p0.y),
                direction: (p1.x - p0.x, p1.y - p0.y),
                line: ColorLine::new(color_stops, extend, &color),
            },
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => Self::Radial {
                c0: (c0.x, c0.y),
                r0,
                dc: (c1.x - c0.x, c1.y - c0.y),
                dr: r1 - r0,
                line: ColorLine::new(color_stops, extend, &color),
            },
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => Self::Sweep {
                center: (c0.x, c0.y),
                start_angle,
                end_angle,
                line: ColorLine::new(color_stops, extend, &color),
            },
        }
    }

    /// The colour at `(x, y)`, in the brush's own coordinates.
    pub fn at(&self, x: f32, y: f32) -> Rgba {
        match self {
            Self::Solid(color) => *color,
            Self::Linear {
                start,
                direction,
                line,
            } => {
                let length = direction.0 * direction.0 + direction.1 * direction.1;
                if length <= f32::EPSILON {
                    return TRANSPARENT;
                }
                let t = ((x - start.0) * direction.0 + (y - start.1) * direction.1) / length;
                line.at(t)
            }
            Self::Radial {
                c0,
                r0,
                dc,
                dr,
                line,
            } => radial_t((x - c0.0, y - c0.1), *r0, *dc, *dr).map_or(TRANSPARENT, |t| line.at(t)),
            Self::Sweep {
                center,
                start_angle,
                end_angle,
                line,
            } => {
                if (end_angle - start_angle).abs() <= f32::EPSILON {
                    return TRANSPARENT;
                }
                let angle = (y - center.1)
                    .atan2(x - center.0)
                    .to_degrees()
                    .rem_euclid(360.0);
                line.at((angle - start_angle) / (end_angle - start_angle))
            }
        }
    }
}

/// Where on the colour line the point `p` (relative to the first circle's centre) lies for a
/// two-circle gradient: the largest `t` whose circle, with a radius of zero or more, passes
/// through `p`.
fn radial_t(p: (f32, f32), r0: f32, dc: (f32, f32), dr: f32) -> Option<f32> {
    let a = dc.0 * dc.0 + dc.1 * dc.1 - dr * dr;
    let b = p.0 * dc.0 + p.1 * dc.1 + r0 * dr;
    let c = p.0 * p.0 + p.1 * p.1 - r0 * r0;
    let radius_ok = |t: f32| r0 + t * dr >= 0.0;
    if a.abs() <= 1e-6 {
        if b.abs() <= 1e-6 {
            return None;
        }
        return Some(c / (2.0 * b)).filter(|&t| radius_ok(t));
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let (t1, t2) = ((b + root) / a, (b - root) / a);
    let (high, low) = if t1 >= t2 { (t1, t2) } else { (t2, t1) };
    [high, low].into_iter().find(|&t| radius_ok(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::raw::types::Point;

    const RED: Rgba = [1.0, 0.0, 0.0, 1.0];
    const BLUE: Rgba = [0.0, 0.0, 1.0, 1.0];

    fn palette(index: u16, alpha: f32) -> Rgba {
        let base = if index == 0 { RED } else { BLUE };
        base.map(|c| c * alpha)
    }

    fn stops() -> [ColorStop; 2] {
        [
            ColorStop {
                offset: 1.0,
                palette_index: 1,
                alpha: 1.0,
            },
            ColorStop {
                offset: 0.0,
                palette_index: 0,
                alpha: 1.0,
            },
        ]
    }

    fn close(a: Rgba, b: Rgba) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn solid_applies_its_alpha() {
        let shader = Shader::new(
            &Brush::Solid {
                palette_index: 0,
                alpha: 0.5,
            },
            palette,
        );
        assert!(close(shader.at(3.0, 4.0), [0.5, 0.0, 0.0, 0.5]));
    }

    #[test]
    fn linear_gradient_runs_between_its_points_and_pads_beyond() {
        let stops = stops();
        let shader = Shader::new(
            &Brush::LinearGradient {
                p0: Point::new(0.0, 0.0),
                p1: Point::new(10.0, 0.0),
                color_stops: &stops,
                extend: Extend::Pad,
            },
            palette,
        );
        assert!(close(shader.at(-5.0, 7.0), RED));
        assert!(close(shader.at(5.0, -3.0), [0.5, 0.0, 0.5, 1.0]));
        assert!(close(shader.at(20.0, 0.0), BLUE));
    }

    #[test]
    fn repeat_and_reflect_continue_the_line() {
        let stops = stops();
        let line = ColorLine::new(&stops, Extend::Repeat, &palette);
        assert!(close(line.at(1.25), line.at(0.25)));
        let line = ColorLine::new(&stops, Extend::Reflect, &palette);
        assert!(close(line.at(1.25), line.at(0.75)));
    }

    #[test]
    fn radial_gradient_grows_from_the_centre() {
        let stops = stops();
        let shader = Shader::new(
            &Brush::RadialGradient {
                c0: Point::new(0.0, 0.0),
                r0: 0.0,
                c1: Point::new(0.0, 0.0),
                r1: 10.0,
                color_stops: &stops,
                extend: Extend::Pad,
            },
            palette,
        );
        assert!(close(shader.at(0.0, 0.0), RED));
        assert!(close(shader.at(0.0, 5.0), [0.5, 0.0, 0.5, 1.0]));
        assert!(close(shader.at(30.0, 0.0), BLUE));
    }

    #[test]
    fn radial_gradient_outside_a_focal_cone_is_empty() {
        // A small circle at the origin growing into a large one to the right: points far to
        // the left lie on no circle of the gradient.
        assert_eq!(radial_t((-100.0, 0.0), 1.0, (10.0, 0.0), 0.5), None);
        assert!(radial_t((10.0, 0.0), 1.0, (10.0, 0.0), 0.5).is_some());
    }

    #[test]
    fn sweep_gradient_follows_the_angle() {
        let stops = stops();
        let shader = Shader::new(
            &Brush::SweepGradient {
                c0: Point::new(0.0, 0.0),
                start_angle: 0.0,
                end_angle: 360.0,
                color_stops: &stops,
                extend: Extend::Pad,
            },
            palette,
        );
        assert!(close(shader.at(-1.0, 0.0), [0.5, 0.0, 0.5, 1.0]));
    }
}

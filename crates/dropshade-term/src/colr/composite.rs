//! Merges a `COLRv1` layer onto the one below it with the layer's composite mode.

use skrifa::color::CompositeMode;

use super::shader::Rgba;

/// `source` composited onto `backdrop`, both premultiplied. The HSL modes, which no common
/// emoji font uses, are drawn as plain source-over.
pub fn composite(source: Rgba, backdrop: Rgba, mode: CompositeMode) -> Rgba {
    let (sa, da) = (source[3], backdrop[3]);
    let porter_duff =
        |fs: f32, fd: f32| -> Rgba { std::array::from_fn(|i| source[i] * fs + backdrop[i] * fd) };
    match mode {
        CompositeMode::Clear => [0.0; 4],
        CompositeMode::Src => source,
        CompositeMode::Dest => backdrop,
        CompositeMode::DestOver => porter_duff(1.0 - da, 1.0),
        CompositeMode::SrcIn => porter_duff(da, 0.0),
        CompositeMode::DestIn => porter_duff(0.0, sa),
        CompositeMode::SrcOut => porter_duff(1.0 - da, 0.0),
        CompositeMode::DestOut => porter_duff(0.0, 1.0 - sa),
        CompositeMode::SrcAtop => porter_duff(da, 1.0 - sa),
        CompositeMode::DestAtop => porter_duff(1.0 - da, sa),
        CompositeMode::Xor => porter_duff(1.0 - da, 1.0 - sa),
        CompositeMode::Plus => std::array::from_fn(|i| (source[i] + backdrop[i]).min(1.0)),
        CompositeMode::Screen => blend(source, backdrop, |s, d| s + d - s * d),
        CompositeMode::Overlay => blend(source, backdrop, |s, d| hard_light(d, s)),
        CompositeMode::Darken => blend(source, backdrop, f32::min),
        CompositeMode::Lighten => blend(source, backdrop, f32::max),
        CompositeMode::ColorDodge => blend(source, backdrop, color_dodge),
        CompositeMode::ColorBurn => blend(source, backdrop, color_burn),
        CompositeMode::HardLight => blend(source, backdrop, hard_light),
        CompositeMode::SoftLight => blend(source, backdrop, soft_light),
        CompositeMode::Difference => blend(source, backdrop, |s, d| (s - d).abs()),
        CompositeMode::Exclusion => blend(source, backdrop, |s, d| s + d - 2.0 * s * d),
        CompositeMode::Multiply => blend(source, backdrop, |s, d| s * d),
        _ => porter_duff(1.0, 1.0 - sa),
    }
}

/// A separable blend mode, `mix(source, backdrop)` taking unpremultiplied channels.
fn blend(source: Rgba, backdrop: Rgba, mix: impl Fn(f32, f32) -> f32) -> Rgba {
    let (sa, da) = (source[3], backdrop[3]);
    let unpremultiply = |c: f32, a: f32| if a > 0.0 { c / a } else { 0.0 };
    let mut out: Rgba = std::array::from_fn(|i| {
        let mixed = mix(unpremultiply(source[i], sa), unpremultiply(backdrop[i], da));
        (1.0 - da) * source[i] + (1.0 - sa) * backdrop[i] + sa * da * mixed
    });
    out[3] = sa + da - sa * da;
    out
}

fn hard_light(s: f32, d: f32) -> f32 {
    if s <= 0.5 {
        d * 2.0 * s
    } else {
        let s = 2.0 * s - 1.0;
        s + d - s * d
    }
}

fn color_dodge(s: f32, d: f32) -> f32 {
    if d <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (d / (1.0 - s)).min(1.0)
    }
}

fn color_burn(s: f32, d: f32) -> f32 {
    if d >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - d) / s).min(1.0)
    }
}

fn soft_light(s: f32, d: f32) -> f32 {
    if s <= 0.5 {
        d - (1.0 - 2.0 * s) * d * (1.0 - d)
    } else {
        let lifted = if d <= 0.25 {
            ((16.0 * d - 12.0) * d + 4.0) * d
        } else {
            d.sqrt()
        };
        d + (2.0 * s - 1.0) * (lifted - d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_RED: Rgba = [0.5, 0.0, 0.0, 0.5];
    const BLUE: Rgba = [0.0, 0.0, 1.0, 1.0];

    #[test]
    fn source_over_lets_the_backdrop_through_translucent_source() {
        assert_eq!(
            composite(HALF_RED, BLUE, CompositeMode::SrcOver),
            [0.5, 0.0, 0.5, 1.0]
        );
    }

    #[test]
    fn source_in_keeps_the_source_where_the_backdrop_is() {
        assert_eq!(
            composite(HALF_RED, [0.0; 4], CompositeMode::SrcIn),
            [0.0; 4]
        );
        assert_eq!(composite(HALF_RED, BLUE, CompositeMode::SrcIn), HALF_RED);
    }

    #[test]
    fn multiply_of_opaque_colours_multiplies_channels() {
        let grey = [0.5, 0.5, 0.5, 1.0];
        let out = composite(grey, [1.0, 0.5, 0.0, 1.0], CompositeMode::Multiply);
        assert_eq!(out, [0.5, 0.25, 0.0, 1.0]);
    }

    #[test]
    fn blending_onto_nothing_is_the_source() {
        let out = composite(HALF_RED, [0.0; 4], CompositeMode::Screen);
        assert_eq!(out, HALF_RED);
    }
}

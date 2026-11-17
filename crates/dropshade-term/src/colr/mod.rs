//! Paints `COLRv1` colour glyphs into RGBA pixels. iced's text renderer only knows bitmap and
//! `COLRv0` colour fonts and draws `COLRv1` glyphs, like those of Noto Color Emoji on Fedora,
//! as nothing.

mod composite;
mod shader;

use skrifa::color::{
    Brush, ColorGlyphFormat, ColorPainter, CompositeMode, Transform as ColrTransform,
};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlineGlyphCollection, OutlinePen};
use skrifa::raw::types::BoundingBox;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{FillRule, Mask, Path, PathBuilder, Point, Rect, Transform};

use composite::composite;
pub use shader::Rgba;
use shader::{Shader, TRANSPARENT};

/// The palette index `COLRv1` uses for the text colour.
const FOREGROUND_INDEX: u16 = 0xFFFF;

/// A glyph to paint, with its origin on the baseline in pixels from the image's top left.
#[derive(Debug, Clone, Copy)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub baseline: f32,
}

/// Paints `glyphs` at `pixels_per_em` into a `width` × `height` image, as straight `RGBA8`.
/// `None` unless every glyph has a `COLRv1` paint, so other fonts keep their usual rendering.
pub fn paint(
    font: &FontRef<'_>,
    glyphs: &[PlacedGlyph],
    pixels_per_em: f32,
    (width, height): (u32, u32),
    foreground: Rgba,
) -> Option<Vec<u8>> {
    let color_glyphs = font.color_glyphs();
    let paints: Vec<_> = glyphs
        .iter()
        .map(|glyph| {
            color_glyphs
                .get_with_format(GlyphId::new(glyph.id), ColorGlyphFormat::ColrV1)
                .map(|paint| (glyph, paint))
        })
        .collect::<Option<_>>()?;
    let units_per_em = f32::from(
        font.metrics(Size::unscaled(), LocationRef::default())
            .units_per_em,
    );
    if paints.is_empty() || units_per_em <= 0.0 {
        return None;
    }
    let scale = pixels_per_em / units_per_em;
    let mut painter = Painter::new(font, (width, height), foreground)?;
    for (glyph, paint) in paints {
        painter.start_glyph(Transform::from_row(
            scale,
            0.0,
            0.0,
            -scale,
            glyph.x,
            glyph.baseline,
        ));
        paint.paint(LocationRef::default(), &mut painter).ok()?;
    }
    Some(painter.into_rgba8())
}

struct Painter<'a> {
    outlines: OutlineGlyphCollection<'a>,
    palette: Vec<Rgba>,
    foreground: Rgba,
    width: u32,
    height: u32,
    transforms: Vec<Transform>,
    clips: Vec<Mask>,
    layers: Vec<Vec<Rgba>>,
}

impl<'a> Painter<'a> {
    fn new(font: &FontRef<'a>, (width, height): (u32, u32), foreground: Rgba) -> Option<Self> {
        let palette = font
            .color_palettes()
            .get(0)
            .map(|palette| {
                palette
                    .colors()
                    .iter()
                    .map(|c| premultiply([c.red, c.green, c.blue, c.alpha]))
                    .collect()
            })
            .unwrap_or_default();
        let mut full = Mask::new(width, height)?;
        full.data_mut().fill(u8::MAX);
        Some(Self {
            outlines: font.outline_glyphs(),
            palette,
            foreground,
            width,
            height,
            transforms: vec![Transform::identity()],
            clips: vec![full],
            layers: vec![vec![TRANSPARENT; (width * height) as usize]],
        })
    }

    /// Resets the transform and clip stacks for the next glyph; what is painted stays.
    fn start_glyph(&mut self, transform: Transform) {
        self.transforms.truncate(1);
        self.transforms[0] = transform;
        self.clips.truncate(1);
        self.layers.truncate(1);
    }

    fn transform(&self) -> Transform {
        self.transforms.last().copied().unwrap_or_default()
    }

    fn color(&self, palette_index: u16, alpha: f32) -> Rgba {
        let base = if palette_index == FOREGROUND_INDEX {
            self.foreground
        } else {
            self.palette
                .get(usize::from(palette_index))
                .copied()
                .unwrap_or(TRANSPARENT)
        };
        base.map(|c| c * alpha)
    }

    fn push_clip_path(&mut self, path: Option<Path>) {
        let mut clip = self
            .clips
            .last()
            .cloned()
            .unwrap_or_else(|| self.empty_mask());
        match path {
            Some(path) => clip.intersect_path(&path, FillRule::Winding, true, self.transform()),
            None => clip.clear(),
        }
        self.clips.push(clip);
    }

    fn empty_mask(&self) -> Mask {
        Mask::new(self.width.max(1), self.height.max(1)).expect("a nonempty mask")
    }

    fn into_rgba8(self) -> Vec<u8> {
        let pixels = self.layers.into_iter().next().unwrap_or_default();
        pixels
            .iter()
            .flat_map(|&pixel| unpremultiply(pixel))
            .collect()
    }
}

impl ColorPainter for Painter<'_> {
    fn push_transform(&mut self, transform: ColrTransform) {
        let pushed = Transform::from_row(
            transform.xx,
            transform.yx,
            transform.xy,
            transform.yy,
            transform.dx,
            transform.dy,
        );
        self.transforms.push(self.transform().pre_concat(pushed));
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let path = self.outlines.get(glyph_id).and_then(|outline| {
            let mut pen = Pen(PathBuilder::new());
            outline
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                    &mut pen,
                )
                .ok()?;
            pen.0.finish()
        });
        self.push_clip_path(path);
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        let path = Rect::from_ltrb(
            clip_box.x_min,
            clip_box.y_min,
            clip_box.x_max,
            clip_box.y_max,
        )
        .map(PathBuilder::from_rect);
        self.push_clip_path(path);
    }

    fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    fn fill(&mut self, brush: Brush<'_>) {
        let Some(to_brush) = self.transform().invert() else {
            return;
        };
        let shader = Shader::new(&brush, |index, alpha| self.color(index, alpha));
        let (Some(clip), Some(layer)) = (self.clips.last(), self.layers.last_mut()) else {
            return;
        };
        let width = self.width as usize;
        for (i, (&coverage, pixel)) in clip.data().iter().zip(layer.iter_mut()).enumerate() {
            if coverage == 0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let mut point = Point::from_xy((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
            to_brush.map_point(&mut point);
            let coverage = f32::from(coverage) / 255.0;
            let source = shader.at(point.x, point.y).map(|c| c * coverage);
            *pixel = composite(source, *pixel, CompositeMode::SrcOver);
        }
    }

    fn push_layer(&mut self, _composite_mode: CompositeMode) {
        self.layers
            .push(vec![TRANSPARENT; (self.width * self.height) as usize]);
    }

    fn pop_layer_with_mode(&mut self, composite_mode: CompositeMode) {
        if self.layers.len() < 2 {
            return;
        }
        let Some(source) = self.layers.pop() else {
            return;
        };
        if let Some(backdrop) = self.layers.last_mut() {
            for (below, above) in backdrop.iter_mut().zip(source) {
                *below = composite(above, *below, composite_mode);
            }
        }
    }
}

/// Collects a glyph outline as a path.
struct Pen(PathBuilder);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.0.close();
    }
}

fn premultiply([r, g, b, a]: [u8; 4]) -> Rgba {
    let alpha = f32::from(a) / 255.0;
    let channel = |c: u8| f32::from(c) / 255.0 * alpha;
    [channel(r), channel(g), channel(b), alpha]
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn unpremultiply([r, g, b, a]: Rgba) -> [u8; 4] {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.0 {
        return [0; 4];
    }
    let byte = |c: f32| ((c / a).clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(r), byte(g), byte(b), (a * 255.0).round() as u8]
}

#[cfg(test)]
mod tests;

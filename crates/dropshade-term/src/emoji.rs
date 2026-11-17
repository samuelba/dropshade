//! Emoji from `COLRv1` fonts, which iced's text renderer draws blank, painted into images.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::widget::image::Handle;
use iced::{Color, Font, Size};
use iced_graphics::text::{cosmic_text, font_system, to_attributes};
use skrifa::FontRef;

use crate::colr::{self, PlacedGlyph};

/// Painted at twice the size they are shown at, so they stay sharp on high-DPI displays.
const OVERSAMPLE: f32 = 2.0;
const CACHE_LIMIT: usize = 4096;

/// A painted emoji and the size to show it at, in logical pixels.
#[derive(Debug, Clone)]
pub struct EmojiImage {
    pub handle: Handle,
    pub size: Size,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Key {
    text: String,
    size: u32,
    line_height: u32,
    color: [u8; 4],
}

/// `text` shaped in `font` and painted as an image with its top left at the line's top, or
/// `None` when the font is not a `COLRv1` font and the text renderer draws it fine.
pub fn image(
    text: &str,
    font: Font,
    size: f32,
    line_height: f32,
    color: Color,
) -> Option<EmojiImage> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Option<EmojiImage>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = Key {
        text: text.to_owned(),
        size: size.to_bits(),
        line_height: line_height.to_bits(),
        color: color.into_rgba8(),
    };
    if let Some(hit) = cache.lock().ok()?.get(&key) {
        return hit.clone();
    }
    let painted = paint(text, font, size, line_height, color);
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, painted.clone());
    }
    painted
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn paint(text: &str, font: Font, size: f32, line_height: f32, color: Color) -> Option<EmojiImage> {
    let mut system = font_system().write().ok()?;
    let raw = system.raw();
    let mut buffer = cosmic_text::Buffer::new(raw, cosmic_text::Metrics::new(size, line_height));
    buffer.set_size(raw, None, None);
    buffer.set_text(
        raw,
        text,
        &to_attributes(font),
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(raw, false);
    let run = buffer.layout_runs().next()?;
    let first = run.glyphs.first()?;
    if run
        .glyphs
        .iter()
        .any(|glyph| glyph.font_id != first.font_id)
    {
        return None;
    }
    let index = raw.db().face(first.font_id)?.index;
    let face = raw.get_font(first.font_id, first.font_weight)?;
    let face = FontRef::from_index(face.data(), index).ok()?;
    let glyphs: Vec<PlacedGlyph> = run
        .glyphs
        .iter()
        .map(|glyph| PlacedGlyph {
            id: u32::from(glyph.glyph_id),
            x: (glyph.x + glyph.font_size * glyph.x_offset) * OVERSAMPLE,
            baseline: (run.line_y + glyph.y - glyph.font_size * glyph.y_offset) * OVERSAMPLE,
        })
        .collect();
    let width = (run.line_w * OVERSAMPLE).ceil() as u32;
    let height = (line_height * OVERSAMPLE).ceil() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    let foreground = [
        color.r * color.a,
        color.g * color.a,
        color.b * color.a,
        color.a,
    ];
    let pixels = colr::paint(
        &face,
        &glyphs,
        first.font_size * OVERSAMPLE,
        (width, height),
        foreground,
    )?;
    Some(EmojiImage {
        handle: Handle::from_rgba(width, height, pixels),
        size: Size::new(width as f32 / OVERSAMPLE, height as f32 / OVERSAMPLE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::emoji_font;

    fn colrv1_emoji_font() -> Option<Font> {
        let emoji = emoji_font()?;
        image("\u{1F600}", emoji.font, 20.0, 24.0, Color::WHITE).map(|_| emoji.font)
    }

    fn pixels(image: &EmojiImage) -> &[u8] {
        match &image.handle {
            Handle::Rgba { pixels, .. } => pixels,
            _ => panic!("emoji images are raw pixels"),
        }
    }

    #[test]
    fn sequences_shape_into_one_glyph_the_width_of_one_emoji() {
        let Some(font) = colrv1_emoji_font() else {
            return;
        };
        let wave = image("\u{1F44B}", font, 20.0, 24.0, Color::WHITE).expect("a wave");
        let toned =
            image("\u{1F44B}\u{1F3FD}", font, 20.0, 24.0, Color::WHITE).expect("a toned wave");
        let family = image(
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            font,
            20.0,
            24.0,
            Color::WHITE,
        )
        .expect("a family");
        assert_eq!(toned.size, wave.size);
        assert_eq!(family.size, wave.size);
        assert_ne!(
            pixels(&toned),
            pixels(&wave),
            "the skin tone changes the colours"
        );
        assert!((wave.size.height - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn repeated_emoji_reuse_their_image() {
        let Some(font) = colrv1_emoji_font() else {
            return;
        };
        let first = image("\u{1F680}", font, 20.0, 24.0, Color::WHITE).expect("a rocket");
        let again = image("\u{1F680}", font, 20.0, 24.0, Color::WHITE).expect("a rocket");
        assert_eq!(first.handle.id(), again.handle.id());
    }

    #[test]
    fn text_fonts_are_left_to_the_text_renderer() {
        assert!(image("abc", Font::MONOSPACE, 20.0, 24.0, Color::WHITE).is_none());
    }
}

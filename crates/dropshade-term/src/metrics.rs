//! Measures monospace cells and text widths with the font system iced renders with.

use std::sync::OnceLock;

use iced::Font;
use iced_graphics::text::{cosmic_text, font_system, to_attributes};

use crate::size::CellSize;

/// Cell size for `font` at `size` pixels, with `line_height` as a multiple of the size.
pub fn cell_size(font: Font, size: f32, line_height: f32) -> CellSize {
    let height = (size * line_height).ceil();
    let fallback = CellSize {
        width: (size * 0.6).ceil(),
        height,
    };
    let Ok(mut system) = font_system().write() else {
        tracing::warn!("font system lock poisoned; using estimated cell size");
        return fallback;
    };
    let raw = system.raw();
    let mut buffer = cosmic_text::Buffer::new(raw, cosmic_text::Metrics::new(size, height));
    buffer.set_text(
        raw,
        "MMMMMMMMMM",
        &to_attributes(font),
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(raw, false);
    buffer
        .layout_runs()
        .next()
        .map(|run| CellSize {
            width: run.line_w / 10.0,
            height,
        })
        .filter(|cell| cell.width > 0.0)
        .unwrap_or(fallback)
}

/// Width of `text` laid out on one line in `font` at `size` pixels; an estimate if the font
/// system is unavailable.
#[allow(clippy::cast_precision_loss)]
pub fn text_width(text: &str, font: Font, size: f32) -> f32 {
    let Ok(mut system) = font_system().write() else {
        return text.chars().count() as f32 * size * 0.6;
    };
    let raw = system.raw();
    let mut buffer = cosmic_text::Buffer::new(raw, cosmic_text::Metrics::new(size, size * 1.3));
    buffer.set_size(raw, None, None);
    buffer.set_text(
        raw,
        text,
        &to_attributes(font),
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(raw, false);
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max)
}

/// Whether a font family is installed (case-insensitive), so a typo can be reported.
pub fn font_installed(family: &str) -> bool {
    let Ok(mut system) = font_system().write() else {
        return true;
    };
    system.raw().db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })
}

/// Color emoji families, most preferred first.
const EMOJI_FAMILIES: &[&str] = &[
    "Noto Color Emoji",
    "Twemoji",
    "JoyPixels",
    "OpenMoji Color",
    "Apple Color Emoji",
    "Segoe UI Emoji",
];

/// The installed color emoji font and how wide its emoji are, in ems.
#[derive(Debug, Clone, Copy)]
pub struct EmojiFont {
    pub font: Font,
    pub advance: f32,
}

/// The installed color emoji font, looked up once. Text fonts often carry monochrome
/// versions of symbols like ⚠, which the font fallback would pick before any emoji font.
pub fn emoji_font() -> Option<EmojiFont> {
    static EMOJI: OnceLock<Option<EmojiFont>> = OnceLock::new();
    *EMOJI.get_or_init(|| {
        let family = {
            let mut system = font_system().write().ok()?;
            let families: Vec<String> = system
                .raw()
                .db()
                .faces()
                .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
                .collect();
            EMOJI_FAMILIES
                .iter()
                .find_map(|wanted| families.iter().find(|f| f.eq_ignore_ascii_case(wanted)))
                .or_else(|| families.iter().find(|f| f.contains("Emoji")))?
                .clone()
        };
        let name: &'static str = Box::leak(family.into_boxed_str());
        tracing::debug!("drawing emoji with \"{name}\"");
        let font = Font::with_name(name);
        let advance = text_width("\u{1F600}", font, 100.0) / 100.0;
        Some(EmojiFont {
            font,
            advance: if advance > 0.0 { advance } else { 1.0 },
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monospace_cells_have_a_positive_width_and_rounded_height() {
        let cell = cell_size(Font::MONOSPACE, 15.0, 1.25);
        assert!(
            cell.width > 0.0 && cell.width < 15.0,
            "width {}",
            cell.width
        );
        assert!((cell.height - 19.0).abs() < f32::EPSILON);
    }

    #[test]
    fn longer_text_is_wider() {
        let short = text_width("ab", Font::DEFAULT, 13.0);
        let long = text_width("abcdefgh", Font::DEFAULT, 13.0);
        assert!(short > 0.0 && long > short, "{short} vs {long}");
        assert!(text_width("", Font::DEFAULT, 13.0).abs() < f32::EPSILON);
    }

    #[test]
    fn emoji_fonts_have_a_usable_width_when_installed() {
        if let Some(emoji) = emoji_font() {
            assert!(
                emoji.advance > 0.5 && emoji.advance < 2.0,
                "{}",
                emoji.advance
            );
        }
    }

    #[test]
    fn made_up_fonts_are_not_installed() {
        assert!(!font_installed("Definitely Not A Real Font 42"));
    }
}

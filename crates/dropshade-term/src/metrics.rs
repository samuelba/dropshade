//! Measures monospace cell dimensions with the font system iced renders with.

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
}

use skrifa::{FontRef, MetadataProvider};

use super::{PlacedGlyph, paint};

const FOREGROUND: super::Rgba = [1.0, 1.0, 1.0, 1.0];

/// A `COLRv1` emoji font, when one is installed where distributions put it.
fn colrv1_font() -> Option<Vec<u8>> {
    [
        "/usr/share/fonts/google-noto-color-emoji-fonts/Noto-COLRv1.ttf",
        "/usr/share/fonts/noto/Noto-COLRv1.ttf",
        "/usr/share/fonts/truetype/noto/Noto-COLRv1.ttf",
    ]
    .iter()
    .find_map(|path| std::fs::read(path).ok())
}

fn glyph(font: &FontRef<'_>, c: char) -> PlacedGlyph {
    let id = font
        .charmap()
        .map(c)
        .expect("the emoji font maps the emoji");
    PlacedGlyph {
        id: id.to_u32(),
        x: 0.0,
        baseline: 52.0,
    }
}

#[test]
fn paints_a_colrv1_emoji_in_colour() {
    let Some(data) = colrv1_font() else {
        return;
    };
    let font = FontRef::new(&data).expect("a font");
    let pixels = paint(
        &font,
        &[glyph(&font, '\u{1F600}')],
        54.0,
        (64, 64),
        FOREGROUND,
    )
    .expect("a COLRv1 paint");
    assert_eq!(pixels.len(), 64 * 64 * 4);
    let opaque: Vec<&[u8]> = pixels.chunks(4).filter(|p| p[3] > 200).collect();
    assert!(
        opaque.len() > 64 * 64 / 3,
        "the face fills much of the image"
    );
    let yellow = opaque
        .iter()
        .any(|p| p[0] > 200 && p[1] > 150 && p[2] < 100);
    assert!(yellow, "the grinning face is yellow");
}

#[test]
fn glyphs_without_a_colrv1_paint_are_left_to_the_text_renderer() {
    let Some(data) = colrv1_font() else {
        return;
    };
    let font = FontRef::new(&data).expect("a font");
    let notdef = PlacedGlyph {
        id: 0,
        x: 0.0,
        baseline: 52.0,
    };
    assert!(paint(&font, &[notdef], 54.0, (64, 64), FOREGROUND).is_none());
    assert!(paint(&font, &[], 54.0, (64, 64), FOREGROUND).is_none());
}

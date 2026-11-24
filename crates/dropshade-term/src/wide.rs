//! Picks fonts for double-width characters like 한, 中 and あ.
//!
//! The font fallback can settle on a font that lacks a Hangul syllable but has its jamo, so the
//! shaper splits 한 into ㅎㅏㄴ, three glyphs that spill over the next cells. A wide character is
//! drawn with the terminal font when that font has it, and with an installed CJK font otherwise.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::Font;
use iced::font::Family;
use iced_graphics::text::{cosmic_text, font_system, to_attributes};

use crate::metrics::font_installed;

/// A font that draws a wide character as one glyph, and the glyph's advance in ems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WideGlyph {
    pub family: Family,
    pub advance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Korean,
    Japanese,
    Simplified,
    Traditional,
    HongKong,
}

const REGIONS: [Region; 5] = [
    Region::Korean,
    Region::Japanese,
    Region::Simplified,
    Region::Traditional,
    Region::HongKong,
];

impl Region {
    /// CJK families for the region, monospaced ones first.
    fn families(self) -> &'static [&'static str] {
        match self {
            Self::Korean => &[
                "Noto Sans Mono CJK KR",
                "Noto Sans CJK KR",
                "Source Han Mono K",
                "Source Han Sans K",
                "Source Han Sans KR",
                "D2Coding",
                "NanumGothicCoding",
                "Nanum Gothic",
                "UnDotum",
            ],
            Self::Japanese => &[
                "Noto Sans Mono CJK JP",
                "Noto Sans CJK JP",
                "Source Han Mono",
                "Source Han Sans",
                "Source Han Sans JP",
                "IPAGothic",
                "TakaoGothic",
                "VL Gothic",
            ],
            Self::Simplified => &[
                "Noto Sans Mono CJK SC",
                "Noto Sans CJK SC",
                "Source Han Mono SC",
                "Source Han Sans SC",
                "Source Han Sans CN",
                "WenQuanYi Zen Hei Mono",
                "WenQuanYi Micro Hei Mono",
                "WenQuanYi Zen Hei",
            ],
            Self::Traditional => &[
                "Noto Sans Mono CJK TC",
                "Noto Sans CJK TC",
                "Source Han Mono TC",
                "Source Han Sans TC",
                "Source Han Sans TW",
            ],
            Self::HongKong => &[
                "Noto Sans Mono CJK HK",
                "Noto Sans CJK HK",
                "Source Han Mono HC",
                "Source Han Sans HC",
                "Source Han Sans HK",
            ],
        }
    }

    /// The region whose glyph shapes suit `c`: Hangul is Korean and kana Japanese; Han
    /// characters, shared by all of them, follow the locale.
    fn of(c: char, locale: &str) -> Self {
        match u32::from(c) {
            0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Self::Korean,
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF65..=0xFF9F => Self::Japanese,
            _ => Self::from_locale(locale),
        }
    }

    fn from_locale(locale: &str) -> Self {
        let locale = locale.replace('_', "-");
        if locale.starts_with("ja") {
            Self::Japanese
        } else if locale.starts_with("ko") {
            Self::Korean
        } else if locale.starts_with("zh-HK") || locale.starts_with("zh-MO") {
            Self::HongKong
        } else if locale.starts_with("zh-TW") || locale.contains("Hant") {
            Self::Traditional
        } else {
            Self::Simplified
        }
    }
}

type Cache = HashMap<(Family, char), Option<WideGlyph>>;

/// The font to draw the wide character `c` with, given the terminal font `base`; `None` when
/// no installed font has it as a single glyph. Looked up once per character and font.
pub fn glyph(base: Family, c: char) -> Option<WideGlyph> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache.lock().ok()?.get(&(base, c)) {
        return *hit;
    }
    let found = find(base, c);
    if let Ok(mut cache) = cache.lock() {
        cache.insert((base, c), found);
    }
    found
}

fn find(base: Family, c: char) -> Option<WideGlyph> {
    let text = c.to_string();
    let own_face = shape("M", base).first().map(|glyph| glyph.0);
    if let Some(advance) = single_glyph(&text, base, |id| Some(id) == own_face) {
        return Some(WideGlyph {
            family: base,
            advance,
        });
    }
    let locale = font_system().write().ok()?.raw().locale().to_owned();
    let first = Region::of(c, &locale);
    let regions = std::iter::once(first).chain(REGIONS.into_iter().filter(|&r| r != first));
    regions
        .flat_map(Region::families)
        .filter(|name| font_installed(name))
        .find_map(|&name| {
            let family = Family::Name(name);
            let advance = single_glyph(&text, family, |id| face_is(id, name))?;
            tracing::debug!("drawing {c:?} with \"{name}\"");
            Some(WideGlyph { family, advance })
        })
}

/// The advance in ems of `text` shaped in `family`, if it is one glyph from a face `accept`s.
fn single_glyph(
    text: &str,
    family: Family,
    accept: impl Fn(cosmic_text::fontdb::ID) -> bool,
) -> Option<f32> {
    match shape(text, family).as_slice() {
        [(id, advance)] if accept(*id) && *advance > 0.0 => Some(*advance),
        _ => None,
    }
}

const PROBE_SIZE: f32 = 100.0;

/// The faces and advances (in ems) of the glyphs `text` is shaped into, fallbacks included.
fn shape(text: &str, family: Family) -> Vec<(cosmic_text::fontdb::ID, f32)> {
    let Ok(mut system) = font_system().write() else {
        return Vec::new();
    };
    let raw = system.raw();
    let metrics = cosmic_text::Metrics::new(PROBE_SIZE, PROBE_SIZE);
    let mut buffer = cosmic_text::Buffer::new(raw, metrics);
    buffer.set_size(raw, None, None);
    let font = Font {
        family,
        ..Font::DEFAULT
    };
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
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| (glyph.font_id, glyph.w / PROBE_SIZE))
        .collect()
}

fn face_is(id: cosmic_text::fontdb::ID, family: &str) -> bool {
    let Ok(mut system) = font_system().write() else {
        return false;
    };
    system.raw().db().face(id).is_some_and(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hangul_and_kana_pick_their_region_and_han_follows_the_locale() {
        assert_eq!(Region::of('한', "en-US"), Region::Korean);
        assert_eq!(Region::of('あ', "zh-CN"), Region::Japanese);
        assert_eq!(Region::of('中', "ja-JP"), Region::Japanese);
        assert_eq!(Region::of('中', "zh_TW"), Region::Traditional);
        assert_eq!(Region::of('中', "zh-HK"), Region::HongKong);
        assert_eq!(Region::of('中', "en-US"), Region::Simplified);
    }

    #[test]
    fn wide_characters_are_one_glyph_that_fits_two_cells_when_a_font_has_them() {
        for c in ['한', '글', '中', 'あ', 'Ａ'] {
            if let Some(glyph) = glyph(Family::Monospace, c) {
                assert!(
                    glyph.advance > 0.5 && glyph.advance <= 1.25,
                    "{c}: {}",
                    glyph.advance
                );
            }
        }
    }
}

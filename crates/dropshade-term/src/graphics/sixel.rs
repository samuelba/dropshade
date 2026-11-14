//! Decodes sixel images (the data of `DCS P1;P2;P3 q … ST`).
//!
//! Pixels nothing was drawn on stay transparent, so the terminal background shows through.

use super::kitty::{self, Error};
use super::table::Image;

const REGISTERS: usize = 1024;
/// Most repeats of one sixel taken; wider runs are cut at the image size limit anyway.
const MAX_REPEAT: usize = kitty::MAX_SIDE as usize;

/// The VT340's default colour registers, in percent.
const VT340: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (20, 20, 80),
    (80, 13, 13),
    (20, 80, 20),
    (80, 20, 80),
    (20, 80, 80),
    (80, 80, 20),
    (53, 53, 53),
    (26, 26, 26),
    (33, 33, 60),
    (60, 26, 26),
    (33, 60, 33),
    (60, 33, 60),
    (33, 60, 60),
    (60, 60, 33),
    (80, 80, 80),
];

/// One instruction of the sixel data.
enum Step {
    /// `"Pan;Pad;Ph;Pv`: the image size the sender declares.
    Size(usize, usize),
    /// `#Pc;Pu;Px;Py;Pz`: sets a colour register to RGBA and draws with it.
    Define(usize, [u8; 4]),
    /// `#Pc`: draws with a colour register from here.
    Select(usize),
    /// A sixel: six pixels of one column, `count` times from `x`, in the band starting at `y`.
    Paint {
        x: usize,
        y: usize,
        bits: u8,
        count: usize,
    },
}

/// Decodes the data after the `q`.
pub fn decode(data: &[u8]) -> Result<Image, Error> {
    let (mut width, mut height) = (0, 0);
    walk(data, |step| match step {
        Step::Size(w, h) => {
            width = width.max(w);
            height = height.max(h);
        }
        Step::Paint { x, y, bits, count } if bits != 0 => {
            let below_lowest = 8 - bits.leading_zeros() as usize;
            width = width.max(x.saturating_add(count));
            height = height.max(y.saturating_add(below_lowest));
        }
        _ => {}
    });
    let side = |n: usize| u32::try_from(n.min(MAX_REPEAT)).unwrap_or(kitty::MAX_SIDE);
    let (width, height) = (side(width), side(height));
    kitty::check_size(width, height)?;
    let (columns, rows) = (width as usize, height as usize);

    let mut pixels = vec![0u8; columns * rows * 4];
    let mut registers: Vec<[u8; 4]> = (0..REGISTERS)
        .map(|index| {
            VT340
                .get(index)
                .map_or([0, 0, 0, 0xff], |&(r, g, b)| percent(r, g, b))
        })
        .collect();
    let mut pen = registers[0];
    walk(data, |step| match step {
        Step::Size(..) => {}
        Step::Define(register, rgba) => {
            registers[register] = rgba;
            pen = rgba;
        }
        Step::Select(register) => pen = registers[register],
        Step::Paint { x, y, bits, count } => {
            let end = x.saturating_add(count).min(columns);
            for bit in 0..6 {
                let row = y.saturating_add(bit);
                if bits & (1 << bit) == 0 || row >= rows {
                    continue;
                }
                for column in x..end {
                    let at = (row * columns + column) * 4;
                    pixels[at..at + 4].copy_from_slice(&pen);
                }
            }
        }
    });
    Ok(Image::from_rgba(width, height, pixels))
}

/// Calls `on` with each instruction of `data`, keeping track of the drawing position.
#[allow(clippy::many_single_char_names)]
fn walk(data: &[u8], mut on: impl FnMut(Step)) {
    let (mut x, mut y) = (0usize, 0usize);
    let mut index = 0;
    let mut repeat = 1;
    while index < data.len() {
        let byte = data[index];
        index += 1;
        match byte {
            b'?'..=b'~' => {
                let count = std::mem::replace(&mut repeat, 1);
                on(Step::Paint {
                    x,
                    y,
                    bits: byte - b'?',
                    count,
                });
                x = x.saturating_add(count);
            }
            b'!' => {
                let (numbers, next) = numbers(data, index);
                index = next;
                repeat = numbers.first().copied().unwrap_or(1).clamp(1, MAX_REPEAT);
            }
            b'$' => x = 0,
            b'-' => {
                x = 0;
                y = y.saturating_add(6);
            }
            b'"' => {
                let (numbers, next) = numbers(data, index);
                index = next;
                if let [_, _, width, height, ..] = numbers[..] {
                    on(Step::Size(width, height));
                }
            }
            b'#' => {
                let (numbers, next) = numbers(data, index);
                index = next;
                match numbers[..] {
                    [register, space, a, b, c, ..] => {
                        if let Some(rgba) = color(space, a, b, c) {
                            on(Step::Define(register % REGISTERS, rgba));
                        }
                    }
                    [register, ..] => on(Step::Select(register % REGISTERS)),
                    [] => {}
                }
            }
            _ => {}
        }
    }
}

/// Reads `n;n;…` from `start`; returns the numbers and where they end.
fn numbers(data: &[u8], start: usize) -> (Vec<usize>, usize) {
    let mut numbers = Vec::new();
    let mut current: Option<usize> = None;
    let mut index = start;
    while let Some(&byte) = data.get(index) {
        match byte {
            b'0'..=b'9' => {
                let digit = usize::from(byte - b'0');
                current = Some(
                    current
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
            }
            b';' => numbers.push(current.take().unwrap_or(0)),
            _ => break,
        }
        index += 1;
    }
    if let Some(last) = current {
        numbers.push(last);
    }
    (numbers, index)
}

/// A colour from its space (1 HLS, 2 RGB) and components.
fn color(space: usize, a: usize, b: usize, c: usize) -> Option<[u8; 4]> {
    let clamp = |n: usize| u8::try_from(n.min(100)).unwrap_or(100);
    match space {
        2 => Some(percent(clamp(a), clamp(b), clamp(c))),
        1 => Some(hls(a % 360, clamp(b), clamp(c))),
        _ => None,
    }
}

fn percent(r: u8, g: u8, b: u8) -> [u8; 4] {
    let scale = |n: u8| u8::try_from((u32::from(n.min(100)) * 255 + 50) / 100).unwrap_or(255);
    [scale(r), scale(g), scale(b), 0xff]
}

/// DEC's HLS: hue 0° is blue, 120° red and 240° green; lightness and saturation in percent.
#[allow(clippy::many_single_char_names)]
fn hls(hue: usize, lightness: u8, saturation: u8) -> [u8; 4] {
    #[allow(clippy::cast_precision_loss)]
    let hue = ((hue + 240) % 360) as f32 / 360.0;
    let (l, s) = (f32::from(lightness) / 100.0, f32::from(saturation) / 100.0);
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        v
    };
    [
        channel(hue + 1.0 / 3.0),
        channel(hue),
        channel(hue - 1.0 / 3.0),
        0xff,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: usize, y: usize) -> [u8; 4] {
        let iced::widget::image::Handle::Rgba { ref pixels, .. } = image.handle else {
            panic!("not an RGBA handle");
        };
        let at = (y * image.width as usize + x) * 4;
        pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn a_full_sixel_paints_six_pixels() {
        let image = decode(b"#0;2;100;0;0~").unwrap();
        assert_eq!((image.width, image.height), (1, 6));
        assert!((0..6).all(|y| pixel(&image, 0, y) == [255, 0, 0, 255]));
    }

    #[test]
    fn the_height_ends_at_the_lowest_painted_pixel() {
        let image = decode(b"#1;2;0;0;100@").unwrap();
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(pixel(&image, 0, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn repeats_carriage_returns_and_new_lines() {
        let image = decode(b"#1;2;0;100;0!3~$#2;2;100;0;0@-#1~").unwrap();
        assert_eq!((image.width, image.height), (3, 12));
        assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 0, 1), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 2, 5), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 0, 11), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 1, 11), [0, 0, 0, 0]);
    }

    #[test]
    fn the_declared_size_counts_and_unpainted_pixels_are_clear() {
        let image = decode(b"\"1;1;4;8#0;2;100;100;100@").unwrap();
        assert_eq!((image.width, image.height), (4, 8));
        assert_eq!(pixel(&image, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&image, 3, 7), [0, 0, 0, 0]);
    }

    #[test]
    fn registers_start_with_the_vt340_colours() {
        let image = decode(b"#2@").unwrap();
        assert_eq!(pixel(&image, 0, 0), percent(80, 13, 13));
    }

    #[test]
    fn hls_hues_start_at_blue() {
        assert_eq!(hls(0, 50, 100), [0, 0, 255, 255]);
        assert_eq!(hls(120, 50, 100), [255, 0, 0, 255]);
        assert_eq!(hls(240, 50, 100), [0, 255, 0, 255]);
    }

    #[test]
    fn an_empty_image_is_an_error() {
        assert!(decode(b"").is_err());
        assert!(decode(b"#0;2;0;0;0??").is_err());
    }

    #[test]
    fn huge_repeats_are_cut_at_the_size_limit() {
        let image = decode(b"!99999999@").unwrap();
        assert_eq!(image.width, kitty::MAX_SIDE);
    }
}

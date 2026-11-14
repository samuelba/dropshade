//! Kitty graphics commands: their control keys, decoding the image data, and replies.
//!
//! See <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.

use std::borrow::Cow;
use std::fmt::Write;
use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::read::ZlibDecoder;

use super::table::Image;

/// Largest decoded image accepted, in bytes of RGBA: 4096 × 4096 pixels.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Largest width or height accepted, in pixels.
pub const MAX_SIDE: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    Transmit,
    TransmitAndPut,
    Put,
    Query,
    Delete,
    Unsupported,
}

/// The control keys of one command; absent numbers are 0.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub action: Action,
    pub quiet: u8,
    pub id: u32,
    pub number: u32,
    pub placement_id: u32,
    pub format: u32,
    pub medium: u8,
    pub compressed: bool,
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub more: bool,
    pub cursor_stays: bool,
    pub virtual_placement: bool,
    pub delete: u8,
    /// Only `m` and `q` are set: the next chunk of a chunked transmission.
    pub continuation: bool,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            action: Action::Transmit,
            quiet: 0,
            id: 0,
            number: 0,
            placement_id: 0,
            format: 32,
            medium: b'd',
            compressed: false,
            width: 0,
            height: 0,
            columns: 0,
            rows: 0,
            more: false,
            cursor_stays: false,
            virtual_placement: false,
            delete: b'a',
            continuation: false,
        }
    }
}

/// Splits a command body (after the `G`) into its control keys and its payload.
pub fn parse(body: &[u8]) -> (Command, &[u8]) {
    let (control, payload) = match body.iter().position(|&byte| byte == b';') {
        Some(split) => (&body[..split], &body[split + 1..]),
        None => (body, &[][..]),
    };
    let mut command = Command::default();
    let mut chunk_keys_only = true;
    let mut has_more = false;
    for pair in control.split(|&byte| byte == b',') {
        let [key, b'=', value @ ..] = pair else {
            continue;
        };
        has_more |= *key == b'm';
        chunk_keys_only &= matches!(key, b'm' | b'q');
        let number = || {
            std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0)
        };
        let letter = value.first().copied().unwrap_or(0);
        match key {
            b'a' => {
                command.action = match letter {
                    b't' => Action::Transmit,
                    b'T' => Action::TransmitAndPut,
                    b'p' => Action::Put,
                    b'q' => Action::Query,
                    b'd' => Action::Delete,
                    _ => Action::Unsupported,
                }
            }
            b'q' => command.quiet = u8::try_from(number()).unwrap_or(2),
            b'i' => command.id = number(),
            b'I' => command.number = number(),
            b'p' => command.placement_id = number(),
            b'f' => command.format = number(),
            b't' => command.medium = letter,
            b'o' => command.compressed = letter == b'z',
            b's' => command.width = number(),
            b'v' => command.height = number(),
            b'c' => command.columns = number(),
            b'r' => command.rows = number(),
            b'm' => command.more = number() == 1,
            b'C' => command.cursor_stays = number() == 1,
            b'U' => command.virtual_placement = number() == 1,
            b'd' => command.delete = letter,
            _ => {}
        }
    }
    command.continuation = has_more && chunk_keys_only;
    (command, payload)
}

/// Why a command failed, as kitty reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub message: &'static str,
}

pub const fn error(code: &'static str, message: &'static str) -> Error {
    Error { code, message }
}

/// Decodes the base64 payload of a transmission into an image.
pub fn decode(command: &Command, payload: &[u8]) -> Result<Image, Error> {
    if command.medium != b'd' {
        return Err(error("EINVAL", "only direct transmission is supported"));
    }
    let payload: Cow<[u8]> = if payload.iter().any(u8::is_ascii_whitespace) {
        Cow::Owned(
            payload
                .iter()
                .copied()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect(),
        )
    } else {
        Cow::Borrowed(payload)
    };
    let data = STANDARD
        .decode(&payload)
        .map_err(|_| error("EINVAL", "the data is not valid base64"))?;
    let data = if command.compressed {
        inflate(&data)?
    } else {
        data
    };
    if data.is_empty() {
        return Err(error("ENODATA", "no image data"));
    }
    match command.format {
        24 => raw(command, data, 3),
        32 => raw(command, data, 4),
        100 => png(&data),
        _ => Err(error("EINVAL", "unknown image format")),
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let limit = u64::try_from(MAX_IMAGE_BYTES).unwrap_or(u64::MAX) + 1;
    ZlibDecoder::new(data)
        .take(limit)
        .read_to_end(&mut out)
        .map_err(|_| error("EINVAL", "the data is not valid zlib"))?;
    if out.len() > MAX_IMAGE_BYTES {
        return Err(error("EFBIG", "the image is too large"));
    }
    Ok(out)
}

/// Fails for an empty image or one too large to keep.
pub fn check_size(width: u32, height: u32) -> Result<(), Error> {
    if width == 0 || height == 0 {
        return Err(error("EINVAL", "the image has no size"));
    }
    let bytes = u64::from(width) * u64::from(height) * 4;
    if width > MAX_SIDE
        || height > MAX_SIDE
        || bytes > u64::try_from(MAX_IMAGE_BYTES).unwrap_or(u64::MAX)
    {
        return Err(error("EFBIG", "the image is too large"));
    }
    Ok(())
}

fn raw(command: &Command, data: Vec<u8>, channels: usize) -> Result<Image, Error> {
    let (width, height) = (command.width, command.height);
    check_size(width, height)?;
    let pixels = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| error("EFBIG", "the image is too large"))?;
    if data.len() < pixels * channels {
        return Err(error("ENODATA", "not enough image data for its size"));
    }
    let rgba = if channels == 4 {
        let mut data = data;
        data.truncate(pixels * 4);
        data
    } else {
        rgb_to_rgba(&data[..pixels * 3])
    };
    Ok(Image::from_rgba(width, height, rgba))
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    rgb.as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[r, g, b]| [r, g, b, 0xff])
        .collect()
}

fn png(data: &[u8]) -> Result<Image, Error> {
    let bad = |_| error("EBADPNG", "the PNG could not be decoded");
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(bad)?;
    let (width, height) = reader.info().size();
    check_size(width, height)?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).map_err(bad)?;
    buffer.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => rgb_to_rgba(&buffer),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 0xff]).collect(),
        png::ColorType::Indexed => return Err(error("EBADPNG", "unexpanded palette")),
    };
    Ok(Image::from_rgba(width, height, rgba))
}

/// The reply to a command, or `None` when the command asked for none or names no image.
pub fn reply(command: &Command, id: u32, result: Result<(), &Error>) -> Option<Vec<u8>> {
    if id == 0 && command.number == 0 {
        return None;
    }
    let message = match result {
        Ok(()) if command.quiet == 0 => "OK".to_owned(),
        Err(error) if command.quiet < 2 => format!("{}:{}", error.code, error.message),
        _ => return None,
    };
    let mut keys = format!("i={id}");
    if command.number != 0 {
        let _ = write!(keys, ",I={}", command.number);
    }
    if command.placement_id != 0 {
        let _ = write!(keys, ",p={}", command.placement_id);
    }
    Some(format!("\x1b_G{keys};{message}\x1b\\").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(bytes: &[u8]) -> Vec<u8> {
        STANDARD.encode(bytes).into_bytes()
    }

    fn rgba(image: &Image) -> Vec<u8> {
        match image.handle {
            iced::widget::image::Handle::Rgba { ref pixels, .. } => pixels.to_vec(),
            _ => panic!("not an RGBA handle"),
        }
    }

    #[test]
    fn reads_the_control_keys_and_the_payload() {
        let (command, payload) = parse(b"a=T,f=100,i=31,c=10,r=5,q=2,m=1,C=1,U=1;AAAA");
        assert_eq!(command.action, Action::TransmitAndPut);
        assert_eq!(command.format, 100);
        assert_eq!(command.id, 31);
        assert_eq!((command.columns, command.rows), (10, 5));
        assert_eq!(command.quiet, 2);
        assert!(command.more && command.cursor_stays && command.virtual_placement);
        assert!(!command.continuation);
        assert!(parse(b"m=0;AAAA").0.continuation);
        assert!(parse(b"q=2,m=1;AAAA").0.continuation);
        assert_eq!(payload, b"AAAA");
    }

    #[test]
    fn keys_have_kittys_defaults() {
        let (command, payload) = parse(b"");
        assert_eq!(command, Command::default());
        assert_eq!(command.format, 32);
        assert_eq!(payload, b"");
        assert_eq!(parse(b"a=d,d=I,i=4").0.delete, b'I');
    }

    #[test]
    fn decodes_rgb_and_rgba() {
        let rgb = Command {
            format: 24,
            width: 2,
            height: 1,
            ..Command::default()
        };
        let image = decode(&rgb, &encode(&[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(rgba(&image), [1, 2, 3, 255, 4, 5, 6, 255]);

        let rgba_command = Command {
            width: 1,
            height: 1,
            ..Command::default()
        };
        let image = decode(&rgba_command, &encode(&[9, 8, 7, 6])).unwrap();
        assert_eq!(rgba(&image), [9, 8, 7, 6]);
    }

    #[test]
    fn decodes_zlib_compressed_data() {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&[10, 20, 30, 40]).unwrap();
        let command = Command {
            width: 1,
            height: 1,
            compressed: true,
            ..Command::default()
        };
        let image = decode(&command, &encode(&encoder.finish().unwrap())).unwrap();
        assert_eq!(rgba(&image), [10, 20, 30, 40]);
    }

    #[test]
    fn decodes_png() {
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 0, 255, 0]).unwrap();
        }
        let command = Command {
            format: 100,
            ..Command::default()
        };
        let image = decode(&command, &encode(&png_bytes)).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(rgba(&image), [255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn rejects_bad_data() {
        let command = Command {
            width: 2,
            height: 2,
            ..Command::default()
        };
        assert_eq!(decode(&command, b"!!!").unwrap_err().code, "EINVAL");
        assert_eq!(
            decode(&command, &encode(&[1, 2])).unwrap_err().code,
            "ENODATA"
        );
        let file = Command {
            medium: b'f',
            ..command.clone()
        };
        assert_eq!(
            decode(&file, &encode(b"/etc/passwd")).unwrap_err().code,
            "EINVAL"
        );
        let huge = Command {
            width: MAX_SIDE + 1,
            height: 1,
            ..Command::default()
        };
        assert_eq!(decode(&huge, &encode(&[0; 4])).unwrap_err().code, "EFBIG");
        let png = Command {
            format: 100,
            ..Command::default()
        };
        assert_eq!(
            decode(&png, &encode(b"not a png")).unwrap_err().code,
            "EBADPNG"
        );
    }

    #[test]
    fn replies_name_the_image_and_respect_quiet() {
        let command = Command {
            id: 31,
            ..Command::default()
        };
        assert_eq!(reply(&command, 31, Ok(())).unwrap(), b"\x1b_Gi=31;OK\x1b\\");
        let failed = error("ENOENT", "no such image");
        assert_eq!(
            reply(&command, 31, Err(&failed)).unwrap(),
            b"\x1b_Gi=31;ENOENT:no such image\x1b\\"
        );
        let quiet = Command {
            quiet: 1,
            ..command.clone()
        };
        assert_eq!(reply(&quiet, 31, Ok(())), None);
        assert!(reply(&quiet, 31, Err(&failed)).is_some());
        let silent = Command {
            quiet: 2,
            ..command
        };
        assert_eq!(reply(&silent, 31, Err(&failed)), None);
    }

    #[test]
    fn a_numbered_image_reply_carries_the_assigned_id() {
        let command = Command {
            number: 13,
            ..Command::default()
        };
        assert_eq!(
            reply(&command, 7, Ok(())).unwrap(),
            b"\x1b_Gi=7,I=13;OK\x1b\\"
        );
    }

    #[test]
    fn anonymous_images_get_no_reply() {
        assert_eq!(reply(&Command::default(), 0, Ok(())), None);
    }
}

//! Reads the `text/uri-list` (RFC 2483) a file manager offers for dragged files.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// The local files in `list`, in order; comments, blank lines and non-file URIs are skipped.
pub fn paths(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(file_path)
        .collect()
}

fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path_start = rest.find('/')?;
    let host = &rest[..path_start];
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    let bytes = percent_decoded(&rest[path_start..]);
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

fn percent_decoded(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_file_in_order() {
        assert_eq!(
            paths("file:///home/sam/a.txt\r\nfile:///tmp/b\r\n"),
            [PathBuf::from("/home/sam/a.txt"), PathBuf::from("/tmp/b")]
        );
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            paths("file:///tmp/my%20file%27s%E2%9C%93.txt"),
            [PathBuf::from("/tmp/my file's✓.txt")]
        );
    }

    #[test]
    fn keeps_a_lone_percent_sign() {
        assert_eq!(paths("file:///tmp/100%"), [PathBuf::from("/tmp/100%")]);
    }

    #[test]
    fn accepts_localhost_and_rejects_other_hosts() {
        assert_eq!(
            paths("file://localhost/etc/hosts\nfile://server/share/x"),
            [PathBuf::from("/etc/hosts")]
        );
    }

    #[test]
    fn skips_comments_blank_lines_and_other_schemes() {
        assert_eq!(
            paths("# from nautilus\n\nhttps://example.com/x\nfile:///a\n"),
            [PathBuf::from("/a")]
        );
    }
}

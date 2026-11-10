//! Finds URLs in terminal output and decides which links are safe to open.

const SCHEMES: [&str; 5] = ["https://", "http://", "file://", "ftp://", "mailto:"];

/// The URL covering `column` in a row given as one char per cell.
pub fn url_at(row: &[char], column: usize) -> Option<String> {
    let mut start = 0;
    while start < row.len() {
        let Some(scheme_len) = scheme_at(&row[start..]) else {
            start += 1;
            continue;
        };
        let end = url_end(row, start, scheme_len);
        if end > start + scheme_len && (start..end).contains(&column) {
            return Some(row[start..end].iter().collect());
        }
        start = end.max(start + 1);
    }
    None
}

fn scheme_at(text: &[char]) -> Option<usize> {
    SCHEMES
        .iter()
        .find(|scheme| starts_with_ignore_case(text, scheme))
        .map(|scheme| scheme.chars().count())
}

fn starts_with_ignore_case(text: &[char], prefix: &str) -> bool {
    prefix.chars().count() <= text.len()
        && prefix
            .chars()
            .zip(text)
            .all(|(p, c)| p.eq_ignore_ascii_case(c))
}

fn url_end(row: &[char], start: usize, scheme_len: usize) -> usize {
    let mut end = start + scheme_len;
    while end < row.len() && is_url_char(row[end]) {
        end += 1;
    }
    while end > start + scheme_len {
        let last = row[end - 1];
        let unbalanced = match last {
            ')' => count(&row[start..end], '(') < count(&row[start..end], ')'),
            ']' => count(&row[start..end], '[') < count(&row[start..end], ']'),
            '}' => count(&row[start..end], '{') < count(&row[start..end], '}'),
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            _ => false,
        };
        if !unbalanced {
            break;
        }
        end -= 1;
    }
    end
}

fn is_url_char(c: char) -> bool {
    !c.is_whitespace() && !c.is_control() && !matches!(c, '"' | '\'' | '<' | '>' | '`')
}

fn count(text: &[char], wanted: char) -> usize {
    text.iter().filter(|&&c| c == wanted).count()
}

/// Whether a link from terminal output may be handed to the desktop to open. Output is untrusted,
/// so only well-known schemes are allowed, never arbitrary URL handlers.
pub fn is_openable(uri: &str) -> bool {
    let chars: Vec<char> = uri.chars().collect();
    SCHEMES.iter().any(|scheme| {
        starts_with_ignore_case(&chars, scheme) && chars.len() > scheme.chars().count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn finds_the_url_under_the_column() {
        let row = chars("see https://example.com/a?b=1 for details");
        assert_eq!(
            url_at(&row, 4).as_deref(),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(
            url_at(&row, 20).as_deref(),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(url_at(&row, 2), None);
        assert_eq!(url_at(&row, 30), None);
    }

    #[test]
    fn trailing_punctuation_and_unbalanced_brackets_are_not_part_of_the_url() {
        let row = chars("(docs: https://example.com/x).");
        assert_eq!(url_at(&row, 10).as_deref(), Some("https://example.com/x"));
        let row = chars("https://en.wikipedia.org/wiki/Rust_(language),");
        assert_eq!(
            url_at(&row, 3).as_deref(),
            Some("https://en.wikipedia.org/wiki/Rust_(language)")
        );
    }

    #[test]
    fn urls_stop_at_quotes_and_angle_brackets() {
        let row = chars("href=\"http://a.test/p\"><b>");
        assert_eq!(url_at(&row, 8).as_deref(), Some("http://a.test/p"));
    }

    #[test]
    fn several_urls_on_a_row() {
        let row = chars("file:///tmp/a mailto:me@example.com");
        assert_eq!(url_at(&row, 2).as_deref(), Some("file:///tmp/a"));
        assert_eq!(url_at(&row, 20).as_deref(), Some("mailto:me@example.com"));
    }

    #[test]
    fn a_bare_scheme_is_not_a_url() {
        assert_eq!(url_at(&chars("https:// nothing"), 2), None);
    }

    #[test]
    fn only_known_schemes_may_be_opened() {
        assert!(is_openable("https://example.com"));
        assert!(is_openable("HTTP://EXAMPLE.COM"));
        assert!(is_openable("file:///home/sam/notes.txt"));
        assert!(is_openable("mailto:me@example.com"));
        assert!(!is_openable("javascript:alert(1)"));
        assert!(!is_openable("ms-settings:"));
        assert!(!is_openable("ssh://host"));
        assert!(!is_openable(""));
    }
}

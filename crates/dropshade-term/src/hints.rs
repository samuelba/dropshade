//! Quick select: labels the URLs, paths and hashes on screen so one can be picked by typing.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::links;

const ALPHABET: &[u8; 26] = b"asdfghjklqwertyuiopzxcvbnm";
const MAX_HINTS: usize = ALPHABET.len() * ALPHABET.len();
const TRAILING_PUNCTUATION: &[char] = &['.', ',', ';', ':', '!', '?'];

/// What kind of text a hint covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintKind {
    Url,
    Path,
    Hash,
}

/// A piece of text on one viewport row that can be picked by typing its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub line: usize,
    pub columns: Range<usize>,
    pub text: String,
    pub kind: HintKind,
    pub label: String,
}

/// A key pressed while hints are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintInput {
    Key(char),
    Cancel,
}

/// The hint the user finished typing. `open` is set when a label key was typed with Shift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub text: String,
    pub kind: HintKind,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintOutcome {
    /// The typed keys start some labels; wait for more.
    Pending,
    /// The typed keys match no label.
    Cancelled,
    Picked(Pick),
}

/// The hints shown on screen and the label keys typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintSession {
    hints: Vec<Hint>,
    typed: String,
    open: bool,
}

impl HintSession {
    /// Labels the matches in `rows` (one char per cell, top to bottom); `None` when there are none.
    pub fn new(rows: &[Vec<char>]) -> Option<Self> {
        let hints = find(rows);
        (!hints.is_empty()).then(|| Self {
            hints,
            typed: String::new(),
            open: false,
        })
    }

    pub fn hints(&self) -> &[Hint] {
        &self.hints
    }

    /// The label keys typed so far, in lowercase.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    pub fn key(&mut self, c: char) -> HintOutcome {
        self.open |= c.is_uppercase();
        self.typed.push(c.to_ascii_lowercase());
        if let Some(hint) = self.hints.iter().find(|hint| hint.label == self.typed) {
            return HintOutcome::Picked(Pick {
                text: hint.text.clone(),
                kind: hint.kind,
                open: self.open,
            });
        }
        if self
            .hints
            .iter()
            .any(|hint| hint.label.starts_with(&self.typed))
        {
            HintOutcome::Pending
        } else {
            HintOutcome::Cancelled
        }
    }
}

/// Finds the URLs, paths and hashes in `rows` and labels them, the shortest and easiest labels
/// going to the newest output at the bottom.
pub fn find(rows: &[Vec<char>]) -> Vec<Hint> {
    let mut matches: Vec<(usize, Range<usize>, HintKind)> = rows
        .iter()
        .enumerate()
        .flat_map(|(line, row)| {
            row_matches(row)
                .into_iter()
                .map(move |(columns, kind)| (line, columns, kind))
        })
        .collect();
    let skipped = matches.len().saturating_sub(MAX_HINTS);
    matches.drain(..skipped);
    let labels = labels(matches.len());
    matches
        .into_iter()
        .rev()
        .zip(labels)
        .map(|((line, columns, kind), label)| Hint {
            line,
            text: rows[line][columns.clone()].iter().collect(),
            columns,
            kind,
            label,
        })
        .collect()
}

fn row_matches(row: &[char]) -> Vec<(Range<usize>, HintKind)> {
    let urls: Vec<Range<usize>> = links::url_spans(row).collect();
    let others = tokens(row)
        .filter(|token| !urls.iter().any(|url| overlaps(url, token)))
        .filter_map(|token| {
            let token = trim_trailing_punctuation(row, token);
            let text = &row[token.clone()];
            let kind = if is_path(text) {
                HintKind::Path
            } else if is_hash(text) {
                HintKind::Hash
            } else {
                return None;
            };
            Some((token, kind))
        });
    let mut matches: Vec<_> = urls
        .iter()
        .cloned()
        .map(|url| (url, HintKind::Url))
        .chain(others)
        .collect();
    matches.sort_by_key(|(columns, _)| columns.start);
    matches
}

/// Labels for `count` hints. Either all have one letter or all have two, so none is the start
/// of another.
pub fn labels(count: usize) -> Vec<String> {
    let letter = |i: usize| char::from(ALPHABET[i % ALPHABET.len()]);
    (0..count.min(MAX_HINTS))
        .map(|i| {
            if count <= ALPHABET.len() {
                letter(i).to_string()
            } else {
                [letter(i / ALPHABET.len()), letter(i)].iter().collect()
            }
        })
        .collect()
}

fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || matches!(
            c,
            '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '=' | '|'
        )
}

/// The runs of cells between separators.
fn tokens(row: &[char]) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        while start < row.len() && is_separator(row[start]) {
            start += 1;
        }
        if start >= row.len() {
            return None;
        }
        let end = (start..row.len())
            .find(|&i| is_separator(row[i]))
            .unwrap_or(row.len());
        let token = start..end;
        start = end;
        Some(token)
    })
}

fn trim_trailing_punctuation(row: &[char], token: Range<usize>) -> Range<usize> {
    let mut end = token.end;
    while end > token.start && TRAILING_PUNCTUATION.contains(&row[end - 1]) {
        end -= 1;
    }
    token.start..end
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn is_path(text: &[char]) -> bool {
    text.contains(&'/') && text.iter().any(|c| c.is_alphanumeric())
}

/// A git object id, checksum or similar (7 to 64 hex digits with both digits and letters),
/// or a UUID.
fn is_hash(text: &[char]) -> bool {
    let hex = |c: &char| c.is_ascii_hexdigit();
    let plain = (7..=64).contains(&text.len())
        && text.iter().all(hex)
        && text.iter().any(char::is_ascii_digit)
        && text.iter().any(char::is_ascii_alphabetic);
    plain || is_uuid(text)
}

fn is_uuid(text: &[char]) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let groups: Vec<&[char]> = text.split(|&c| c == '-').collect();
    groups.len() == GROUPS.len()
        && groups
            .iter()
            .zip(GROUPS)
            .all(|(group, len)| group.len() == len && group.iter().all(char::is_ascii_hexdigit))
}

/// The file a picked path names: without a `:line` or `:line:column` suffix, with `~/` expanded
/// and relative paths taken from `cwd`. `None` when that needs a directory that is unknown.
pub fn local_path(text: &str, cwd: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let path = strip_position(text);
    if let Some(rest) = path.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Some(path.to_owned());
    }
    cwd.map(|cwd| cwd.join(path))
}

fn strip_position(text: &str) -> &str {
    let mut path = text;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((head, number))
                if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) =>
            {
                path = head;
            }
            _ => break,
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[&str]) -> Vec<Vec<char>> {
        lines.iter().map(|line| line.chars().collect()).collect()
    }

    fn found(lines: &[&str]) -> Vec<(String, HintKind)> {
        let mut hints = find(&rows(lines));
        hints.sort_by_key(|hint| (hint.line, hint.columns.start));
        hints
            .into_iter()
            .map(|hint| (hint.text, hint.kind))
            .collect()
    }

    #[test]
    fn finds_urls_paths_and_hashes() {
        let hints = found(&["a1b2c3d fix: see https://x.test/a (src/main.rs:12:3)."]);
        assert_eq!(
            hints,
            vec![
                ("a1b2c3d".into(), HintKind::Hash),
                ("https://x.test/a".into(), HintKind::Url),
                ("src/main.rs:12:3".into(), HintKind::Path),
            ]
        );
    }

    #[test]
    fn urls_are_not_also_paths() {
        assert_eq!(
            found(&["https://x.test/a/b"]),
            vec![("https://x.test/a/b".into(), HintKind::Url)]
        );
    }

    #[test]
    fn paths_come_in_many_shapes() {
        let hints = found(&["~/notes ./run ../up /etc/hosts --out=dist/app"]);
        let texts: Vec<_> = hints.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["~/notes", "./run", "../up", "/etc/hosts", "dist/app"]
        );
        assert!(hints.iter().all(|(_, kind)| *kind == HintKind::Path));
    }

    #[test]
    fn words_numbers_and_lone_slashes_are_not_hints() {
        assert_eq!(found(&["deadbeef added 1234567 / // and done"]), vec![]);
    }

    #[test]
    fn uuids_and_long_hashes_are_hashes() {
        let sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        let line = format!("{sha} {uuid}");
        assert_eq!(
            found(&[&line]),
            vec![(sha.into(), HintKind::Hash), (uuid.into(), HintKind::Hash)]
        );
    }

    #[test]
    fn the_newest_output_gets_the_first_labels() {
        let hints = find(&rows(&["/top", "/bottom"]));
        assert_eq!(hints[0].text, "/bottom");
        assert_eq!(hints[0].label, "a");
        assert_eq!(hints[1].label, "s");
    }

    #[test]
    fn labels_never_start_other_labels() {
        assert_eq!(labels(3), vec!["a", "s", "d"]);
        let many = labels(30);
        assert_eq!(many.len(), 30);
        assert!(many.iter().all(|label| label.len() == 2));
        let unique: std::collections::HashSet<_> = many.iter().collect();
        assert_eq!(unique.len(), 30);
        assert_eq!(labels(MAX_HINTS + 5).len(), MAX_HINTS);
    }

    #[test]
    fn typing_a_label_picks_its_hint() {
        let mut session = HintSession::new(&rows(&["/a /b"])).expect("hints");
        assert_eq!(
            session.key('s'),
            HintOutcome::Picked(Pick {
                text: "/a".into(),
                kind: HintKind::Path,
                open: false,
            })
        );
    }

    #[test]
    fn shift_opens_and_wrong_keys_cancel() {
        let mut session = HintSession::new(&rows(&["/a"])).expect("hints");
        assert!(matches!(
            session.key('A'),
            HintOutcome::Picked(Pick { open: true, .. })
        ));
        let mut session = HintSession::new(&rows(&["/a"])).expect("hints");
        assert_eq!(session.key('z'), HintOutcome::Cancelled);
    }

    #[test]
    fn two_letter_labels_wait_for_the_second_key() {
        let line = (0..30)
            .map(|i| format!("/p{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut session = HintSession::new(&rows(&[&line])).expect("hints");
        assert_eq!(session.key('a'), HintOutcome::Pending);
        assert_eq!(session.typed(), "a");
        assert!(matches!(session.key('a'), HintOutcome::Picked(_)));
    }

    #[test]
    fn no_matches_means_no_session() {
        assert_eq!(HintSession::new(&rows(&["nothing here"])), None);
    }

    #[test]
    fn picked_paths_resolve_to_files() {
        let cwd = Path::new("/work");
        let home = Path::new("/home/me");
        let resolve = |text| local_path(text, Some(cwd), Some(home));
        assert_eq!(resolve("src/a.rs:12:3"), Some("/work/src/a.rs".into()));
        assert_eq!(resolve("~/notes:4"), Some("/home/me/notes".into()));
        assert_eq!(resolve("/etc/hosts"), Some("/etc/hosts".into()));
        assert_eq!(resolve("a:b"), Some("/work/a:b".into()));
        assert_eq!(local_path("rel/x", None, None), None);
    }
}

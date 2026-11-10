//! Configuration errors that point at the offending file position.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ConfigError {
    path: PathBuf,
    position: Option<(usize, usize)>,
    message: String,
}

impl ConfigError {
    pub(crate) fn at(
        path: &Path,
        text: &str,
        offset: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.to_path_buf(),
            position: offset.map(|offset| line_and_column(text, offset)),
            message: message.into(),
        }
    }

    pub(crate) fn read(path: &Path, error: &std::io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            position: None,
            message: format!("cannot read the file: {error}"),
        }
    }

    /// One-based line of the error, when known.
    pub fn line(&self) -> Option<usize> {
        self.position.map(|(line, _)| line)
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some((line, column)) => write!(
                f,
                "{}:{line}:{column}: {}",
                self.path.display(),
                self.message
            ),
            None => write!(f, "{}: {}", self.path.display(), self.message),
        }
    }
}

fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rfind('\n')
        .map_or(before.len(), |newline| before.len() - newline - 1)
        + 1;
    (line, column)
}

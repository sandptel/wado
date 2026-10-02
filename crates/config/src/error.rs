//! A config error that points at the file, line and column it came from.

use std::{fmt, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: PathBuf,
    /// 1-based; 0 when there is no position (the file could not be read at all).
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl ConfigError {
    pub fn at(file: PathBuf, src: &str, offset: usize, message: impl Into<String>) -> Self {
        let (line, col) = line_col(src, offset);
        Self {
            file,
            line,
            col,
            message: message.into(),
        }
    }

    pub fn io(file: PathBuf, e: &std::io::Error) -> Self {
        Self {
            file,
            line: 0,
            col: 0,
            message: e.to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file.display(), self.message)
        } else {
            write!(
                f,
                "{}:{}:{}: {}",
                self.file.display(),
                self.line,
                self.col,
                self.message
            )
        }
    }
}

impl std::error::Error for ConfigError {}

/// 1-based line and column (in chars) of a byte offset.
fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset.min(src.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    #[test]
    fn line_col_counts_from_one() {
        assert_eq!(super::line_col("ab\ncd", 0), (1, 1));
        assert_eq!(super::line_col("ab\ncd", 4), (2, 2));
    }
}

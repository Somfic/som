use std::{fmt::Debug, ops::Range, path::PathBuf, sync::Arc};

/// Create raw source with the caller's file path as context for resolving relative paths.
///
/// # Example
/// ```ignore
/// let source = source_raw!(r#"
///     extern "my_lib.so" { fn foo(); }
///     fn main() { foo() }
/// "#);
/// ```
#[macro_export]
macro_rules! source_raw {
    ($source:expr) => {
        $crate::Source::from_raw_at($source, concat!(env!("CARGO_MANIFEST_DIR"), "/", file!()))
    };
}

/// Represents a source of code
#[derive(Clone, PartialEq, Eq)]
pub enum Source {
    /// Raw source code without a file path
    Raw(Arc<str>),
    /// Source code from a file
    File(PathBuf, Arc<str>),
}

impl Source {
    pub fn content(&self) -> &str {
        match self {
            Source::Raw(source) => source,
            Source::File(_, source) => source,
        }
    }

    pub fn identifier(&self) -> &str {
        match self {
            Source::Raw(_) => "<input>",
            Source::File(path, _) => path.to_str().unwrap_or("<unknown>"),
        }
    }

    pub fn from_raw(source: impl Into<Arc<str>>) -> Self {
        Source::Raw(source.into())
    }

    /// Create raw source with a path context for resolving relative paths.
    /// Use the `source_raw!` macro to automatically capture the caller's file path.
    pub fn from_raw_at(source: impl Into<Arc<str>>, context_path: impl Into<PathBuf>) -> Self {
        Source::File(context_path.into(), source.into())
    }

    pub fn from_file(file: impl Into<PathBuf>) -> std::io::Result<Self> {
        let file = file.into();
        let content = std::fs::read_to_string(&file)?;
        Ok(Source::File(file, Arc::from(content)))
    }
}

/// Represents a position in source code (line and column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Represents a location in source code
#[derive(Clone, PartialEq, Eq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
    pub start_offset: usize,
    pub length: usize,
    pub source: Arc<Source>,
}

impl Debug for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Span")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("start_offset", &self.start_offset)
            .field("length", &self.length)
            .finish()
    }
}

impl Span {
    pub fn new(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        start_offset: usize,
        length: usize,
        source: Arc<Source>,
    ) -> Self {
        Self {
            start: Position {
                line: start_line,
                col: start_col,
            },
            end: Position {
                line: end_line,
                col: end_col,
            },
            start_offset,
            length,
            source,
        }
    }

    /// Create a span from a byte range in source code
    pub fn from_range(range: Range<usize>, source: Arc<Source>) -> Self {
        let start_offset = range.start;
        let length = range.end - range.start;

        // Calculate line and column for start position
        let (start_line, start_col) = Self::calculate_position(source.content(), start_offset);
        let (end_line, end_col) = Self::calculate_position(source.content(), range.end);

        Self {
            start: Position {
                line: start_line,
                col: start_col,
            },
            end: Position {
                line: end_line,
                col: end_col,
            },
            start_offset,
            length,
            source,
        }
    }

    /// Calculate line and column (1-indexed) for a byte offset
    fn calculate_position(source: &str, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;

        for (i, ch) in source.chars().enumerate() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        (line, col)
    }

    /// Get the line containing this span
    pub fn get_line(&self) -> Option<Arc<str>> {
        self.source
            .content()
            .lines()
            .nth(self.start.line.saturating_sub(1))
            .map(|s| s.into())
    }

    /// Get the text covered by this span
    pub fn get_text(&self) -> Arc<str> {
        let end = (self.start_offset + self.length).min(self.source.content().len());
        (&self.source.content()[self.start_offset..end]).into()
    }

    /// Create an empty span at the start of a source
    pub fn empty(source: Arc<Source>) -> Self {
        Span {
            start: Position { line: 1, col: 1 },
            end: Position { line: 1, col: 1 },
            start_offset: 0,
            length: 0,
            source,
        }
    }

    /// Merge two spans into one that covers both
    pub fn merge(&self, other: &Span) -> Span {
        let (first, second) = if self.start_offset <= other.start_offset {
            (self, other)
        } else {
            (other, self)
        };

        let end_offset =
            (second.start_offset + second.length).max(first.start_offset + first.length);
        let length = end_offset - first.start_offset;

        Span {
            start: first.start,
            end: second.end,
            start_offset: first.start_offset,
            length,
            source: first.source.clone(),
        }
    }
}

use std::sync::Arc;

/// Represents a span of source code with line, column, and byte offset information.
/// This is used for generating precise error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Starting column number (1-indexed)
    pub start_col: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// Ending column number (1-indexed)
    pub end_col: usize,
    /// Starting byte offset in the source
    pub start_offset: usize,
    /// Length in bytes
    pub length: usize,
    /// Source identifier (file path or source name like "<input>")
    pub source_name: Box<str>,
    /// The full source content (shared across all spans from the same source)
    pub source_content: Arc<str>,
}

impl Span {
    pub fn new(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        start_offset: usize,
        length: usize,
        source_name: impl Into<Box<str>>,
        source_content: Arc<str>,
    ) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
            start_offset,
            length,
            source_name: source_name.into(),
            source_content,
        }
    }

    /// Create a span from a single position (for zero-length spans)
    pub fn at(
        line: usize,
        col: usize,
        offset: usize,
        source_name: impl Into<Box<str>>,
        source_content: Arc<str>,
    ) -> Self {
        Self {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            start_offset: offset,
            length: 0,
            source_name: source_name.into(),
            source_content,
        }
    }

    /// Get the line from the source that contains this span
    pub fn get_line(&self) -> Option<&str> {
        self.source_content
            .lines()
            .nth(self.start_line.saturating_sub(1))
    }

    /// Get the text content of this span
    pub fn get_text(&self) -> &str {
        let end = (self.start_offset + self.length).min(self.source_content.len());
        &self.source_content[self.start_offset..end]
    }
}

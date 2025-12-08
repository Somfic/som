use std::ops::Range;

/// Represents a location in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset from start of source
    pub start: u32,
    /// Byte offset from start of source (exclusive end)
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn from_range(range: Range<usize>) -> Self {
        Self {
            start: range.start as u32,
            end: range.end as u32,
        }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Combine two spans into one that covers both
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Get the text covered by this span from source
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start as usize..self.end as usize]
    }

    /// Calculate line and column numbers (0-indexed)
    pub fn position(&self, source: &str) -> Position {
        let mut line = 0;
        let mut col = 0;
        let mut line_start = 0;

        for (i, ch) in source.chars().enumerate() {
            if i >= self.start as usize {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
                line_start = i + 1;
            } else {
                col += 1;
            }
        }

        Position {
            line,
            col,
            line_start: line_start as u32,
        }
    }

    /// Get the line containing this span
    pub fn get_line<'a>(&self, source: &'a str) -> &'a str {
        let pos = self.position(source);
        let line_start = pos.line_start as usize;

        // Find end of line
        let line_end = source[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .unwrap_or(source.len());

        &source[line_start..line_end]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: usize,
    /// Column number (0-indexed)
    pub col: usize,
    /// Byte offset of the start of this line
    pub line_start: u32,
}

impl Position {
    /// Convert to 1-indexed for display
    pub fn display(&self) -> (usize, usize) {
        (self.line + 1, self.col + 1)
    }
}

/// Format a span with source context for error reporting
pub fn format_error(source: &str, span: Span, message: &str) -> String {
    format_error_with_secondary(source, span, message, &[])
}

/// Format a span with source context and secondary spans for error reporting
pub fn format_error_with_secondary(
    source: &str,
    primary_span: Span,
    message: &str,
    secondary_spans: &[(Span, &str)],
) -> String {
    let mut result = String::new();

    // Primary span
    let pos = primary_span.position(source);
    let (line_num, col_num) = pos.display();
    let line_text = primary_span.get_line(source);

    result.push_str(&format!("error at {}:{}\n", line_num, col_num));
    result.push_str(&format!("  | {}\n", line_text));
    result.push_str("  | ");

    // Add underline for primary span
    let col_in_line = primary_span.start - pos.line_start;
    for _ in 0..col_in_line {
        result.push(' ');
    }

    // Calculate how many carets to show (only on the first line)
    let line_end = pos.line_start + line_text.len() as u32;
    let caret_end = primary_span.end.min(line_end);
    let caret_count = (caret_end - primary_span.start).max(1);

    for _ in 0..caret_count {
        result.push('^');
    }
    result.push('\n');

    // Show secondary spans
    for (secondary_span, label) in secondary_spans {
        let sec_pos = secondary_span.position(source);
        let (sec_line_num, sec_col_num) = sec_pos.display();
        let sec_line_text = secondary_span.get_line(source);

        result.push_str(&format!("  | \n"));
        result.push_str(&format!("  = note: {}\n", label));
        result.push_str(&format!("  | at {}:{}\n", sec_line_num, sec_col_num));
        result.push_str(&format!("  | {}\n", sec_line_text));
        result.push_str("  | ");

        // Add underline for secondary span
        let sec_col_in_line = secondary_span.start - sec_pos.line_start;
        for _ in 0..sec_col_in_line {
            result.push(' ');
        }

        let sec_line_end = sec_pos.line_start + sec_line_text.len() as u32;
        let sec_caret_end = secondary_span.end.min(sec_line_end);
        let sec_caret_count = (sec_caret_end - secondary_span.start).max(1);

        for _ in 0..sec_caret_count {
            result.push('^');
        }
        result.push('\n');
    }

    result.push_str(&format!("  = {}", message));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_position() {
        let source = "line 1\nline 2\nline 3";
        let span = Span::new(7, 13); // "line 2"
        let pos = span.position(source);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.col, 0);
    }

    #[test]
    fn test_span_text() {
        let source = "hello world";
        let span = Span::new(0, 5);
        assert_eq!(span.text(source), "hello");
    }

    #[test]
    fn test_format_error() {
        let source = "fn add(x: i32, y: i32) {\n    x + y\n}";
        let span = Span::new(29, 30); // The 'x' in the body
        let formatted = format_error(source, span, "variable not found");
        assert!(formatted.contains("error at 2:5"));
        assert!(formatted.contains("x + y"));
        assert!(formatted.contains("^"));
    }
}

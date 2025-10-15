use miette::{MietteSpanContents, SourceCode, SourceOffset, SourceSpan, SpanContents};
use std::collections::HashMap;

/// A source code implementation that can handle multiple named sources
pub struct MultiSource {
    sources: Vec<(String, String)>,
}

impl MultiSource {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    pub fn add_source(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.sources.push((name.into(), source.into()));
    }
}

impl SourceCode for MultiSource {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, miette::MietteError> {
        let offset = span.offset();
        let length = span.len();

        // Try each source to find one that contains this span
        // Check sources in order they were added (imported modules first, then current module)
        for (_name, source) in &self.sources {
            // Check if this span fits entirely within this source
            if offset + length <= source.len() {
                // For zero-length spans, just use the offset position
                if length == 0 {
                    let line = source[..offset].matches('\n').count();
                    let line_start = source[..offset].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
                    let column = offset - line_start;

                    // Find the line containing this position for context
                    let line_end = source[offset..]
                        .find('\n')
                        .map(|pos| offset + pos)
                        .unwrap_or(source.len());
                    let line_text = &source[line_start..line_end];

                    return Ok(Box::new(MietteSpanContents::new(
                        line_text.as_bytes(),
                        (line_start, line_text.len()).into(),
                        line,
                        column,
                        line,
                    )));
                }

                // Non-zero length span
                let data = &source[offset..offset + length];
                let line = source[..offset].matches('\n').count();
                let line_start = source[..offset].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
                let column = offset - line_start;

                // Get context lines
                let context_data = if context_lines_before > 0 || context_lines_after > 0 {
                    let mut lines_before = 0;
                    let mut context_start = offset;
                    while lines_before < context_lines_before && context_start > 0 {
                        context_start -= 1;
                        if source.as_bytes()[context_start] == b'\n' {
                            lines_before += 1;
                        }
                    }

                    let mut lines_after = 0;
                    let mut context_end = offset + length;
                    while lines_after < context_lines_after && context_end < source.len() {
                        if source.as_bytes()[context_end] == b'\n' {
                            lines_after += 1;
                        }
                        context_end += 1;
                    }

                    &source[context_start..context_end]
                } else {
                    data
                };

                return Ok(Box::new(MietteSpanContents::new(
                    context_data.as_bytes(),
                    *span,
                    line,
                    column,
                    line,
                )));
            }
        }

        Err(miette::MietteError::OutOfBounds)
    }
}

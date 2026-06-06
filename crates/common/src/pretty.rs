use std::fmt;

use crate::{SourceMap, Span};

/// Pretty-print a value that needs a context (e.g. a type table) to render fully.
/// Wrap with [`Show`] (or a crate-specific `display(...)` helper) to get `Display`.
pub trait Pretty<Ctx: Copy> {
    fn pretty(&self, ctx: Ctx, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// `Display` adapter for any `T: Pretty<C>` paired with its context.
pub struct Show<'a, T: ?Sized, C: Copy> {
    value: &'a T,
    ctx: C,
}

impl<'a, T: ?Sized, C: Copy> Show<'a, T, C> {
    pub fn new(value: &'a T, ctx: C) -> Self {
        Self { value, ctx }
    }
}

impl<'a, T: Pretty<C> + ?Sized, C: Copy> fmt::Display for Show<'a, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.pretty(self.ctx, f)
    }
}

/// Renders lines with an optional left-hand source-line gutter.
///
/// When `sources` is `None`, lines are emitted plainly. When `Some`, every line
/// is prefixed with `"<line> | "` (or `"   | "` when no span is provided) so a
/// reader can map dump output back to source positions.
pub struct LineWriter<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    sources: Option<&'a SourceMap>,
}

impl<'a, 'b> LineWriter<'a, 'b> {
    pub fn new(f: &'a mut fmt::Formatter<'b>, sources: Option<&'a SourceMap>) -> Self {
        Self { f, sources }
    }

    /// Write a line with `indent` leading spaces. If `span` is provided and we
    /// have a `SourceMap`, the line's source line number is shown in the gutter.
    pub fn line(
        &mut self,
        span: Option<Span>,
        indent: usize,
        content: impl fmt::Display,
    ) -> fmt::Result {
        self.write_gutter(span)?;
        for _ in 0..indent {
            self.f.write_str(" ")?;
        }
        writeln!(self.f, "{content}")
    }

    /// Emit a blank separator line (with an empty gutter when active).
    pub fn blank(&mut self) -> fmt::Result {
        if self.sources.is_some() {
            self.f.write_str("    |\n")
        } else {
            self.f.write_str("\n")
        }
    }

    fn write_gutter(&mut self, span: Option<Span>) -> fmt::Result {
        let Some(sources) = self.sources else {
            return Ok(());
        };
        match span {
            Some(span) => {
                let pos = sources.position(span.source, span.start);
                write!(self.f, "{:>3} | ", pos.line)
            }
            None => self.f.write_str("    | "),
        }
    }
}

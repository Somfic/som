use std::{path::PathBuf, sync::Arc};

use crate::{Arena, Id, Position, Span};

/// Holds all loaded sources. Hand out `Id<Source>` to refer to one.
#[derive(Default, Debug)]
pub struct SourceMap {
    sources: Arena<Source>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, source: Source) -> Id<Source> {
        self.sources.alloc(source)
    }

    pub fn source(&self, id: Id<Source>) -> &Source {
        self.sources.get(&id)
    }

    /// Source text covered by `span`.
    pub fn text(&self, span: Span) -> &str {
        let content = self.source(span.source).content();
        let end = (span.end as usize).min(content.len());
        &content[span.start as usize..end]
    }

    /// 1-indexed line/column for a byte offset within `source`.
    pub fn position(&self, source: Id<Source>, offset: u32) -> Position {
        self.source(source).position(offset)
    }

    /// The line containing the start of `span`.
    pub fn line(&self, span: Span) -> &str {
        self.source(span.source).line_at(span.start)
    }
}

/// Create raw source with the caller's file path as context for resolving relative paths.
#[macro_export]
macro_rules! source_raw {
    ($source:expr) => {
        $crate::source::Source::from_raw_at(
            $source,
            concat!(env!("CARGO_MANIFEST_DIR"), "/", file!()),
        )
    };
}

/// A loaded source file. Holds the content and a precomputed line-offset
/// table for fast position lookup.
#[derive(Clone, Debug)]
pub struct Source {
    path: Option<PathBuf>,
    content: Arc<str>,
    /// Byte offset of the start of each line. Always `[0, ...]`.
    line_starts: Arc<[u32]>,
}

impl Source {
    pub fn from_raw(source: impl Into<Arc<str>>) -> Self {
        Self::build(None, source.into())
    }

    pub fn from_raw_at(source: impl Into<Arc<str>>, context_path: impl Into<PathBuf>) -> Self {
        Self::build(Some(context_path.into()), source.into())
    }

    pub fn from_file(file: impl Into<PathBuf>) -> std::io::Result<Self> {
        let file = file.into();
        let content: Arc<str> = std::fs::read_to_string(&file)?.into();
        Ok(Self::build(Some(file), content))
    }

    fn build(path: Option<PathBuf>, content: Arc<str>) -> Self {
        let mut line_starts = Vec::with_capacity(content.len() / 32 + 1);
        line_starts.push(0);
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self {
            path,
            content,
            line_starts: line_starts.into(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn identifier(&self) -> &str {
        self.path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("<input>")
    }

    /// 1-indexed line/column for a byte offset.
    pub fn position(&self, offset: u32) -> Position {
        let line_idx = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line_idx];
        Position::new(line_idx as u32 + 1, offset - line_start + 1)
    }

    /// The line containing byte offset `offset`.
    pub fn line_at(&self, offset: u32) -> &str {
        let line_idx = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let start = self.line_starts[line_idx] as usize;
        let end = self
            .line_starts
            .get(line_idx + 1)
            .map(|&e| e as usize - 1) // strip the trailing '\n'
            .unwrap_or(self.content.len());
        &self.content[start..end]
    }
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && Arc::ptr_eq(&self.content, &other.content)
    }
}

impl Eq for Source {}

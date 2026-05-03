use std::ops::Range;

use crate::{Id, source::Source};

/// A byte range inside a source file. Pure data — line/column rendering is
/// done on demand by `Source` (which holds a line-offset index).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub source: Id<Source>,
}

impl Span {
    pub fn new(source: Id<Source>, start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end, source }
    }

    pub fn from_range(source: Id<Source>, range: Range<usize>) -> Self {
        Self::new(source, range.start as u32, range.end as u32)
    }

    pub fn empty(source: Id<Source>) -> Self {
        Self {
            start: 0,
            end: 0,
            source,
        }
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Smallest span covering both. Both spans must reference the same source.
    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(self.source, other.source);
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            source: self.source,
        }
    }

    /// Zero-width span pointing right after `self`.
    pub fn after(self) -> Span {
        Span {
            start: self.end,
            end: self.end,
            source: self.source,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    /// 1-indexed line.
    pub line: u32,
    /// 1-indexed column (in bytes).
    pub col: u32,
}

impl Position {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

pub trait Spanned {
    fn span(&self) -> Span;
}

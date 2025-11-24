use std::{
    fmt::Debug,
    ops::{Add, Sub},
    sync::Arc,
};

use crate::{lexer::Cursor, Source};

#[derive(Clone)]
pub struct Span {
    pub start: Position,
    pub end: Position,
    pub start_offset: usize,
    pub length: usize,
    pub source: Arc<Source>,
}

impl Span {
    pub fn empty() -> Self {
        Span {
            start: Position { line: 0, col: 0 },
            end: Position { line: 0, col: 0 },
            start_offset: 0,
            length: 0,
            source: Arc::new(Source::from_raw("")),
        }
    }
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

impl From<&Cursor> for Span {
    fn from(cursor: &Cursor) -> Self {
        Span {
            start: cursor.position,
            end: cursor.position,
            start_offset: cursor.byte_offset,
            length: 0,
            source: cursor.source.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub line: usize,
    pub col: usize,
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

    pub fn get_line(&self) -> Option<Arc<str>> {
        self.source
            .content()
            .lines()
            .nth(self.start.line.saturating_sub(1))
            .map(|s| s.into())
    }

    pub fn get_text(&self) -> Arc<str> {
        let end = (self.start_offset + self.length).min(self.source.content().len());
        (&self.source.content()[self.start_offset..end]).into()
    }
}

impl Add for Span {
    type Output = Span;

    fn add(self, rhs: Self) -> Self::Output {
        &self + &rhs
    }
}

impl Add for &Span {
    type Output = Span;

    fn add(self, rhs: Self) -> Self::Output {
        let (start, end) = if self.start_offset <= rhs.start_offset {
            (self, rhs)
        } else {
            (rhs, self)
        };

        Span {
            start: start.start,
            end: end.end,
            start_offset: start.start_offset,
            length: end.start_offset + end.length - start.start_offset,
            source: start.source.clone(),
        }
    }
}

impl Sub for Cursor {
    type Output = Span;

    fn sub(self, rhs: Self) -> Self::Output {
        let (start, end) = if self.byte_offset <= rhs.byte_offset {
            (self, rhs)
        } else {
            (rhs, self)
        };

        Span {
            start: Position {
                line: start.position.line,
                col: start.position.col,
            },
            end: Position {
                line: end.position.line,
                col: end.position.col,
            },
            start_offset: start.byte_offset,
            length: end.byte_offset - start.byte_offset,
            source: start.source.clone(),
        }
    }
}

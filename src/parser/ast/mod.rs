use miette::SourceSpan;
use std::{borrow::Cow, fmt::Display};

mod expression;
mod statement;
mod typing;
pub use self::expression::*;
pub use self::statement::*;
pub use self::typing::*;

pub trait Spannable<'ast>: Sized {
    type Value;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self;

    fn at_multiple(spans: Vec<impl Into<miette::SourceSpan>>, value: Self::Value) -> Self {
        let spans = spans.into_iter().map(|s| s.into()).collect::<Vec<_>>();

        let start = spans
            .iter()
            .min_by_key(|s| s.offset())
            .map(|s| s.offset())
            .unwrap_or(0);

        let end = spans
            .iter()
            .max_by_key(|s| s.offset() + s.len())
            .map(|s| s.offset() + s.len())
            .unwrap_or(0);

        let span = miette::SourceSpan::new(start.into(), end - start);

        Self::at(span, value)
    }
}

pub trait CombineSpan {
    fn combine(spans: Vec<SourceSpan>) -> SourceSpan {
        let start = spans
            .iter()
            .min_by_key(|s| s.offset())
            .map(|s| s.offset())
            .unwrap_or(0);

        let end = spans
            .iter()
            .max_by_key(|s| s.offset() + s.len())
            .map(|s| s.offset() + s.len())
            .unwrap_or(0);

        SourceSpan::new(start.into(), end - start)
    }
}

impl CombineSpan for SourceSpan {}

impl<'ast> Spannable<'ast> for Expression<'ast> {
    type Value = ExpressionValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'ast> Spannable<'ast> for Statement<'ast> {
    type Value = StatementValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'ast> Spannable<'ast> for Type<'ast> {
    type Value = TypeValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self {
            value,
            span,
            original_span: None,
        }
    }
}

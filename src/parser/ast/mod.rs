use miette::SourceSpan;

pub mod typed;
pub mod untyped;

pub trait Spannable<'de>: Sized {
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

impl<'de> Spannable<'de> for untyped::Expression<'de> {
    type Value = untyped::ExpressionValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for untyped::Statement<'de> {
    type Value = untyped::StatementValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for untyped::Type<'de> {
    type Value = untyped::TypeValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

use miette::SourceSpan;

pub trait Spannable: Sized {
    type Value;

    fn at(span: SourceSpan, value: Self::Value) -> Self;

    fn at_multiple(spans: Vec<impl Into<SourceSpan>>, value: Self::Value) -> Self {
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

        let span = SourceSpan::new(start.into(), end - start);

        Self::at(span, value)
    }
}

pub trait CombineSpan {
    fn combine(self, span: SourceSpan) -> SourceSpan;
}

impl CombineSpan for SourceSpan {
    fn combine(self, span: SourceSpan) -> SourceSpan {
        combine_spans(vec![self, span])
    }
}

pub fn combine_spans(spans: Vec<SourceSpan>) -> SourceSpan {
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

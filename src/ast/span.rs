use miette::SourceSpan;
use std::ops::Add;

pub struct SpanWrapper(pub SourceSpan);

impl Add for SpanWrapper {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        let combined = combine_spans(vec![self.0, other.0]);
        SpanWrapper(combined)
    }
}

fn combine_spans(spans: Vec<SourceSpan>) -> SourceSpan {
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

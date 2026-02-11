use som::{Source, Span};
use std::sync::Arc;

#[test]
fn test_span_from_range() {
    let source = Arc::new(Source::from_raw("line 1\nline 2\nline 3"));
    let span = Span::from_range(7..13, source); // "line 2"
    assert_eq!(span.start.line, 2);
    assert_eq!(span.start.col, 1);
    assert_eq!(span.end.line, 2);
    assert_eq!(span.end.col, 7);
}

#[test]
fn test_span_text() {
    let source = Arc::new(Source::from_raw("hello world"));
    let span = Span::from_range(0..5, source);
    assert_eq!(span.get_text().as_ref(), "hello");
}

#[test]
fn test_get_line() {
    let source = Arc::new(Source::from_raw("line 1\nline 2\nline 3"));
    let span = Span::from_range(7..13, source);
    assert_eq!(span.get_line().unwrap().as_ref(), "line 2");
}

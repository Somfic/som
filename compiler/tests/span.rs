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

#[test]
fn test_span_multi_line() {
    let source = Arc::new(Source::from_raw("line1\nline2\nline3"));
    let span = Span::from_range(0..16, source); // entire content
    assert_eq!(span.start.line, 1);
    assert_eq!(span.end.line, 3);
    assert!(span.start.line != span.end.line);
}

#[test]
fn test_span_empty_source() {
    let source = Arc::new(Source::from_raw(""));
    let span = Span::from_range(0..0, source);
    assert_eq!(span.get_text().as_ref(), "");
}

#[test]
fn test_span_merge() {
    let source = Arc::new(Source::from_raw("hello world"));
    let span_a = Span::from_range(0..5, source.clone());  // "hello"
    let span_b = Span::from_range(6..11, source);         // "world"
    let merged = span_a.merge(&span_b);
    assert_eq!(merged.start_offset, 0);
    assert_eq!(merged.length, 11);
    assert_eq!(merged.get_text().as_ref(), "hello world");
}

#[test]
fn test_span_single_char() {
    let source = Arc::new(Source::from_raw("hello"));
    let span = Span::from_range(0..1, source);
    assert_eq!(span.get_text().as_ref(), "h");
    assert_eq!(span.start_offset, 0);
    assert_eq!(span.length, 1);
}

#[test]
fn test_span_last_line() {
    let source = Arc::new(Source::from_raw("first\nsecond\nthird"));
    let span = Span::from_range(13..18, source); // "third"
    assert_eq!(span.start.line, 3);
    assert_eq!(span.get_line().unwrap().as_ref(), "third");
}

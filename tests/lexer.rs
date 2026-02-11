use som::lexer::{lex, TokenKind};
use som::Source;
use std::sync::Arc;

#[test]
fn test_lex_keywords() {
    let source = Arc::new(Source::from_raw("fn let if else"));
    let tokens = lex(source);
    let non_ws: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    assert_eq!(non_ws[0].kind, TokenKind::Fn);
    assert_eq!(non_ws[1].kind, TokenKind::Let);
    assert_eq!(non_ws[2].kind, TokenKind::If);
    assert_eq!(non_ws[3].kind, TokenKind::Else);
}

#[test]
fn test_lex_identifiers() {
    let source = Arc::new(Source::from_raw("foo bar_baz x123"));
    let tokens = lex(source);
    let non_ws: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    assert_eq!(non_ws[0].kind, TokenKind::Ident);
    assert_eq!(non_ws[0].text, "foo");
    assert_eq!(non_ws[1].kind, TokenKind::Ident);
    assert_eq!(non_ws[1].text, "bar_baz");
}

#[test]
fn test_lex_integers() {
    let source = Arc::new(Source::from_raw("123 456"));
    let tokens = lex(source);
    let non_ws: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    assert_eq!(non_ws[0].kind, TokenKind::Int);
    assert_eq!(non_ws[0].text, "123");
    assert_eq!(non_ws[1].kind, TokenKind::Int);
    assert_eq!(non_ws[1].text, "456");
}

#[test]
fn test_lex_operators() {
    let source = Arc::new(Source::from_raw("+ - * / == != < >"));
    let tokens = lex(source);
    let non_ws: Vec<_> = tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    assert_eq!(non_ws[0].kind, TokenKind::Plus);
    assert_eq!(non_ws[1].kind, TokenKind::Minus);
    assert_eq!(non_ws[2].kind, TokenKind::Star);
    assert_eq!(non_ws[3].kind, TokenKind::Slash);
    assert_eq!(non_ws[4].kind, TokenKind::DoubleEquals);
    assert_eq!(non_ws[5].kind, TokenKind::NotEquals);
    assert_eq!(non_ws[6].kind, TokenKind::LessThan);
    assert_eq!(non_ws[7].kind, TokenKind::GreaterThan);
}

#[test]
fn test_spans() {
    let source = Arc::new(Source::from_raw("fn add"));
    let tokens = lex(source.clone());
    assert_eq!(tokens[0].span.start_offset, 0);
    assert_eq!(tokens[0].span.length, 2); // "fn"
    assert_eq!(tokens[2].span.start_offset, 3);
    assert_eq!(tokens[2].span.length, 3); // "add"
}

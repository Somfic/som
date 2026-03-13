mod common;

use common::{filter_whitespace, test_lex};
use som::lexer::TokenKind;

#[test]
fn test_lex_keywords() {
    let tokens = test_lex("fn let if else");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Fn);
    assert_eq!(non_ws[1].kind, TokenKind::Let);
    assert_eq!(non_ws[2].kind, TokenKind::If);
    assert_eq!(non_ws[3].kind, TokenKind::Else);
}

#[test]
fn test_lex_identifiers() {
    let tokens = test_lex("foo bar_baz x123");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Ident);
    assert_eq!(&*non_ws[0].text, "foo");
    assert_eq!(non_ws[1].kind, TokenKind::Ident);
    assert_eq!(&*non_ws[1].text, "bar_baz");
}

#[test]
fn test_lex_integers() {
    let tokens = test_lex("123 456");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Int);
    assert_eq!(&*non_ws[0].text, "123");
    assert_eq!(non_ws[1].kind, TokenKind::Int);
    assert_eq!(&*non_ws[1].text, "456");
}

#[test]
fn test_lex_operators() {
    let tokens = test_lex("+ - * / == != < >");
    let non_ws = filter_whitespace(&tokens);
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
    let tokens = test_lex("fn add");
    assert_eq!(tokens[0].span.start_offset, 0);
    assert_eq!(tokens[0].span.length, 2); // "fn"
    assert_eq!(tokens[2].span.start_offset, 3);
    assert_eq!(tokens[2].span.length, 3); // "add"
}

#[test]
fn test_lex_float_literal() {
    let tokens = test_lex("3.14");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Float);
    assert_eq!(&*non_ws[0].text, "3.14");
}

#[test]
fn test_lex_string_literal() {
    let tokens = test_lex("\"hello\"");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Text);
    assert_eq!(&*non_ws[0].text, "\"hello\"");
}

#[test]
fn test_lex_less_equal() {
    let tokens = test_lex("<=");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::LessThanOrEqual);
}

#[test]
fn test_lex_greater_equal() {
    let tokens = test_lex(">=");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::GreaterThanOrEqual);
}

#[test]
fn test_lex_comments() {
    let tokens = test_lex("x // comment\ny");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Ident);
    assert_eq!(&*non_ws[0].text, "x");
    assert_eq!(non_ws[1].kind, TokenKind::Comment);
    assert_eq!(non_ws[2].kind, TokenKind::Ident);
    assert_eq!(&*non_ws[2].text, "y");
}

#[test]
fn test_lex_empty_input() {
    let tokens = test_lex("");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws.len(), 1);
    assert_eq!(non_ws[0].kind, TokenKind::Eof);
}

#[test]
fn test_lex_keyword_prefix_ident() {
    let tokens = test_lex("letter");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws.len(), 2); // Ident + Eof
    assert_eq!(non_ws[0].kind, TokenKind::Ident);
    assert_eq!(&*non_ws[0].text, "letter");
}

#[test]
fn test_lex_modulo_operator() {
    let tokens = test_lex("%");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Percentage);
}

#[test]
fn test_lex_arrow() {
    let tokens = test_lex("->");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Arrow);
}

#[test]
fn test_lex_double_colon() {
    let tokens = test_lex("::");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::DoubleColon);
}

#[test]
fn test_lex_struct_keyword() {
    let tokens = test_lex("struct impl");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Struct);
    assert_eq!(non_ws[1].kind, TokenKind::Impl);
}

#[test]
fn test_lex_while_keyword() {
    let tokens = test_lex("while for loop");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::While);
    assert_eq!(non_ws[1].kind, TokenKind::For);
    assert_eq!(non_ws[2].kind, TokenKind::Loop);
}

#[test]
fn test_lex_bool_literals() {
    let tokens = test_lex("true false");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::True);
    assert_eq!(non_ws[1].kind, TokenKind::False);
}

#[test]
fn test_lex_ampersand() {
    let tokens = test_lex("&");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::Ampersand);
}

#[test]
fn test_lex_all_delimiters() {
    let tokens = test_lex("( ) { } , ; : .");
    let non_ws = filter_whitespace(&tokens);
    assert_eq!(non_ws[0].kind, TokenKind::OpenParen);
    assert_eq!(non_ws[1].kind, TokenKind::CloseParen);
    assert_eq!(non_ws[2].kind, TokenKind::OpenBrace);
    assert_eq!(non_ws[3].kind, TokenKind::CloseBrace);
    assert_eq!(non_ws[4].kind, TokenKind::Comma);
    assert_eq!(non_ws[5].kind, TokenKind::Semicolon);
    assert_eq!(non_ws[6].kind, TokenKind::Colon);
    assert_eq!(non_ws[7].kind, TokenKind::Dot);
}

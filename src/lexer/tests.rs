
use super::*;

fn lex(input: &str) -> Vec<Token> {
    let source = Source::Raw(input);
    let lexer = Lexer::new(source);
    lexer.filter_map(|r| r.ok()).collect()
}

fn lex_one(input: &str) -> Token {
    lex(input).into_iter().next().unwrap()
}

#[test]
fn test_delimiters() {
    let tokens = lex("( ) { } [ ] ; , . :");
    assert_eq!(tokens.len(), 10);
    assert_eq!(tokens[0].kind, TokenKind::ParenOpen);
    assert_eq!(tokens[1].kind, TokenKind::ParenClose);
    assert_eq!(tokens[2].kind, TokenKind::CurlyOpen);
    assert_eq!(tokens[3].kind, TokenKind::CurlyClose);
    assert_eq!(tokens[4].kind, TokenKind::SquareOpen);
    assert_eq!(tokens[5].kind, TokenKind::SquareClose);
    assert_eq!(tokens[6].kind, TokenKind::Semicolon);
    assert_eq!(tokens[7].kind, TokenKind::Comma);
    assert_eq!(tokens[8].kind, TokenKind::Dot);
    assert_eq!(tokens[9].kind, TokenKind::Colon);
}

#[test]
fn test_operators() {
    let tokens = lex("+ - * / % = ! < > <= >= == != && || ->");
    assert_eq!(tokens[0].kind, TokenKind::Plus);
    assert_eq!(tokens[1].kind, TokenKind::Minus);
    assert_eq!(tokens[2].kind, TokenKind::Star);
    assert_eq!(tokens[3].kind, TokenKind::Slash);
    assert_eq!(tokens[4].kind, TokenKind::Percent);
    assert_eq!(tokens[5].kind, TokenKind::Equal);
    assert_eq!(tokens[6].kind, TokenKind::Not);
    assert_eq!(tokens[7].kind, TokenKind::LessThan);
    assert_eq!(tokens[8].kind, TokenKind::GreaterThan);
    assert_eq!(tokens[9].kind, TokenKind::LessThanOrEqual);
    assert_eq!(tokens[10].kind, TokenKind::GreaterThanOrEqual);
    assert_eq!(tokens[11].kind, TokenKind::Equality);
    assert_eq!(tokens[12].kind, TokenKind::Inequality);
    assert_eq!(tokens[13].kind, TokenKind::And);
    assert_eq!(tokens[14].kind, TokenKind::Or);
    assert_eq!(tokens[15].kind, TokenKind::Arrow);
}

#[test]
fn test_keywords() {
    let tokens = lex("if else while for let fn struct enum trait return extern use mod type");
    assert_eq!(tokens[0].kind, TokenKind::If);
    assert_eq!(tokens[1].kind, TokenKind::Else);
    assert_eq!(tokens[2].kind, TokenKind::While);
    assert_eq!(tokens[3].kind, TokenKind::For);
    assert_eq!(tokens[4].kind, TokenKind::Let);
    assert_eq!(tokens[5].kind, TokenKind::Function);
    assert_eq!(tokens[6].kind, TokenKind::Struct);
    assert_eq!(tokens[7].kind, TokenKind::Enum);
    assert_eq!(tokens[8].kind, TokenKind::Trait);
    assert_eq!(tokens[9].kind, TokenKind::Return);
    assert_eq!(tokens[10].kind, TokenKind::Extern);
    assert_eq!(tokens[11].kind, TokenKind::Use);
    assert_eq!(tokens[12].kind, TokenKind::Mod);
    assert_eq!(tokens[13].kind, TokenKind::Type);
}

#[test]
fn test_type_keywords() {
    let tokens = lex("bool int long dec str char unit");
    assert_eq!(tokens[0].kind, TokenKind::BooleanType);
    assert_eq!(tokens[1].kind, TokenKind::I32Type);
    assert_eq!(tokens[2].kind, TokenKind::I64Type);
    assert_eq!(tokens[3].kind, TokenKind::DecimalType);
    assert_eq!(tokens[4].kind, TokenKind::StringType);
    assert_eq!(tokens[5].kind, TokenKind::CharacterType);
    assert_eq!(tokens[6].kind, TokenKind::UnitType);
}

#[test]
fn test_booleans() {
    let tokens = lex("true false");
    assert_eq!(tokens[0].kind, TokenKind::Boolean);
    assert_eq!(tokens[0].value, TokenValue::Boolean(true));
    assert_eq!(tokens[1].kind, TokenKind::Boolean);
    assert_eq!(tokens[1].value, TokenValue::Boolean(false));
}

#[test]
fn test_identifiers() {
    let tokens = lex("foo bar_baz _private MyType");
    assert_eq!(tokens.len(), 4);
    for token in &tokens {
        assert_eq!(token.kind, TokenKind::Identifier);
    }
}

#[test]
fn test_integers() {
    let token = lex_one("42");
    assert_eq!(token.kind, TokenKind::I32);
    assert_eq!(token.value, TokenValue::I32(42));

    let token = lex_one("0");
    assert_eq!(token.kind, TokenKind::I32);
    assert_eq!(token.value, TokenValue::I32(0));
}

#[test]
fn test_integer_with_suffix() {
    let token = lex_one("42i");
    assert_eq!(token.kind, TokenKind::I32);
    assert_eq!(token.value, TokenValue::I32(42));

    let token = lex_one("42l");
    assert_eq!(token.kind, TokenKind::I64);
    assert_eq!(token.value, TokenValue::I64(42));
}

#[test]
fn test_large_integer_becomes_i64() {
    let token = lex_one("9999999999");
    assert_eq!(token.kind, TokenKind::I64);
    assert_eq!(token.value, TokenValue::I64(9999999999));
}

#[test]
fn test_decimal() {
    let token = lex_one("3.14");
    assert_eq!(token.kind, TokenKind::Decimal);
    assert_eq!(token.value, TokenValue::Decimal(3.14));

    let token = lex_one("0.5");
    assert_eq!(token.kind, TokenKind::Decimal);
    assert_eq!(token.value, TokenValue::Decimal(0.5));
}

#[test]
fn test_string_literal() {
    let token = lex_one(r#""hello world""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("hello world".into()));
}

#[test]
fn test_empty_string() {
    let token = lex_one(r#""""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("".into()));
}

#[test]
fn test_string_with_escape_sequences() {
    let token = lex_one(r#""hello\nworld""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("hello\nworld".into()));

    let token = lex_one(r#""tab\there""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("tab\there".into()));

    let token = lex_one(r#""quote\"inside""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("quote\"inside".into()));

    let token = lex_one(r#""backslash\\test""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("backslash\\test".into()));
}

#[test]
fn test_string_with_hex_escape() {
    let token = lex_one(r#""hex\x41B""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("hexAB".into()));
}

#[test]
fn test_string_with_unicode_escape() {
    let token = lex_one(r#""unicode\u{1F600}!""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("unicode😀!".into()));

    let token = lex_one(r#""\u{41}""#);
    assert_eq!(token.kind, TokenKind::String);
    assert_eq!(token.value, TokenValue::String("A".into()));
}

#[test]
fn test_unterminated_string_error() {
    let token = lex_one(r#""hello"#);
    assert_eq!(token.kind, TokenKind::Error);
}

#[test]
fn test_invalid_escape_sequence_error() {
    let token = lex_one(r#""hello\q""#);
    assert_eq!(token.kind, TokenKind::Error);
}

#[test]
fn test_character_literal() {
    let token = lex_one("'a'");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('a'));

    let token = lex_one("'Z'");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('Z'));
}

#[test]
fn test_character_with_escape() {
    let token = lex_one(r"'\n'");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('\n'));

    let token = lex_one(r"'\''");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('\''));

    let token = lex_one(r"'\x41'");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('A'));
}

#[test]
fn test_character_error_too_long() {
    let token = lex_one("'ab'");
    assert_eq!(token.kind, TokenKind::Error);
}

#[test]
fn test_single_line_comment() {
    let tokens = lex("x // this is a comment\ny");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::Identifier);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
}

#[test]
fn test_multi_line_comment() {
    let tokens = lex("x /* comment */ y");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::Identifier);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
}

#[test]
fn test_multi_line_comment_across_lines() {
    let tokens = lex("x /* comment\nspanning\nmultiple lines */ y");
    assert_eq!(tokens.len(), 2);
}

#[test]
fn test_unterminated_comment_error() {
    let token = lex_one("/* unterminated");
    assert_eq!(token.kind, TokenKind::Error);
}

#[test]
fn test_span_positions() {
    let input = "let x = 42";
    let tokens = lex(input);

    // "let" at position 1:1
    assert_eq!(tokens[0].span.start_line, 1);
    assert_eq!(tokens[0].span.start_col, 1);
    assert_eq!(tokens[0].span.end_line, 1);
    assert_eq!(tokens[0].span.end_col, 3);

    // "x" at position 1:5
    assert_eq!(tokens[1].span.start_line, 1);
    assert_eq!(tokens[1].span.start_col, 5);

    // "=" at position 1:7
    assert_eq!(tokens[2].span.start_line, 1);
    assert_eq!(tokens[2].span.start_col, 7);

    // "42" at position 1:9
    assert_eq!(tokens[3].span.start_line, 1);
    assert_eq!(tokens[3].span.start_col, 9);
}

#[test]
fn test_span_multiline() {
    let input = "let\nx\n=\n42";
    let tokens = lex(input);

    assert_eq!(tokens[0].span.start_line, 1);
    assert_eq!(tokens[1].span.start_line, 2);
    assert_eq!(tokens[2].span.start_line, 3);
    assert_eq!(tokens[3].span.start_line, 4);
}

#[test]
fn test_span_source_name() {
    let input = "foo";
    let tokens = lex(input);
    assert_eq!(tokens[0].span.source_name.as_ref(), "<input>");
}

#[test]
fn test_span_get_text() {
    let input = "hello";
    let token = lex_one(input);
    assert_eq!(token.span.get_text(), "hello");
}

#[test]
fn test_span_get_line() {
    let input = "let x = 42\nlet y = 10";
    let tokens = lex(input);

    // First token should get first line
    assert_eq!(tokens[0].span.get_line(), Some("let x = 42"));

    // Token on second line
    assert_eq!(tokens[4].span.get_line(), Some("let y = 10"));
}

#[test]
fn test_original_text_preserved() {
    let input = "foo";
    let token = lex_one(input);
    assert_eq!(token.original.as_ref(), "foo");

    let input = "  bar  ";
    let token = lex_one(input);
    assert_eq!(token.original.as_ref(), "bar");
}

#[test]
fn test_peek_and_current() {
    let input = "x y z";
    let source = Source::Raw(input);
    let mut lexer = Lexer::new(source);

    // Peek at first token
    let peeked = lexer.peek().unwrap();
    assert_eq!(peeked.kind, TokenKind::Identifier);

    // Current should be None before consuming
    assert!(lexer.current().is_none());

    // Consume first token
    let first = lexer.next().unwrap().unwrap();
    assert_eq!(first.kind, TokenKind::Identifier);

    // Current should now be the first token
    let current = lexer.current().unwrap();
    assert_eq!(current.kind, TokenKind::Identifier);

    // Peek at second token
    let peeked = lexer.peek().unwrap();
    assert_eq!(peeked.kind, TokenKind::Identifier);
}

#[test]
fn test_complex_expression() {
    let tokens = lex("fn add(x: int, y: int) -> int { return x + y; }");

    let expected = vec![
        TokenKind::Function,
        TokenKind::Identifier,
        TokenKind::ParenOpen,
        TokenKind::Identifier,
        TokenKind::Colon,
        TokenKind::I32Type,
        TokenKind::Comma,
        TokenKind::Identifier,
        TokenKind::Colon,
        TokenKind::I32Type,
        TokenKind::ParenClose,
        TokenKind::Arrow,
        TokenKind::I32Type,
        TokenKind::CurlyOpen,
        TokenKind::Return,
        TokenKind::Identifier,
        TokenKind::Plus,
        TokenKind::Identifier,
        TokenKind::Semicolon,
        TokenKind::CurlyClose,
    ];

    assert_eq!(tokens.len(), expected.len());
    for (token, expected_kind) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.kind, expected_kind);
    }
}

#[test]
fn test_special_symbols() {
    let tokens = lex("@ # $ & | ^ ` ~ ?");
    assert_eq!(tokens[0].kind, TokenKind::At);
    assert_eq!(tokens[1].kind, TokenKind::Hash);
    assert_eq!(tokens[2].kind, TokenKind::Dollar);
    assert_eq!(tokens[3].kind, TokenKind::Ampersand);
    assert_eq!(tokens[4].kind, TokenKind::Pipe);
    assert_eq!(tokens[5].kind, TokenKind::Caret);
    assert_eq!(tokens[6].kind, TokenKind::Tick);
    assert_eq!(tokens[7].kind, TokenKind::Tilde);
    assert_eq!(tokens[8].kind, TokenKind::Question);
}

#[test]
fn test_whitespace_handling() {
    let tokens = lex("  x\t\ty  \n  z  ");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].kind, TokenKind::Identifier);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[2].kind, TokenKind::Identifier);
}

#[test]
fn test_unexpected_character_error() {
    let token = lex_one("§");
    assert_eq!(token.kind, TokenKind::Error);
}

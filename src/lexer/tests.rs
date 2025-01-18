use super::*;
use pretty_assertions::assert_eq;

#[test]
fn punctuation() {
    test_tokens_eq(
        Lexer::new(".,@#$~?:-|+*/^% () [] {}"),
        vec![
            (TokenKind::Dot, TokenValue::None),
            (TokenKind::Comma, TokenValue::None),
            (TokenKind::At, TokenValue::None),
            (TokenKind::Hash, TokenValue::None),
            (TokenKind::Dollar, TokenValue::None),
            (TokenKind::Tilde, TokenValue::None),
            (TokenKind::Question, TokenValue::None),
            (TokenKind::Colon, TokenValue::None),
            (TokenKind::Minus, TokenValue::None),
            (TokenKind::Pipe, TokenValue::None),
            (TokenKind::Plus, TokenValue::None),
            (TokenKind::Star, TokenValue::None),
            (TokenKind::Slash, TokenValue::None),
            (TokenKind::Caret, TokenValue::None),
            (TokenKind::Percent, TokenValue::None),
            (TokenKind::ParenOpen, TokenValue::None),
            (TokenKind::ParenClose, TokenValue::None),
            (TokenKind::SquareOpen, TokenValue::None),
            (TokenKind::SquareClose, TokenValue::None),
            (TokenKind::CurlyOpen, TokenValue::None),
            (TokenKind::CurlyClose, TokenValue::None),
        ],
    );
}

#[test]
fn comparison() {
    test_tokens_eq(
        Lexer::new("= != == < > <= >="),
        vec![
            (TokenKind::Equal, TokenValue::None),
            (TokenKind::Inequality, TokenValue::None),
            (TokenKind::Equality, TokenValue::None),
            (TokenKind::LessThan, TokenValue::None),
            (TokenKind::GreaterThan, TokenValue::None),
            (TokenKind::LessThanOrEqual, TokenValue::None),
            (TokenKind::GreaterThanOrEqual, TokenValue::None),
        ],
    );
}

#[test]
fn keywords() {
    test_tokens_eq(
        Lexer::new("if else"),
        vec![
            (TokenKind::If, TokenValue::None),
            (TokenKind::Else, TokenValue::None),
        ],
    );
}

#[test]
fn numbers() {
    test_tokens_eq(
        Lexer::new("1 1.0 0.1"),
        vec![
            (TokenKind::Integer, TokenValue::Integer(1)),
            (TokenKind::Decimal, TokenValue::Decimal(1.0)),
            (TokenKind::Decimal, TokenValue::Decimal(0.1)),
        ],
    );
}

#[test]
fn strings() {
    test_tokens_eq(
        Lexer::new("\"foo\" \"bar\" \"baz\""),
        vec![
            (TokenKind::String, TokenValue::String("foo".into())),
            (TokenKind::String, TokenValue::String("bar".into())),
            (TokenKind::String, TokenValue::String("baz".into())),
        ],
    );
}

#[test]
fn characters() {
    test_tokens_eq(
        Lexer::new("'a' 'b' 'c'"),
        vec![
            (TokenKind::Character, TokenValue::Character('a')),
            (TokenKind::Character, TokenValue::Character('b')),
            (TokenKind::Character, TokenValue::Character('c')),
        ],
    );
}

#[test]
fn booleans() {
    test_tokens_eq(
        Lexer::new("true false"),
        vec![
            (TokenKind::Boolean, TokenValue::Boolean(true)),
            (TokenKind::Boolean, TokenValue::Boolean(false)),
        ],
    );
}

#[test]
fn identifiers() {
    test_tokens_eq(
        Lexer::new("foo bar baz"),
        vec![
            (TokenKind::Identifier, TokenValue::Identifier("foo".into())),
            (TokenKind::Identifier, TokenValue::Identifier("bar".into())),
            (TokenKind::Identifier, TokenValue::Identifier("baz".into())),
        ],
    );
}

#[test]
fn program() {
    let program = "
            fn main(self) ~ number {
                print(\"{self.name} ({self.age}) is purring\");
            };
        ";

    let lexer = Lexer::new(program);
    let expected_tokens = vec![
        (TokenKind::Function, TokenValue::None),
        (TokenKind::Identifier, TokenValue::Identifier("main".into())),
        (TokenKind::ParenOpen, TokenValue::None),
        (TokenKind::Identifier, TokenValue::Identifier("self".into())),
        (TokenKind::ParenClose, TokenValue::None),
        (TokenKind::Tilde, TokenValue::None),
        (
            TokenKind::Identifier,
            TokenValue::Identifier("number".into()),
        ),
        (TokenKind::CurlyOpen, TokenValue::None),
        (
            TokenKind::Identifier,
            TokenValue::Identifier("print".into()),
        ),
        (TokenKind::ParenOpen, TokenValue::None),
        (
            TokenKind::String,
            TokenValue::String("{self.name} ({self.age}) is purring".into()),
        ),
        (TokenKind::ParenClose, TokenValue::None),
        (TokenKind::Semicolon, TokenValue::None),
        (TokenKind::CurlyClose, TokenValue::None),
        (TokenKind::Semicolon, TokenValue::None),
    ];

    test_tokens_eq(lexer, expected_tokens);
}

#[test]
fn peeking() {
    let input = "1 2 3";

    let mut lexer = Lexer::new(input);

    let first = lexer.peek().unwrap().as_ref().unwrap();
    assert_eq!(first.kind, TokenKind::Integer);
    assert_eq!(first.value, TokenValue::Integer(1));

    let first = lexer.next().unwrap().unwrap();
    assert_eq!(first.kind, TokenKind::Integer);
    assert_eq!(first.value, TokenValue::Integer(1));

    let second = lexer.peek().unwrap().as_ref().unwrap();
    assert_eq!(second.kind, TokenKind::Integer);
    assert_eq!(second.value, TokenValue::Integer(2));

    let second = lexer.next().unwrap().unwrap();
    assert_eq!(second.kind, TokenKind::Integer);
    assert_eq!(second.value, TokenValue::Integer(2));

    let third = lexer.peek().unwrap().as_ref().unwrap();
    assert_eq!(third.kind, TokenKind::Integer);
    assert_eq!(third.value, TokenValue::Integer(3));

    let third = lexer.next().unwrap().unwrap();
    assert_eq!(third.kind, TokenKind::Integer);
    assert_eq!(third.value, TokenValue::Integer(3));
}

fn test_tokens_eq(lexer: Lexer<'_>, tokens: Vec<(TokenKind, TokenValue<'_>)>) {
    let actual_tokens = lexer
        .map(Result::unwrap)
        .map(|x| (x.kind, x.value))
        .collect::<Vec<_>>();

    assert_eq!(actual_tokens, tokens);
}

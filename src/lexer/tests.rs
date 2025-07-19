use crate::prelude::*;

fn tokenize(input: &str) -> Vec<Token> {
    let lexer = Lexer::new(input);
    lexer.map(|result| result.unwrap()).collect()
}

#[test]
fn numbers() {
    let input = "123 456.789";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::I32);
    assert_eq!(tokens[0].value, TokenValue::I32(123));
    assert_eq!(tokens[1].kind, TokenKind::Decimal);
    assert_eq!(tokens[1].value, TokenValue::Decimal(456.789));
}

#[test]
fn operators() {
    let input = "+ - * /";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].kind, TokenKind::Plus);
    assert_eq!(tokens[1].kind, TokenKind::Minus);
    assert_eq!(tokens[2].kind, TokenKind::Star);
    assert_eq!(tokens[3].kind, TokenKind::Slash);
}

#[test]
fn identifiers() {
    let input = "foo bar123";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::Identifier);
    assert!(matches!(tokens[0].value, TokenValue::Identifier(_)));
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert!(matches!(tokens[1].value, TokenValue::Identifier(_)));
}

#[test]
fn strings() {
    let input = r#""hello" "world""#;
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::String);
    assert_eq!(tokens[0].value, TokenValue::String(Box::from("hello")));
    assert_eq!(tokens[1].kind, TokenKind::String);
    assert_eq!(tokens[1].value, TokenValue::String(Box::from("world")));
}

#[test]
fn boolean_literals() {
    let input = "true false";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, TokenKind::Boolean);
    assert_eq!(tokens[0].value, TokenValue::Boolean(true));
    assert_eq!(tokens[1].kind, TokenKind::Boolean);
    assert_eq!(tokens[1].value, TokenValue::Boolean(false));
}

#[test]
fn keywords() {
    let input = "if else while for let type fn extern return use mod struct enum trait";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 14);
    assert_eq!(tokens[0].kind, TokenKind::If);
    assert_eq!(tokens[1].kind, TokenKind::Else);
    assert_eq!(tokens[2].kind, TokenKind::While);
    assert_eq!(tokens[3].kind, TokenKind::For);
    assert_eq!(tokens[4].kind, TokenKind::Let);
    assert_eq!(tokens[5].kind, TokenKind::Type);
    assert_eq!(tokens[6].kind, TokenKind::Function);
    assert_eq!(tokens[7].kind, TokenKind::Extern);
    assert_eq!(tokens[8].kind, TokenKind::Return);
    assert_eq!(tokens[9].kind, TokenKind::Use);
    assert_eq!(tokens[10].kind, TokenKind::Mod);
    assert_eq!(tokens[11].kind, TokenKind::Struct);
    assert_eq!(tokens[12].kind, TokenKind::Enum);
    assert_eq!(tokens[13].kind, TokenKind::Trait);
}

#[test]
fn punctuation() {
    let input = ".,;:()[]{}<>!@#$%^&*+=";
    let tokens = tokenize(input);

    assert_eq!(tokens.len(), 22);
    assert_eq!(tokens[0].kind, TokenKind::Dot);
    assert_eq!(tokens[1].kind, TokenKind::Comma);
    assert_eq!(tokens[2].kind, TokenKind::Semicolon);
    assert_eq!(tokens[3].kind, TokenKind::Colon);
    assert_eq!(tokens[4].kind, TokenKind::ParenOpen);
    assert_eq!(tokens[5].kind, TokenKind::ParenClose);
    assert_eq!(tokens[6].kind, TokenKind::SquareOpen);
    assert_eq!(tokens[7].kind, TokenKind::SquareClose);
    assert_eq!(tokens[8].kind, TokenKind::CurlyOpen);
    assert_eq!(tokens[9].kind, TokenKind::CurlyClose);
    assert_eq!(tokens[10].kind, TokenKind::LessThan);
    assert_eq!(tokens[11].kind, TokenKind::GreaterThan);
    assert_eq!(tokens[12].kind, TokenKind::Not);
    assert_eq!(tokens[13].kind, TokenKind::At);
    assert_eq!(tokens[14].kind, TokenKind::Hash);
    assert_eq!(tokens[15].kind, TokenKind::Dollar);
    assert_eq!(tokens[16].kind, TokenKind::Percent);
    assert_eq!(tokens[17].kind, TokenKind::Caret);
    assert_eq!(tokens[18].kind, TokenKind::Ampersand);
    assert_eq!(tokens[19].kind, TokenKind::Star);
    assert_eq!(tokens[20].kind, TokenKind::Plus);
    assert_eq!(tokens[21].kind, TokenKind::Equal);
}

#[test]
fn single_line_comments() {
    let input = "let x = 42; // This is a comment\nlet y = 24;";
    let tokens = tokenize(input);
    // Should have: let, x, =, 42, ;, let, y, =, 24, ;
    assert_eq!(tokens.len(), 10);
    assert_eq!(tokens[0].kind, TokenKind::Let);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[2].kind, TokenKind::Equal);
    assert_eq!(tokens[3].kind, TokenKind::I32);
    assert_eq!(tokens[4].kind, TokenKind::Semicolon);
    assert_eq!(tokens[5].kind, TokenKind::Let);
    assert_eq!(tokens[6].kind, TokenKind::Identifier);
    assert_eq!(tokens[7].kind, TokenKind::Equal);
    assert_eq!(tokens[8].kind, TokenKind::I32);
    assert_eq!(tokens[9].kind, TokenKind::Semicolon);
}

#[test]
fn multi_line_comments() {
    let input = "let x = /* this is a comment */ 42;";
    let tokens = tokenize(input);
    // Should have: let, x, =, 42, ;
    assert_eq!(tokens.len(), 5);
    assert_eq!(tokens[0].kind, TokenKind::Let);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[2].kind, TokenKind::Equal);
    assert_eq!(tokens[3].kind, TokenKind::I32);
    assert_eq!(tokens[4].kind, TokenKind::Semicolon);
}

#[test]
fn multi_line_comments_with_newlines() {
    let input = r#"let x = /*
    this is a multi-line
    comment spanning
    multiple lines
    */ 42;"#;
    let tokens = tokenize(input);
    // Should have: let, x, =, 42, ;
    assert_eq!(tokens.len(), 5);
    assert_eq!(tokens[0].kind, TokenKind::Let);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[2].kind, TokenKind::Equal);
    assert_eq!(tokens[3].kind, TokenKind::I32);
    assert_eq!(tokens[4].kind, TokenKind::Semicolon);
}

#[test]
fn comments_only() {
    let input = "// Just a comment\n/* And another comment */";
    let tokens = tokenize(input);
    // Should have no tokens since only comments
    assert_eq!(tokens.len(), 0);
}

#[test]
#[should_panic] // This should fail with unterminated comment error
fn unterminated_multi_line_comment() {
    let input = "let x = /* unterminated comment";
    let _tokens = tokenize(input);
}

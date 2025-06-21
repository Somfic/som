use som::lexer::{Lexer, TokenKind, TokenValue};

#[test]
fn ascii_character_literal_tokenizes() {
    let mut lexer = Lexer::new("'a'");
    let token = lexer.next().expect("expected token").expect("no error");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('a'));
    assert!(lexer.next().is_none());
}

#[test]
fn multibyte_character_literal_tokenizes() {
    let mut lexer = Lexer::new("'🦀'");
    let token = lexer.next().expect("expected token").expect("no error");
    assert_eq!(token.kind, TokenKind::Character);
    assert_eq!(token.value, TokenValue::Character('🦀'));
    assert!(lexer.next().is_none());
}

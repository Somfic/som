use logos::Logos;

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum Syntax {
    // Keywords
    #[token("fn")]
    FnKw = 0,
    #[token("let")]
    LetKw,
    #[token("if")]
    IfKw,
    #[token("else")]
    ElseKw,

    // Literals and identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    #[regex(r"[0-9]+")]
    Int,

    // Operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("=")]
    Eq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,

    // Delimiters
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,

    // Whitespace and comments
    #[regex(r"[ \t\r\n]+")]
    Whitespace,
    #[regex(r"//[^\n]*")]
    Comment,

    // Special
    Error,
    EndOfFile,

    // CST nodes (not tokens, but part of Syntax enum for rowan)
    Root,
    FuncDec,
    FuncParam,
    TypeAnnotation,
    Block,
    LetStmt,
    ExprStmt,
    BinaryExpr,
    CallExpr,
    ParenExpr,
    VarExpr,
    IntExpr,
}

impl From<Syntax> for rowan::SyntaxKind {
    fn from(syntax: Syntax) -> Self {
        Self(syntax as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}

impl rowan::Language for Lang {
    type Kind = Syntax;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        unsafe { std::mem::transmute::<u16, Syntax>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Lang>;

pub fn lex(input: &str) -> Vec<(Syntax, &str)> {
    let mut lexer = Syntax::lexer(input);
    let mut tokens = Vec::new();

    while let Some(token) = lexer.next() {
        let token = token.unwrap_or(Syntax::Error);
        let text = lexer.slice();
        tokens.push((token, text));
    }

    tokens.push((Syntax::EndOfFile, ""));
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_keywords() {
        let tokens = lex("fn let if else");
        assert_eq!(tokens[0].0, Syntax::FnKw);
        assert_eq!(tokens[2].0, Syntax::LetKw);
        assert_eq!(tokens[4].0, Syntax::IfKw);
        assert_eq!(tokens[6].0, Syntax::ElseKw);
    }

    #[test]
    fn test_lex_identifiers() {
        let tokens = lex("foo bar_baz x123");
        assert_eq!(tokens[0], (Syntax::Ident, "foo"));
        assert_eq!(tokens[2], (Syntax::Ident, "bar_baz"));
        assert_eq!(tokens[4], (Syntax::Ident, "x123"));
    }

    #[test]
    fn test_lex_integers() {
        let tokens = lex("123 456");
        assert_eq!(tokens[0], (Syntax::Int, "123"));
        assert_eq!(tokens[2], (Syntax::Int, "456"));
    }

    #[test]
    fn test_lex_operators() {
        let tokens = lex("+ - * / == != < >");
        assert_eq!(tokens[0].0, Syntax::Plus);
        assert_eq!(tokens[2].0, Syntax::Minus);
        assert_eq!(tokens[4].0, Syntax::Star);
        assert_eq!(tokens[6].0, Syntax::Slash);
        assert_eq!(tokens[8].0, Syntax::EqEq);
        assert_eq!(tokens[10].0, Syntax::NotEq);
        assert_eq!(tokens[12].0, Syntax::Lt);
        assert_eq!(tokens[14].0, Syntax::Gt);
    }

    #[test]
    fn test_lex_function() {
        let tokens = lex("fn add(x: i32, y: i32) -> i32 { x + y }");
        assert_eq!(tokens[0].0, Syntax::FnKw);
        assert_eq!(tokens[2], (Syntax::Ident, "add"));
        assert_eq!(tokens[3].0, Syntax::LeftParen);
        assert_eq!(tokens[4], (Syntax::Ident, "x"));
        assert_eq!(tokens[5].0, Syntax::Colon);
    }
}

use std::{fmt::Display, hash::Hash};

#[derive(Debug, Clone, PartialEq)]
pub enum Lexeme {
    Valid(Token),
    Invalid(Range),
}

impl Display for Lexeme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lexeme::Valid(token) => write!(f, "{:?} at {:?}", token, token.range),
            Lexeme::Invalid(range) => write!(f, "Invalid token at {:?}", range),
        }
    }
}

impl Lexeme {
    pub fn valid(token_type: TokenType, value: TokenValue, start: usize, length: usize) -> Lexeme {
        Lexeme::Valid(Token::new(
            token_type,
            value,
            Range {
                position: start,
                length,
            },
        ))
    }

    pub fn invalid(start: usize, length: usize) -> Lexeme {
        Lexeme::Invalid(Range {
            position: start,
            length,
        })
    }

    pub fn range(&self) -> &Range {
        match self {
            Lexeme::Valid(token) => &token.range,
            Lexeme::Invalid(range) => range,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Range {
    pub position: usize,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub value: TokenValue,
    pub range: Range,
}

impl Token {
    pub fn new(token_type: TokenType, value: TokenValue, range: Range) -> Token {
        Token {
            token_type,
            value,
            range,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    None,
    Boolean(bool),
    Integer(i64),
    Decimal(f64),
    String(String),
    Character(char),
    Identifier(String),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum TokenType {
    /// A token that should be ignored. This is used for whitespace, comments, etc.
    Ignore,
    /// An opening parenthesis; `(`.
    ParenOpen,
    /// A closing parenthesis; `)`.
    ParenClose,
    /// An opening curly brace; `{`.
    CurlyOpen,
    /// A closing curly brace; `}`.
    CurlyClose,
    /// An opening square bracket; `[`.
    SquareOpen,
    /// A closing square bracket; `]`.
    SquareClose,

    /// A comma; `,`.
    Comma,
    /// A dot; `.`.
    Dot,
    /// A colon; `:`.
    Colon,
    /// A semicolon; `;`.
    Semicolon,

    /// A plus sign; `+`;
    Plus,
    /// A minus sign; `-`.
    Minus,
    /// A forward slash; `/`.
    Slash,
    /// An asterisk; `*`.
    Star,

    /// An equals sign; `=`.
    Equal,
    /// A negetion sign; `!`.
    Not,
    /// A less-than sign; `<`.
    LessThan,
    /// A greater-than sign; `>`.
    GreaterThan,
    /// A less-than-or-equal sign; `<=`.
    LessThanOrEqual,
    /// A greater-than-or-equal sign; `>=`.
    GreaterThanOrEqual,
    /// An equality sign; `==`.
    Equality,
    /// An inequality sign; `!=`.
    Inequality,

    /// An if keyword; `if`.
    If,
    /// An else keyword; `else`.
    Else,

    /// A while keyword; `while`.
    While,
    /// A for keyword; `for`.
    For,

    /// A let keyword; `let`.
    Let,

    /// A function keyword; `fn`.
    Function,
    /// A return keyword; `return`.
    Return,

    /// A boolean; `true`, `false`.
    Boolean,
    /// A number; `42`, `12`, `-7`.
    Integer,
    /// A decimal; `3.14`, `2.718`, `-1.0`.
    Decimal,
    /// A string; `"foo"`, `"bar"`, `"baz"`.
    String,
    /// A character; `'a'`, `'b'`, `'c'`.
    Character,

    /// An identifying name; `foo`, `bar`, `baz`.
    Identifier,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Ignore => write!(f, ""),
            TokenType::ParenOpen => write!(f, "("),
            TokenType::ParenClose => write!(f, ")"),
            TokenType::CurlyOpen => write!(f, "{{"),
            TokenType::CurlyClose => write!(f, "}}"),
            TokenType::SquareOpen => write!(f, "["),
            TokenType::SquareClose => write!(f, "]"),
            TokenType::Comma => write!(f, ","),
            TokenType::Dot => write!(f, "."),
            TokenType::Colon => write!(f, ":"),
            TokenType::Semicolon => write!(f, ";"),
            TokenType::Plus => write!(f, "+"),
            TokenType::Minus => write!(f, "-"),
            TokenType::Slash => write!(f, "/"),
            TokenType::Star => write!(f, "*"),
            TokenType::Equal => write!(f, "="),
            TokenType::Not => write!(f, "!"),
            TokenType::LessThan => write!(f, "<"),
            TokenType::GreaterThan => write!(f, ">"),
            TokenType::LessThanOrEqual => write!(f, "<="),
            TokenType::GreaterThanOrEqual => write!(f, ">="),
            TokenType::Equality => write!(f, "=="),
            TokenType::Inequality => write!(f, "!="),
            TokenType::If => write!(f, "if"),
            TokenType::Else => write!(f, "else"),
            TokenType::While => write!(f, "while"),
            TokenType::For => write!(f, "for"),
            TokenType::Let => write!(f, "let"),
            TokenType::Function => write!(f, "fn"),
            TokenType::Return => write!(f, "return"),
            TokenType::Boolean => write!(f, "boolean"),
            TokenType::Integer => write!(f, "integer"),
            TokenType::Decimal => write!(f, "decimal"),
            TokenType::String => write!(f, "string"),
            TokenType::Character => write!(f, "character"),
            TokenType::Identifier => write!(f, "identifier"),
        }
    }
}

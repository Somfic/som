use std::{fmt::Display, hash::Hash};

use crate::diagnostic::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Token<'a> {
    pub token_type: TokenType,
    pub value: TokenValue,
    /// The range of the token in the source code.
    pub range: Range<'a>,
}

impl<'a> Token<'a> {
    pub fn new(
        file_id: impl Into<&'a str>,
        token_type: TokenType,
        value: TokenValue,
        start: usize,
        length: usize,
    ) -> Token<'a> {
        Token {
            token_type,
            value,
            range: Range {
                file_id: file_id.into(),
                position: start,
                length,
            },
        }
    }
}

impl<'a> Display for Token<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}..{})",
            self.token_type,
            self.range.position,
            self.range.position + self.range.length
        )
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

    /// The opening of an indentation level.
    IndentationOpen,
    /// The closing of an indentation level.
    IndentationClose,

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

    /// A var keyword; `var`.
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

    /// A struct keyword; `struct`.
    Struct,
    /// A enum keyword; `enum`.
    Enum,

    /// The end of the file.
    EndOfFile,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Ignore => write!(f, ""),
            TokenType::IndentationOpen => write!(f, "opening indentation level"),
            TokenType::IndentationClose => write!(f, "closing indentation level"),
            TokenType::ParenOpen => write!(f, "`(`"),
            TokenType::ParenClose => write!(f, "`)`"),
            TokenType::CurlyOpen => write!(f, "`{{`"),
            TokenType::CurlyClose => write!(f, "`}}`"),
            TokenType::SquareOpen => write!(f, "`[`"),
            TokenType::SquareClose => write!(f, "`]`"),
            TokenType::Comma => write!(f, "`,`"),
            TokenType::Dot => write!(f, "`.`"),
            TokenType::Colon => write!(f, "`:`"),
            TokenType::Semicolon => write!(f, "`;`"),
            TokenType::Plus => write!(f, "`+`"),
            TokenType::Minus => write!(f, "`-`"),
            TokenType::Slash => write!(f, "`/`"),
            TokenType::Star => write!(f, "`*`"),
            TokenType::Equal => write!(f, "`=`"),
            TokenType::Not => write!(f, "`!`"),
            TokenType::LessThan => write!(f, "`<`"),
            TokenType::GreaterThan => write!(f, "`>`"),
            TokenType::LessThanOrEqual => write!(f, "`<=`"),
            TokenType::GreaterThanOrEqual => write!(f, "`>=`"),
            TokenType::Equality => write!(f, "`==`"),
            TokenType::Inequality => write!(f, "`!=`"),
            TokenType::If => write!(f, "`if`"),
            TokenType::Else => write!(f, "`else`"),
            TokenType::While => write!(f, "`while`"),
            TokenType::For => write!(f, "`for`"),
            TokenType::Let => write!(f, "`let`"),
            TokenType::Function => write!(f, "`fn`"),
            TokenType::Return => write!(f, "`return`"),
            TokenType::Boolean => write!(f, "a boolean value"),
            TokenType::Integer => write!(f, "an integer value"),
            TokenType::Decimal => write!(f, "a decimal value"),
            TokenType::String => write!(f, "a string value"),
            TokenType::Character => write!(f, "a character value"),
            TokenType::Identifier => write!(f, "an identifier"),
            TokenType::Struct => write!(f, "`struct`"),
            TokenType::Enum => write!(f, "`enum`"),
            TokenType::EndOfFile => write!(f, "end of file"),
        }
    }
}

use cranelift::prelude::{types, InstBuilder, Value, Variable};

use crate::{
    lexer::{Identifier, Span},
    Emit, FunctionContext, ModuleContext, Parse, ParserError, Result,
};
use std::fmt::{Debug, Display};

#[derive(Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: TokenValue,
    pub original: Box<str>,
    pub span: Span,
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            TokenValue::None => write!(f, "{}", self.kind),
            value => write!(f, "`{}` ({})", value, self.kind),
        }
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.value == other.value
    }
}

impl Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value {
            TokenValue::None => write!(f, "{:?}", self.kind),
            _ => write!(f, "{:?}", self.value),
        }
    }
}

impl From<Token> for Span {
    fn from(token: Token) -> Self {
        token.span
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    None,
    Error(Box<str>),
    Boolean(bool),
    I32(i32),
    I64(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
    Identifier(Identifier),
}

impl Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenValue::None => write!(f, "nothing"),
            TokenValue::Error(msg) => write!(f, "{msg}"),
            TokenValue::Boolean(value) => write!(f, "{value}"),
            TokenValue::I32(value) => write!(f, "{value}"),
            TokenValue::I64(value) => write!(f, "{value}"),
            TokenValue::Decimal(value) => write!(f, "{value}"),
            TokenValue::String(value) => write!(f, "{value}"),
            TokenValue::Character(value) => write!(f, "{value}"),
            TokenValue::Identifier(value) => write!(f, "{value}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum TokenKind {
    /// A lexer error token containing an error message.
    Error,

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
    /// A double colon; `::`.
    DoubleColon,
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
    /// A percent sign; `%`.
    Percent,

    /// An equals sign; `=`.
    Equal,
    /// A negation sign; `!`.
    Bang,
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

    /// An and sign; `&&`.
    And,
    /// An or sign; `||`.
    Or,

    /// An at sign; `@`.
    At,
    /// A hash sign; `#`.
    Hash,
    /// A dollar sign; `$`.
    Dollar,
    /// A tilde sign; `~`.
    Tilde,
    /// An arrow; `->`.
    Arrow,
    /// A question mark; `?`.
    Question,
    /// A pipe; `|`.
    Pipe,
    /// An ampersand sign; `&`.
    Ampersand,
    /// A caret; `^`.
    Caret,
    /// A tick; ```.
    Tick,

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
    /// A type keyword; `type`.
    Type,

    /// A function keyword; `fn`.
    Function,
    /// An extern keyword; `extern`.
    Extern,
    /// An as keyword; `as`.
    As,
    /// A return keyword; `return`.
    Return,

    /// An use keyword; `use`.
    Use,
    /// A mod keyword; `mod`.
    Mod,
    /// A public keyword; `pub`.
    Pub,

    /// A boolean; `true`, `false`.
    Boolean,
    /// A 32 bit number; `42`, `12`, `-7`.
    I32,
    /// A 64 bit number; `42`, `12`, `-7`.
    I64,
    /// A decimal; `3.14`, `2.718`, `-1.0`.
    F64,
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
    /// A trait keyword; `trait`.
    Trait,

    /// The unit type; `unit`.
    UnitType,
    /// The boolean type; `bool`.
    BooleanType,
    /// The byte type; `byte`.
    ByteType,
    /// The integer type; `i32`.
    I32Type,
    /// The integer type; `i64`.
    I64Type,
    /// The decimal type; `f64`.
    F64Type,
    /// The string type; `str`.
    StringType,
    /// The character type; `char`.
    CharacterType,
    /// The end of the file; `EOF`.
    Eof,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Error => write!(f, "an error"),
            TokenKind::ParenOpen => write!(f, "`(`"),
            TokenKind::ParenClose => write!(f, "`)`"),
            TokenKind::CurlyOpen => write!(f, "`{{`"),
            TokenKind::CurlyClose => write!(f, "`}}`"),
            TokenKind::SquareOpen => write!(f, "`[`"),
            TokenKind::SquareClose => write!(f, "`]`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Colon => write!(f, "`:`"),
            TokenKind::DoubleColon => write!(f, "`::`"),
            TokenKind::Semicolon => write!(f, "`;`"),
            TokenKind::Plus => write!(f, "`+`"),
            TokenKind::Minus => write!(f, "`-`"),
            TokenKind::Slash => write!(f, "`/`"),
            TokenKind::Star => write!(f, "`*`"),
            TokenKind::Equal => write!(f, "`=`"),
            TokenKind::Bang => write!(f, "`!`"),
            TokenKind::LessThan => write!(f, "`<`"),
            TokenKind::GreaterThan => write!(f, "`>`"),
            TokenKind::LessThanOrEqual => write!(f, "`<=`"),
            TokenKind::GreaterThanOrEqual => write!(f, "`>=`"),
            TokenKind::Equality => write!(f, "`==`"),
            TokenKind::Inequality => write!(f, "`!=`"),
            TokenKind::If => write!(f, "`if`"),
            TokenKind::Else => write!(f, "`else`"),
            TokenKind::While => write!(f, "`while`"),
            TokenKind::For => write!(f, "`for`"),
            TokenKind::Let => write!(f, "`let`"),
            TokenKind::Type => write!(f, "`type`"),
            TokenKind::Function => write!(f, "`fn`"),
            TokenKind::Extern => write!(f, "`extern`"),
            TokenKind::As => write!(f, "`as`"),
            TokenKind::Return => write!(f, "`return`"),
            TokenKind::Use => write!(f, "`use`"),
            TokenKind::Pub => write!(f, "`pub`"),
            TokenKind::Mod => write!(f, "`mod`"),
            TokenKind::Boolean => write!(f, "a boolean"),
            TokenKind::I32 => write!(f, "an integer"),
            TokenKind::I64 => write!(f, "a long"),
            TokenKind::F64 => write!(f, "a decimal"),
            TokenKind::String => write!(f, "a string"),
            TokenKind::Character => write!(f, "a character"),
            TokenKind::Identifier => write!(f, "an identifier"),
            TokenKind::Struct => write!(f, "`struct`"),
            TokenKind::Enum => write!(f, "`enum`"),
            TokenKind::Percent => write!(f, "`%`"),
            TokenKind::At => write!(f, "`@`"),
            TokenKind::Hash => write!(f, "`#`"),
            TokenKind::Dollar => write!(f, "`$`"),
            TokenKind::Tilde => write!(f, "`~`"),
            TokenKind::Arrow => write!(f, "`->`"),
            TokenKind::Question => write!(f, "`?`"),
            TokenKind::Pipe => write!(f, "`|`"),
            TokenKind::Caret => write!(f, "`^`"),
            TokenKind::Tick => write!(f, "`"),
            TokenKind::And => write!(f, "`&&`"),
            TokenKind::Or => write!(f, "`||`"),
            TokenKind::Trait => write!(f, "`trait`"),
            TokenKind::UnitType => write!(f, "a unit type"),
            TokenKind::BooleanType => write!(f, "a boolean type"),
            TokenKind::ByteType => write!(f, "a byte type"),
            TokenKind::I32Type => write!(f, "an integer type"),
            TokenKind::I64Type => write!(f, "a long type"),
            TokenKind::F64Type => write!(f, "a decimal type"),
            TokenKind::StringType => write!(f, "a string type"),
            TokenKind::CharacterType => write!(f, "a character type"),
            TokenKind::Eof => write!(f, "the end of the file"),
            TokenKind::Ampersand => write!(f, "`&`"),
        }
    }
}

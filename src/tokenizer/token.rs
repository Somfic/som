use span_derive::Span;
use std::fmt::Display;

use crate::ast::Identifier;

#[derive(Debug, Clone, Span, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub value: TokenValue,
    pub original: Box<str>,
    pub span: miette::SourceSpan,
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            TokenValue::None => write!(f, "{}", self.kind),
            value => write!(f, "{}: {}", self.kind, value),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    None,
    Boolean(bool),
    Integer(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
    Identifier(Identifier),
}

impl Display for TokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenValue::None => write!(f, ""),
            TokenValue::Boolean(value) => write!(f, "{value}"),
            TokenValue::Integer(value) => write!(f, "{value}"),
            TokenValue::Decimal(value) => write!(f, "{value}"),
            TokenValue::String(value) => write!(f, "{value}"),
            TokenValue::Character(value) => write!(f, "{value}"),
            TokenValue::Identifier(value) => write!(f, "{value}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum TokenKind {
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
    /// A percent sign; `%`.
    Percent,

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

    /// A var keyword; `var`.
    Let,
    /// A type keyword; `type`.
    Type,

    /// A function keyword; `fn`.
    Function,
    /// An intrinsic keyword; `intrinsic`.
    Intrinsic,
    /// A return keyword; `return`.
    Return,

    /// An use keyword; `use`.
    Use,
    /// A mod keyword; `mod`.
    Mod,

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
    /// A trait keyword; `trait`.
    Trait,

    /// The unit type; `unit`.
    UnitType,
    /// The boolean type; `bool`.
    BooleanType,
    /// The integer type; `int`.
    IntegerType,
    /// The decimal type; `dec`.
    DecimalType,
    /// The string type; `str`.
    StringType,
    /// The character type; `char`.
    CharacterType,
    /// The end of the file; `EOF`.
    EOF,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Ignore => write!(f, ""),
            TokenKind::IndentationOpen => write!(f, "opening indentation level"),
            TokenKind::IndentationClose => write!(f, "closing indentation level"),
            TokenKind::ParenOpen => write!(f, "`(`"),
            TokenKind::ParenClose => write!(f, "`)`"),
            TokenKind::CurlyOpen => write!(f, "`{{`"),
            TokenKind::CurlyClose => write!(f, "`}}`"),
            TokenKind::SquareOpen => write!(f, "`[`"),
            TokenKind::SquareClose => write!(f, "`]`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Colon => write!(f, "`:`"),
            TokenKind::Semicolon => write!(f, "`;`"),
            TokenKind::Plus => write!(f, "`+`"),
            TokenKind::Minus => write!(f, "`-`"),
            TokenKind::Slash => write!(f, "`/`"),
            TokenKind::Star => write!(f, "`*`"),
            TokenKind::Equal => write!(f, "`=`"),
            TokenKind::Not => write!(f, "`!`"),
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
            TokenKind::Intrinsic => write!(f, "`intrinsic`"),
            TokenKind::Return => write!(f, "`return`"),
            TokenKind::Use => write!(f, "`use`"),
            TokenKind::Mod => write!(f, "`mod`"),
            TokenKind::Boolean => write!(f, "a boolean value"),
            TokenKind::Integer => write!(f, "an integer value"),
            TokenKind::Decimal => write!(f, "a decimal value"),
            TokenKind::String => write!(f, "a string value"),
            TokenKind::Character => write!(f, "a character value"),
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
            TokenKind::IntegerType => write!(f, "an integer type"),
            TokenKind::DecimalType => write!(f, "a decimal type"),
            TokenKind::StringType => write!(f, "a string type"),
            TokenKind::CharacterType => write!(f, "a character type"),

            TokenKind::EOF => write!(f, "the end of the file"),
        }
    }
}

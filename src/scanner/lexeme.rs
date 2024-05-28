#[derive(Debug, PartialEq, Eq)]
pub enum Lexeme {
    Valid(Token, Range),
    Invalid(Range),
}

impl Lexeme {
    pub fn valid(token: Token, start: usize, length: usize) -> Lexeme {
        Lexeme::Valid(
            token,
            Range {
                position: start,
                length,
            },
        )
    }

    pub fn invalid(start: usize, length: usize) -> Lexeme {
        Lexeme::Invalid(Range {
            position: start,
            length,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Range {
    pub position: usize,
    pub length: usize,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Token {
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
    Equiality,
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
    Boolean(bool),
    /// A number; `42`, `12`, `-7`.
    Number(i32),
    /// A string; `"foo"`, `"bar"`, `"baz"`.
    String(String),
    /// A character; `'a'`, `'b'`, `'c'`.
    Character(char),

    /// An identifying name; `foo`, `bar`, `baz`.
    Identifier(String),
}

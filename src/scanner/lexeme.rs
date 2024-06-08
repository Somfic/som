use std::{fmt::Display, hash::Hash};

#[derive(Debug, Clone, PartialEq)]
pub enum Lexeme {
    Valid(Token, Range),
    Invalid(Range),
}

impl Display for Lexeme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lexeme::Valid(token, range) => write!(f, "{:?} at {:?}", token, range),
            Lexeme::Invalid(range) => write!(f, "Invalid token at {:?}", range),
        }
    }
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
            position: start - 1,
            length,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub position: usize,
    pub length: usize,
}

#[derive(Debug, Clone)]
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
    Integer(i64),
    /// A decimal; `3.14`, `2.718`, `-1.0`.
    Decimal(f64),
    /// A string; `"foo"`, `"bar"`, `"baz"`.
    String(String),
    /// A character; `'a'`, `'b'`, `'c'`.
    Character(char),

    /// An identifying name; `foo`, `bar`, `baz`.
    Identifier(String),
}

impl Hash for Token {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash based on enum variant, not value
        std::mem::discriminant(self).hash(state);
    }
}

impl Eq for Token {}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }
}

#[cfg(test)]
mod test {
    use super::Token;

    #[test]
    fn partial_eq_impl() {
        let a = Token::Integer(42);
        let b = Token::Integer(41);
        let c = Token::String("42".to_owned());

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn hash_impl() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        Token::Integer(42).hash(&mut hasher);
        let a = hasher.finish();

        let mut hasher = DefaultHasher::new();
        Token::Integer(41).hash(&mut hasher);
        let b = hasher.finish();

        assert_eq!(a, b);
    }
}

mod highlight;

pub use highlight::highlight;
use logos::Logos;
use som_base::message::{Part, code};
use som_base::{Source, Span};
use std::sync::Arc;

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Keywords
    #[token("fn")]
    Fn,
    #[token("prop")]
    Prop,
    /// `self`: the value a method is called on.
    #[token("self")]
    SelfValue,
    /// `Self`: in an impl block, the type it's for; in a trait, the type implementing it.
    #[token("Self")]
    SelfType,
    #[token("extern")]
    Extern,
    #[token("struct")]
    Struct,
    #[token("impl")]
    Impl,
    #[token("enum")]
    Enum,
    #[token("trait")]
    Trait,
    #[token("type")]
    Type,
    #[token("match")]
    Match,
    #[token("let")]
    Let,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("mut")]
    Mut,
    #[token("out")]
    Out,
    #[token("use")]
    Use,
    #[token("and")]
    And,
    #[token("as")]
    As,
    #[token("or")]
    Or,

    // Built-in types
    #[token("i8")]
    I8,
    #[token("i16")]
    I16,
    #[token("i32")]
    I32,
    #[token("i64")]
    I64,
    #[token("i128")]
    I128,
    #[token("isize")]
    ISize,
    #[token("u8")]
    U8,
    #[token("u16")]
    U16,
    #[token("u32")]
    U32,
    #[token("u64")]
    U64,
    #[token("u128")]
    U128,
    #[token("usize")]
    USize,
    #[token("f32")]
    F32,
    #[token("f64")]
    F64,
    #[token("bool")]
    Bool,
    #[token("char")]
    Char,
    #[token("str")]
    Str,
    #[token("cstr")]
    CStr,

    // Literals and identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    #[regex(r"[0-9]+")]
    Int,
    #[regex(r"[0-9]*\.[0-9]+")]
    Float,
    #[regex(r#""([^"\\]|\\.)*""#)]
    Text,
    #[regex(r"true")]
    True,
    #[regex(r"false")]
    False,
    #[token("none")]
    None,

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
    Equals,
    #[token("==")]
    DoubleEquals,
    #[token("!=")]
    NotEquals,
    #[token("!")]
    Bang,
    #[token("<")]
    LessThan,
    #[token(">")]
    GreaterThan,
    #[token("<=")]
    LessThanOrEqual,
    #[token(">=")]
    GreaterThanOrEqual,
    #[token("%")]
    Percentage,

    // Delimiters
    #[token("(")]
    OpenParen,
    #[token(")")]
    CloseParen,
    #[token("{")]
    OpenBrace,
    #[token("}")]
    CloseBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token("::")]
    DoubleColon,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("&")]
    Ampersand,
    #[token("?")]
    Question,
    #[token("[")]
    OpenBracket,
    #[token("]")]
    CloseBracket,
    #[token(".")]
    Dot,
    #[token("..")]
    DotDot,
    #[token("...")]
    Ellipsis,

    #[token("'")]
    SingleQuote,
    #[token("\"")]
    DoubleQuote,

    // Whitespace and comments (skipped during parsing)
    #[regex(r"[ \t\r\n]+")]
    Whitespace,
    #[regex(r"//[^\n]*", allow_greedy = true)]
    Comment,

    // Loops
    #[token("loop")]
    Loop,
    #[token("while")]
    While,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("return")]
    Return,
    #[token("for")]
    For,
    #[token("in")]
    In,

    // Special
    Error,
    Eof,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Prop => write!(f, "prop"),
            TokenKind::SelfValue => write!(f, "self"),
            TokenKind::SelfType => write!(f, "Self"),
            TokenKind::Extern => write!(f, "extern"),
            TokenKind::Struct => write!(f, "struct"),
            TokenKind::Impl => write!(f, "impl"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Trait => write!(f, "trait"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Let => write!(f, "let"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Mut => write!(f, "mut"),
            TokenKind::Out => write!(f, "out"),
            TokenKind::Use => write!(f, "use"),
            TokenKind::And => write!(f, "and"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Loop => write!(f, "loop"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::I8 => write!(f, "i8"),
            TokenKind::I16 => write!(f, "i16"),
            TokenKind::I32 => write!(f, "i32"),
            TokenKind::I64 => write!(f, "i64"),
            TokenKind::I128 => write!(f, "i128"),
            TokenKind::ISize => write!(f, "isize"),
            TokenKind::U8 => write!(f, "u8"),
            TokenKind::U16 => write!(f, "u16"),
            TokenKind::U32 => write!(f, "u32"),
            TokenKind::U64 => write!(f, "u64"),
            TokenKind::U128 => write!(f, "u128"),
            TokenKind::USize => write!(f, "usize"),
            TokenKind::F32 => write!(f, "f32"),
            TokenKind::F64 => write!(f, "f64"),
            TokenKind::Bool => write!(f, "bool"),
            TokenKind::Char => write!(f, "char"),
            TokenKind::Str => write!(f, "str"),
            TokenKind::CStr => write!(f, "cstr"),
            TokenKind::Ident => write!(f, "identifier"),
            TokenKind::Int => write!(f, "integer"),
            TokenKind::Float => write!(f, "float"),
            TokenKind::Text => write!(f, "string"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::None => write!(f, "none"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Equals => write!(f, "="),
            TokenKind::DoubleEquals => write!(f, "=="),
            TokenKind::NotEquals => write!(f, "!="),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::LessThan => write!(f, "<"),
            TokenKind::GreaterThan => write!(f, ">"),
            TokenKind::LessThanOrEqual => write!(f, "<="),
            TokenKind::GreaterThanOrEqual => write!(f, ">="),
            TokenKind::OpenParen => write!(f, "("),
            TokenKind::CloseParen => write!(f, ")"),
            TokenKind::OpenBrace => write!(f, "{{"),
            TokenKind::CloseBrace => write!(f, "}}"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::DoubleColon => write!(f, "::"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::FatArrow => write!(f, "=>"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::OpenBracket => write!(f, "["),
            TokenKind::CloseBracket => write!(f, "]"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::Ellipsis => write!(f, "..."),
            TokenKind::SingleQuote => write!(f, "'"),
            TokenKind::DoubleQuote => write!(f, "\""),
            TokenKind::Whitespace => write!(f, "whitespace"),
            TokenKind::Comment => write!(f, "comment"),
            TokenKind::Error => write!(f, "unexpected character"),
            TokenKind::Eof => write!(f, "end of file"),
            TokenKind::Percentage => write!(f, "%"),
        }
    }
}

impl TokenKind {
    /// Whether a value can end with this token: a name, a literal, `self`, a closing `)` or
    /// `]`. A `.` after one reads a field (`v.len`); after anything else, it's short for
    /// `self.` (`.len`). A `}` ends a statement, so `.len` after one is `self`'s too.
    pub fn ends_value(self) -> bool {
        matches!(
            self,
            TokenKind::Ident
                | TokenKind::SelfValue
                | TokenKind::SelfType
                | TokenKind::Int
                | TokenKind::Float
                | TokenKind::Text
                | TokenKind::True
                | TokenKind::False
                | TokenKind::None
                | TokenKind::CloseParen
                | TokenKind::CloseBracket
                | TokenKind::Question
        )
    }

    /// Whether a `.` between `prev` and `next` (the tokens around it, whitespace and
    /// comments left out) is short for `self.`: `.len`, not `v.len`.
    pub fn is_self_dot(prev: Option<TokenKind>, next: Option<TokenKind>) -> bool {
        next == Some(TokenKind::Ident) && !prev.is_some_and(TokenKind::ends_value)
    }

    /// The token for a diagnostic: literal tokens (`fn`, `;`) as code, the
    /// rest (identifier, end of file) as words.
    pub fn part(self) -> Part {
        match self {
            TokenKind::Ident
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Text
            | TokenKind::Whitespace
            | TokenKind::Comment
            | TokenKind::Error
            | TokenKind::Eof => Part::Text(self.to_string()),
            _ => code(self),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: Box<str>,
    pub span: Span,
}

pub fn lex(source: Arc<Source>) -> Vec<Token> {
    let input = source.content();
    let mut lexer = TokenKind::lexer(input);
    let mut tokens = Vec::new();
    // Line and column are tracked while moving forward, so lexing stays linear.
    let mut cursor = Cursor::default();

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let range = lexer.span();
        let start = cursor.advance_to(input, range.start);
        let end = cursor.advance_to(input, range.end);
        tokens.push(Token {
            kind,
            text: lexer.slice().into(),
            span: Span::new(
                start.0,
                start.1,
                end.0,
                end.1,
                range.start,
                range.len(),
                source.clone(),
            ),
        });
    }

    let end = cursor.advance_to(input, input.len());
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "".into(),
        span: Span::new(end.0, end.1, end.0, end.1, input.len(), 0, source),
    });

    tokens
}

/// A position in the source, moved forward as tokens are read.
struct Cursor {
    offset: usize,
    line: usize,
    col: usize,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            offset: 0,
            line: 1,
            col: 1,
        }
    }
}

impl Cursor {
    /// Moves to the byte `offset` and returns its line and column (1-indexed; columns
    /// count characters).
    fn advance_to(&mut self, input: &str, offset: usize) -> (usize, usize) {
        for c in input[self.offset..offset].chars() {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        self.offset = offset;
        (self.line, self.col)
    }
}

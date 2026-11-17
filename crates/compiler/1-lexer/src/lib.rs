mod highlight;
mod text;

pub use highlight::highlight;
use logos::Logos;
use som_base::message::{Part, code};
use som_base::{Source, Span};
use som_lang::Primitive;
use std::sync::Arc;
pub use text::{TextPart, text_parts};

// The token attributes are the source of truth; the `keywords_are_tokens` test keeps this
// in step with them.
pub const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "struct", "impl", "extern", "loop", "while", "break",
    "continue", "return", "for", "in", "use", "out", "true", "false", "none", "enum", "match",
    "trait", "type", "prop", "pub", "mod", "and", "or", "as", "self", "Self",
];

pub fn is_reserved(word: &str) -> bool {
    KEYWORDS.contains(&word) || Primitive::from_name(word).is_some()
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    #[token("fn")]
    Fn,
    #[token("prop")]
    Prop,
    #[token("self")]
    SelfValue,
    #[token("Self")]
    SelfType,
    #[token("extern")]
    Extern,
    #[token("struct")]
    Struct,
    #[token("impl")]
    Impl,
    #[token("pub")]
    Pub,
    #[token("mod")]
    Mod,
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

    /// Lexed as an identifier, then looked up.
    Primitive(Primitive),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    #[regex(r"[0-9][0-9_]*|0x[0-9a-fA-F_]+|0b[01_]+")]
    Int,
    #[regex(r"[0-9]*\.[0-9]+")]
    Float,
    #[regex(r#""([^"\\]|\\.)*""#)]
    Text,
    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*")]
    Builtin,
    #[regex(r"true")]
    True,
    #[regex(r"false")]
    False,
    #[token("none")]
    None,

    #[token("+=")]
    PlusEquals,
    #[token("-=")]
    MinusEquals,
    #[token("..=")]
    DotDotEquals,
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
    #[token("^")]
    Caret,
    #[token("|")]
    Pipe,
    #[token("~")]
    Tilde,

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

    #[regex(r"[ \t\r\n]+")]
    Whitespace,
    #[regex(r"//[^\n]*", allow_greedy = true)]
    Comment,
    /// Another language's comment, `# note` or `-- note`, on a line of its own.
    ForeignComment,
    /// `#T`: shared by its copies, counted.
    #[token("#")]
    Hash,

    // Other languages' operators: recognised, to be reported and read as som's.
    #[token("===")]
    TripleEquals,
    #[token("!==")]
    NotDoubleEquals,
    #[token("~=")]
    TildeEquals,
    #[token(":=")]
    ColonEquals,
    #[token("++")]
    PlusPlus,
    /// Also two borrows, `&&x`.
    #[token("&&")]
    AndAnd,
    #[token("||")]
    PipePipe,

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
            TokenKind::Primitive(p) => write!(f, "{}", p.name()),
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
            TokenKind::DotDotEquals => write!(f, "..="),
            TokenKind::PlusEquals => write!(f, "+="),
            TokenKind::MinusEquals => write!(f, "-="),
            TokenKind::Ellipsis => write!(f, "..."),
            TokenKind::SingleQuote => write!(f, "'"),
            TokenKind::DoubleQuote => write!(f, "\""),
            TokenKind::Whitespace => write!(f, "whitespace"),
            TokenKind::Comment => write!(f, "comment"),
            TokenKind::ForeignComment => write!(f, "comment"),
            TokenKind::Hash => write!(f, "#"),
            TokenKind::TripleEquals => write!(f, "==="),
            TokenKind::NotDoubleEquals => write!(f, "!=="),
            TokenKind::TildeEquals => write!(f, "~="),
            TokenKind::ColonEquals => write!(f, ":="),
            TokenKind::PlusPlus => write!(f, "++"),
            TokenKind::AndAnd => write!(f, "&&"),
            TokenKind::PipePipe => write!(f, "||"),
            TokenKind::Error => write!(f, "unexpected character"),
            TokenKind::Eof => write!(f, "end of file"),
            TokenKind::Percentage => write!(f, "%"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Pub => write!(f, "pub"),
            TokenKind::Mod => write!(f, "mod"),
            TokenKind::Builtin => write!(f, "built-in"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Tilde => write!(f, "~"),
        }
    }
}

impl TokenKind {
    /// Whitespace and comments, which the parser skips.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::ForeignComment
        )
    }

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

    /// Literal tokens (`fn`, `;`) as code, the rest (identifier, end of file) as words.
    pub fn part(self) -> Part {
        match self {
            TokenKind::Ident
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Text
            | TokenKind::Whitespace
            | TokenKind::Comment
            | TokenKind::ForeignComment
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
    let mut tokens = lex_without_end(&source, 0..input.len());
    foreign_comments(&mut tokens, input);
    tokens.push(end_of(&source, input.len()));
    tokens
}

/// The tokens of `range` of `source`, such as `{total}` inside text, ending in an `Eof`.
pub fn lex_range(source: Arc<Source>, range: std::ops::Range<usize>) -> Vec<Token> {
    let mut tokens = lex_without_end(&source, range.clone());
    tokens.push(end_of(&source, range.end));
    tokens
}

fn lex_without_end(source: &Arc<Source>, range: std::ops::Range<usize>) -> Vec<Token> {
    let input = source.content();
    let mut lexer = TokenKind::lexer(&input[range.clone()]);
    let mut tokens = Vec::new();
    // Tracked while moving forward, so lexing stays linear.
    let mut cursor = Cursor::default();
    while let Some(result) = lexer.next() {
        let text = lexer.slice();
        let kind = match result {
            Ok(TokenKind::Ident) => {
                Primitive::from_name(text).map_or(TokenKind::Ident, TokenKind::Primitive)
            }
            Ok(kind) => kind,
            Err(()) => TokenKind::Error,
        };
        let at = lexer.span();
        let (from, to) = (range.start + at.start, range.start + at.end);
        let start = cursor.advance_to(input, from);
        let end = cursor.advance_to(input, to);
        tokens.push(Token {
            kind,
            text: text.into(),
            span: Span::new(
                start.0,
                start.1,
                end.0,
                end.1,
                from,
                at.len(),
                source.clone(),
            ),
        });
    }
    tokens
}

fn end_of(source: &Arc<Source>, offset: usize) -> Token {
    let (line, col) = Cursor::default().advance_to(source.content(), offset);
    Token {
        kind: TokenKind::Eof,
        text: "".into(),
        span: Span::new(line, col, line, col, offset, 0, source.clone()),
    }
}

/// Text token `token` with values written in it, `"total: {n + 1}"`, in pieces for
/// colouring: the text as `Text` tokens, each `{` and `}` around a value, and the value's
/// tokens.
pub fn split_text(token: &Token) -> Vec<Token> {
    let source = token.span.source.clone();
    let start = token.span.start_offset;
    let text = &*token.text;
    let piece = |range: std::ops::Range<usize>, kind: TokenKind| Token {
        kind,
        text: text[range.clone()].into(),
        span: Span::from_range(start + range.start..start + range.end, source.clone()),
    };
    let mut tokens = Vec::new();
    let mut from = 0;
    for part in text_parts(text) {
        let TextPart::Value(value) = part else {
            continue;
        };
        let (open, close) = (value.start - 1, value.end);
        if from < open {
            tokens.push(piece(from..open, TokenKind::Text));
        }
        tokens.push(piece(open..open + 1, TokenKind::OpenBrace));
        tokens.extend(
            lex_range(source.clone(), start + value.start..start + value.end)
                .into_iter()
                .filter(|t| t.kind != TokenKind::Eof),
        );
        tokens.push(piece(close..close + 1, TokenKind::CloseBrace));
        from = close + 1;
    }
    if from < text.len() {
        tokens.push(piece(from..text.len(), TokenKind::Text));
    }
    tokens
}

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
    /// The line and column at byte `offset`, 1-indexed, counting characters.
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

/// `# note` and `-- note`, only where nothing comes before them on their line: elsewhere
/// `#` starts a type, `#Node`, and `--` is two minuses, `a--b`.
fn foreign_comments(tokens: &mut Vec<Token>, input: &str) {
    let mut i = 0;
    while i < tokens.len() {
        let starts_line = i == 0
            || tokens[i - 1].kind == TokenKind::Whitespace && tokens[i - 1].text.contains('\n')
            || i == 1 && tokens[0].kind == TokenKind::Whitespace;
        let dashes = i + 1 < tokens.len()
            && tokens[i].kind == TokenKind::Minus
            && tokens[i + 1].kind == TokenKind::Minus
            && tokens[i + 1].span.start_offset == tokens[i].span.start_offset + 1;
        let hash = tokens[i].kind == TokenKind::Hash;
        if !(starts_line && (dashes || hash)) {
            i += 1;
            continue;
        }
        let start = tokens[i].span.clone();
        let line_end = input[start.start_offset..]
            .find('\n')
            .map_or(input.len(), |n| start.start_offset + n);
        // Up to the line's end, but not the whitespace running on past it.
        let mut last = i;
        while last + 1 < tokens.len()
            && tokens[last + 1].span.start_offset < line_end
            && !(tokens[last + 1].kind == TokenKind::Whitespace
                && tokens[last + 1].text.contains('\n'))
        {
            last += 1;
        }
        let span = start.merge(&tokens[last].span);
        let text: Box<str> = input[span.start_offset..span.start_offset + span.length].into();
        tokens.splice(
            i..=last,
            [Token {
                kind: TokenKind::ForeignComment,
                text,
                span,
            }],
        );
        i += 1;
    }
}

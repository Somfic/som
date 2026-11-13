use owo_colors::{OwoColorize, Rgb};
use som_base::Source;
use som_base::diagnostics::*;
use std::sync::Arc;

use crate::{TokenKind, lex};

// Map token kinds to colors
fn token_color(kind: TokenKind) -> Rgb {
    match kind {
        TokenKind::Fn
        | TokenKind::Extern
        | TokenKind::Let
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::Loop
        | TokenKind::While
        | TokenKind::Break
        | TokenKind::Continue
        | TokenKind::Return
        | TokenKind::For
        | TokenKind::In
        | TokenKind::Struct
        | TokenKind::Use
        | TokenKind::Mut
        | TokenKind::Out
        | TokenKind::And
        | TokenKind::As
        | TokenKind::Or
        | TokenKind::Impl => MAUVE,
        TokenKind::I8
        | TokenKind::I16
        | TokenKind::I32
        | TokenKind::I64
        | TokenKind::I128
        | TokenKind::ISize
        | TokenKind::U8
        | TokenKind::U16
        | TokenKind::U32
        | TokenKind::U64
        | TokenKind::U128
        | TokenKind::USize
        | TokenKind::F32
        | TokenKind::F64
        | TokenKind::Bool
        | TokenKind::Char
        | TokenKind::Str
        | TokenKind::CStr => YELLOW,
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::Text
        | TokenKind::True
        | TokenKind::False
        | TokenKind::None => PEACH,
        TokenKind::Ident => TEXT,
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Ampersand
        | TokenKind::Question
        | TokenKind::DotDot
        | TokenKind::Slash
        | TokenKind::Equals
        | TokenKind::DoubleEquals
        | TokenKind::NotEquals
        | TokenKind::Bang
        | TokenKind::LessThan
        | TokenKind::GreaterThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThanOrEqual
        | TokenKind::Percentage => SKY,
        TokenKind::OpenParen
        | TokenKind::CloseParen
        | TokenKind::OpenBrace
        | TokenKind::CloseBrace
        | TokenKind::OpenBracket
        | TokenKind::CloseBracket
        | TokenKind::Comma
        | TokenKind::Colon
        | TokenKind::Semicolon
        | TokenKind::DoubleColon
        | TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::SingleQuote
        | TokenKind::DoubleQuote
        | TokenKind::Dot
        | TokenKind::Ellipsis => SUBTEXT0,
        TokenKind::Comment => SURFACE2,
        TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error => TEXT,
    }
}

// Check if a token should be italic
fn is_italic(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn | TokenKind::Let | TokenKind::If | TokenKind::Else | TokenKind::Mut | TokenKind::Out | TokenKind::Extern | // Keywords
        TokenKind::Comment // Comments
    )
}

// Syntax highlight a line of source code
/// Syntax-highlights one line of source code for terminal output.
/// Install with `som_base::diagnostics::set_highlighter(som_lexer::highlight)`.
pub fn highlight(line: &str) -> String {
    let source = Arc::new(Source::from_raw(line));
    let tokens = lex(source);

    let mut result = String::new();

    for token in tokens {
        if token.kind == TokenKind::Eof {
            break;
        }

        let color = token_color(token.kind);
        let colored_text = if is_italic(token.kind) {
            format!(
                "{}",
                token.text.truecolor(color.0, color.1, color.2).italic()
            )
        } else {
            format!("{}", token.text.truecolor(color.0, color.1, color.2))
        };
        result.push_str(&colored_text);
    }

    result
}

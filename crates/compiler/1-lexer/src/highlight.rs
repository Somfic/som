use owo_colors::{OwoColorize, Rgb};
use som_base::Source;
use som_base::diagnostics::*;
use som_lang::LangStruct;
use std::sync::Arc;

use crate::{TextPart, TokenKind, lex, text_parts};

const KEYWORD: Rgb = MAUVE;
const TYPE: Rgb = YELLOW;
const LITERAL: Rgb = PEACH;
const OPERATOR: Rgb = SKY;
const PUNCTUATION: Rgb = SUBTEXT0;
const COMMENT: Rgb = SURFACE2;
const PLAIN: Rgb = TEXT;

fn token_color(kind: TokenKind) -> Rgb {
    match kind {
        TokenKind::Fn
        | TokenKind::Pub
        | TokenKind::Mod
        | TokenKind::Prop
        | TokenKind::SelfValue
        | TokenKind::Extern
        | TokenKind::Let
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::Loop
        | TokenKind::While
        | TokenKind::Break
        | TokenKind::Continue
        | TokenKind::Return
        | TokenKind::Fail
        | TokenKind::Dyn
        | TokenKind::For
        | TokenKind::In
        | TokenKind::Struct
        | TokenKind::Enum
        | TokenKind::Trait
        | TokenKind::Type
        | TokenKind::Match
        | TokenKind::Use
        | TokenKind::Mut
        | TokenKind::Out
        | TokenKind::And
        | TokenKind::As
        | TokenKind::Or
        | TokenKind::Impl => KEYWORD,
        // A built-in is the compiler's own, like a primitive type.
        TokenKind::Primitive(_) | TokenKind::SelfType | TokenKind::Builtin => TYPE,
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::Text
        | TokenKind::True
        | TokenKind::False
        | TokenKind::None => LITERAL,
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Ampersand
        | TokenKind::Question
        | TokenKind::DotDot
        | TokenKind::DotDotEquals
        | TokenKind::PlusEquals
        | TokenKind::MinusEquals
        | TokenKind::Slash
        | TokenKind::Equals
        | TokenKind::DoubleEquals
        | TokenKind::NotEquals
        | TokenKind::Bang
        | TokenKind::LessThan
        | TokenKind::GreaterThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThanOrEqual
        | TokenKind::Percentage
        | TokenKind::Caret
        | TokenKind::Pipe
        | TokenKind::Tilde
        | TokenKind::AndAnd
        | TokenKind::PipePipe => OPERATOR,
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
        | TokenKind::Ellipsis
        | TokenKind::Hash
        | TokenKind::TripleEquals
        | TokenKind::NotDoubleEquals
        | TokenKind::TildeEquals
        | TokenKind::ColonEquals
        | TokenKind::PlusPlus => PUNCTUATION,
        TokenKind::Comment | TokenKind::ForeignComment => COMMENT,
        TokenKind::Ident | TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error => PLAIN,
    }
}

fn is_italic(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn
            | TokenKind::Prop
            | TokenKind::Let
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Mut
            | TokenKind::Out
            | TokenKind::Extern
            | TokenKind::Comment
            | TokenKind::ForeignComment
    )
}

fn paint(text: &str, color: Rgb) -> String {
    format!("{}", text.truecolor(color.0, color.1, color.2))
}

/// `RED`, `MAX_LEN`.
fn is_constant_name(name: &str) -> bool {
    name.len() > 1
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && (name.chars()).all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Syntax-highlights one line of source code for terminal output.
pub fn highlight(line: &str) -> String {
    let tokens = lex(Arc::new(Source::from_raw(line)));
    let visible: Vec<TokenKind> = (tokens.iter())
        .map(|t| t.kind)
        .filter(|k| !k.is_trivia())
        .collect();
    let mut seen: usize = 0;
    let mut result = String::new();
    for token in tokens {
        if token.kind == TokenKind::Eof {
            break;
        }
        let mut color = token_color(token.kind);
        // The standard library's `string` is a name, but it's text: coloured as a type.
        if token.kind == TokenKind::Ident && &*token.text == LangStruct::String.name() {
            color = TYPE;
        }
        if !token.kind.is_trivia() {
            let prev = seen.checked_sub(1).map(|i| visible[i]);
            let next = visible.get(seen + 1).copied();
            // `.len`, short for `self.len`: the dot is coloured like `self`.
            if token.kind == TokenKind::Dot && TokenKind::is_self_dot(prev, next) {
                color = token_color(TokenKind::SelfValue);
            }
            // A constant is coloured like the value it is.
            if token.kind == TokenKind::Ident && is_constant_name(&token.text) {
                color = token_color(TokenKind::Int);
            }
            seen += 1;
        }
        if token.kind == TokenKind::Text && token.text.contains('{') {
            result.push_str(&highlight_text(&token.text));
        } else if is_italic(token.kind) {
            let text = token.text.truecolor(color.0, color.1, color.2);
            result.push_str(&format!("{}", text.italic()));
        } else {
            result.push_str(&paint(&token.text, color));
        }
    }
    result
}

/// `"total: {n + 1}"`: the text coloured as text, each value as code between braces.
fn highlight_text(text: &str) -> String {
    let mut result = String::new();
    let mut from = 0;
    for part in text_parts(text) {
        let TextPart::Value(value) = part else {
            continue;
        };
        result.push_str(&paint(&text[from..value.start - 1], LITERAL));
        result.push_str(&paint("{", PUNCTUATION));
        result.push_str(&highlight(&text[value.clone()]));
        result.push_str(&paint("}", PUNCTUATION));
        from = value.end + 1;
    }
    result.push_str(&paint(&text[from..], LITERAL));
    result
}

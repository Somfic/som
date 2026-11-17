use std::sync::Arc;

use som_base::Source;
use som_lexer::{Token, TokenKind, lex};
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};

/// The legend's token types, by their index in `TOKEN_TYPES`.
const KEYWORD: u32 = 0;
const VARIABLE: u32 = 1;
const NUMBER: u32 = 2;
const STRING: u32 = 3;
const TYPE: u32 = 4;
const COMMENT: u32 = 5;
const OPERATOR: u32 = 6;
const FUNCTION: u32 = 7;
const STRUCT: u32 = 8;

pub(crate) const TOKEN_TYPES: [SemanticTokenType; 10] = [
    SemanticTokenType::KEYWORD,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::NUMBER,
    SemanticTokenType::STRING,
    SemanticTokenType::TYPE,
    SemanticTokenType::COMMENT,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::STRUCT,
    SemanticTokenType::PARAMETER,
];

/// A constant, which themes colour as one.
const READONLY: u32 = 1;
pub(crate) const TOKEN_MODIFIERS: [SemanticTokenModifier; 1] = [SemanticTokenModifier::READONLY];

/// How far past a name `generic_call` looks for the `>` closing its type arguments.
const TYPE_ARGS_LOOKAHEAD: usize = 64;

/// Whether `name` is written like a constant's: capitals, digits and `_`, at least two
/// characters, since a lone `T` is a type parameter.
fn is_constant_name(name: &str) -> bool {
    name.len() > 1
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Whether the name at `at` has type arguments, then a call: `make_identity<N>(`. The
/// brackets are matched, so `a < b` isn't one.
fn generic_call(visible: &[&Token], at: usize) -> bool {
    if visible.get(at + 1).map(|t| t.kind) != Some(TokenKind::LessThan) {
        return false;
    }
    let mut depth = 0;
    for (k, token) in visible
        .iter()
        .enumerate()
        .skip(at + 1)
        .take(TYPE_ARGS_LOOKAHEAD)
    {
        match token.kind {
            TokenKind::LessThan => depth += 1,
            TokenKind::GreaterThan => depth -= 1,
            TokenKind::Semicolon
            | TokenKind::OpenBrace
            | TokenKind::CloseBrace
            | TokenKind::OpenParen
            | TokenKind::CloseParen => return false,
            _ => {}
        }
        if depth == 0 {
            return visible.get(k + 1).map(|t| t.kind) == Some(TokenKind::OpenParen);
        }
    }
    false
}

/// Token kinds from the lexer, refined with one token of lookaround.
pub(crate) fn semantic_tokens(source: Arc<Source>) -> Vec<SemanticToken> {
    // The values in `"total: {n + 1}"` are code, coloured as such.
    let tokens: Vec<Token> = (lex(source).into_iter())
        .flat_map(
            |t| match t.kind == TokenKind::Text && t.text.contains('{') {
                true => som_lexer::split_text(&t),
                false => vec![t],
            },
        )
        .collect();
    let visible: Vec<&Token> = tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error
            )
        })
        .collect();

    let mut result = Vec::new();
    let (mut prev_line, mut prev_start) = (0, 0);
    for (i, token) in visible.iter().enumerate() {
        let Some(token_type) = token_type(&visible, i) else {
            continue;
        };
        let line = token.span.start.line.saturating_sub(1) as u32;
        let start = token.span.start.col.saturating_sub(1) as u32;
        let delta_line = line - prev_line;
        let constant = token.kind == TokenKind::Ident
            && is_constant_name(&token.text)
            && token_type == VARIABLE;
        result.push(SemanticToken {
            delta_line,
            delta_start: if delta_line == 0 {
                start - prev_start
            } else {
                start
            },
            length: token.span.length as u32,
            token_type,
            token_modifiers_bitset: if constant { READONLY } else { 0 },
        });
        (prev_line, prev_start) = (line, start);
    }
    result
}

fn token_type(visible: &[&Token], i: usize) -> Option<u32> {
    let token = visible[i];
    let next = visible.get(i + 1).map(|t| t.kind);
    let prev = i.checked_sub(1).map(|p| visible[p].kind);
    Some(match token.kind {
        // `.len`, short for `self.len`: the dot is coloured like `self`.
        TokenKind::Dot if TokenKind::is_self_dot(prev, next) => KEYWORD,
        TokenKind::Ident
            if next == Some(TokenKind::OpenParen)
                || prev == Some(TokenKind::Fn)
                || generic_call(visible, i) =>
        {
            FUNCTION
        }
        TokenKind::Ident if is_constant_name(&token.text) => VARIABLE,
        TokenKind::Ident if token.text.starts_with(char::is_uppercase) => STRUCT,
        // `string` is the standard library's, but it's text: coloured as a type, like `i32`.
        TokenKind::Ident if &*token.text == "string" => TYPE,
        // `Self` is the struct an impl block is for. A built-in, `@unreachable`, is coloured
        // the same.
        TokenKind::SelfType | TokenKind::Builtin => STRUCT,
        TokenKind::Ident => VARIABLE,
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::True
        | TokenKind::False
        | TokenKind::None => NUMBER,
        TokenKind::Text => STRING,
        TokenKind::Primitive(_) => TYPE,
        TokenKind::Comment => COMMENT,
        // `*` is the pointer type after `:` or `->`.
        TokenKind::Star if matches!(prev, Some(TokenKind::Colon | TokenKind::Arrow)) => TYPE,
        TokenKind::Star
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Slash
        | TokenKind::Percentage
        | TokenKind::Equals
        | TokenKind::DoubleEquals
        | TokenKind::NotEquals
        | TokenKind::Bang
        | TokenKind::LessThan
        | TokenKind::GreaterThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThanOrEqual
        | TokenKind::Ampersand
        | TokenKind::Question
        | TokenKind::DotDot
        | TokenKind::DotDotEquals
        | TokenKind::PlusEquals
        | TokenKind::MinusEquals
        | TokenKind::AndAnd
        | TokenKind::PipePipe => OPERATOR,
        // The rest of the keywords: a keyword's token is its text.
        _ if som_lexer::KEYWORDS.contains(&&*token.text) => KEYWORD,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn semantic_tokens_classify_calls_types_and_pointers() {
        let source = Arc::new(Source::from_raw("fn f(p: *) -> i32 { g(1) * 2 }"));
        let types: Vec<u32> = semantic_tokens(source)
            .iter()
            .map(|t| t.token_type)
            .collect();
        // fn f p * i32 g 1 * 2   (`(`, `)`, `:`, `->`, `{`, `}` aren't highlighted)
        assert_eq!(types, [0, 7, 1, 4, 4, 7, 2, 6, 2]);
    }

    #[test]
    fn generic_functions_are_functions() {
        let source = Arc::new(Source::from_raw(
            "fn make<N: usize>() -> i32 = make<N>() + 1; let x = a < b;",
        ));
        let types: Vec<u32> = semantic_tokens(source)
            .iter()
            .map(|t| t.token_type)
            .collect();
        // fn make < N usize > i32 = make < N > + 1 let x = a < b
        assert_eq!(types[1], 7, "the declared name: {types:?}");
        assert_eq!(types[8], 7, "the called name: {types:?}");
        assert_eq!(types[17], 1, "`a` in `a < b`: {types:?}");
    }

    #[test]
    fn at_const_is_a_builtin_and_constants_are_not_types() {
        let source = Arc::new(Source::from_raw(
            "@const MAX_LEN: Size = Color::RED; let const = T;",
        ));
        let types: Vec<u32> = semantic_tokens(source)
            .iter()
            .map(|t| t.token_type)
            .collect();
        // @const MAX_LEN Size = Color RED let const = T
        assert_eq!(types, [8, 1, 8, 6, 8, 1, 0, 1, 6, 8]);
        let readonly: Vec<u32> = semantic_tokens(Arc::new(Source::from_raw(
            "@const MAX_LEN: Size = Color::RED; let const = T;",
        )))
        .iter()
        .map(|t| t.token_modifiers_bitset)
        .collect();
        assert_eq!(readonly, [0, 1, 0, 0, 0, 1, 0, 0, 0, 0]);
    }

    #[test]
    fn self_is_a_keyword_and_self_type_a_struct() {
        let text = "fn f(self) -> Self = Point { x: self.x };";
        let tokens = semantic_tokens(Arc::new(Source::from_raw(text)));
        // Each token's column, from the distances between them, all on one line.
        let mut column = 0;
        let at: HashMap<u32, u32> = (tokens.iter())
            .map(|t| {
                column += t.delta_start;
                (column, t.token_type)
            })
            .collect();
        let keyword = at[&0];
        assert_eq!(at[&(text.find("self").unwrap() as u32)], keyword);
        assert_eq!(at[&(text.rfind("self").unwrap() as u32)], keyword);
        // `.x` is `self.x`: its dot is coloured like `self`. The dot in `self.x` isn't.
        let dotted = "fn f(self) = .x + self.y;";
        let tokens = semantic_tokens(Arc::new(Source::from_raw(dotted)));
        let mut column = 0;
        let dots: HashMap<u32, u32> = (tokens.iter())
            .map(|t| {
                column += t.delta_start;
                (column, t.token_type)
            })
            .collect();
        assert_eq!(
            dots.get(&(dotted.find(".x").unwrap() as u32)),
            Some(&keyword)
        );
        assert_eq!(dots.get(&(dotted.find(".y").unwrap() as u32)), None);
        assert_eq!(
            at[&(text.find("Self").unwrap() as u32)],
            at[&(text.find("Point").unwrap() as u32)]
        );
    }
}

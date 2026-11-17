use std::ops::Range;

/// A piece of text that has values written in it, `"total: {n}"`, by byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextPart {
    /// Text as written, escapes and all.
    Literal(Range<usize>),
    /// `{{` or `}}`: the brace itself.
    Brace(Range<usize>),
    /// What's between a `{` and the `}` after it.
    Value(Range<usize>),
    /// A `{` with no `}` after it.
    Unclosed(usize),
    /// A `}` with no `{` before it.
    Unopened(usize),
}

/// The one place text is split around its values: the lexer for colouring, the parser for
/// reading them.
pub fn text_parts(text: &str) -> Vec<TextPart> {
    let mut parts = Vec::new();
    let mut literal: Option<Range<usize>> = None;
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let (part, len) = if rest.starts_with("{{") || rest.starts_with("}}") {
            (Some(TextPart::Brace(at..at + 2)), 2)
        } else if rest.starts_with('\\') {
            // An escape is text, `\u{41}`'s braces too.
            (None, escape_len(rest))
        } else if rest.starts_with('{') {
            match rest.find('}') {
                Some(close) => (Some(TextPart::Value(at + 1..at + close)), close + 1),
                None => (Some(TextPart::Unclosed(at)), 1),
            }
        } else if rest.starts_with('}') {
            (Some(TextPart::Unopened(at)), 1)
        } else {
            (None, rest.chars().next().map_or(1, char::len_utf8))
        };
        match part {
            None => literal.get_or_insert(at..at).end = at + len,
            Some(part) => {
                parts.extend(literal.take().map(TextPart::Literal));
                parts.push(part);
            }
        }
        at += len;
    }
    parts.extend(literal.map(TextPart::Literal));
    parts
}

/// How long the escape at the start of `text`, a `\`, is as written: `\n` is 2, `\u{41}` 6.
pub fn escape_len(text: &str) -> usize {
    match text[1..].chars().next() {
        Some('u') if text[2..].starts_with('{') => text.find('}').map_or(2, |end| end + 1),
        Some(c) => 1 + c.len_utf8(),
        None => 1,
    }
}

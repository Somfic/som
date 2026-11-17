use som_ast::{Expr, Id, Part, Span};
use som_base::message::code;
use som_base::msg;
use som_lexer::{TextPart, Token, text_parts};

use crate::Parser;
use crate::cursor::unquoted;

impl Parser<'_> {
    /// A text literal: plain, or with values written in it.
    pub(crate) fn text_literal(&mut self, token: &Token) -> Expr {
        let raw = unquoted(&token.text);
        if raw.contains('{') || raw.contains('}') {
            self.interpolated(token)
        } else {
            Expr::Str(self.unescape(raw, &token.span).into())
        }
    }

    /// `"total: {n}"`: the text in pieces, each `{…}` read as an expression.
    fn interpolated(&mut self, token: &Token) -> Expr {
        let raw = unquoted(&token.text);
        // Where the text inside the quotes starts in the file.
        let base = token.span.start_offset + 1;
        let brace_at =
            |i: usize| Span::from_range(base + i..base + i + 1, token.span.source.clone());
        let mut parts = Vec::new();
        let mut piece = String::new();
        for part in text_parts(raw) {
            match part {
                TextPart::Literal(range) => piece.push_str(&raw[range]),
                TextPart::Brace(range) => piece.push_str(&raw[range.start..range.start + 1]),
                TextPart::Value(range) => {
                    self.push_text_piece(&mut parts, &mut piece, &token.span);
                    let range = base + range.start..base + range.end;
                    match self.text_value(&token.span, range) {
                        Some(value) => parts.push(Part::Value(value)),
                        None => return Expr::Hole,
                    }
                }
                TextPart::Unclosed(i) => {
                    self.error_at(
                        brace_at(i),
                        msg![
                            "a ",
                            code("{"),
                            " in text starts a value, and needs a ",
                            code("}")
                        ],
                        msg!["for the brace itself, write ", code("{{")],
                    );
                    return Expr::Hole;
                }
                TextPart::Unopened(i) => {
                    self.error_at(
                        brace_at(i),
                        msg!["a ", code("}"), " in text ends a value, and there's none"],
                        msg!["for the brace itself, write ", code("}}")],
                    );
                    return Expr::Hole;
                }
            }
        }
        // Only text after all, `"{{}}"` or `"\u{41}"`: plain text.
        if parts.is_empty() {
            return Expr::Str(self.unescape(&piece, &token.span).into());
        }
        self.push_text_piece(&mut parts, &mut piece, &token.span);
        Expr::Interpolated(parts)
    }

    fn push_text_piece(&mut self, parts: &mut Vec<Part>, piece: &mut String, span: &Span) {
        if piece.is_empty() {
            return;
        }
        let text = self.unescape(&std::mem::take(piece), span);
        let text = self.ast.exprs.alloc(Expr::Str(text.into()), span.clone());
        parts.push(Part::Text(text));
    }

    /// The expression `{…}` in text holds, at `range` of the file: read on its own.
    fn text_value(&mut self, text: &Span, range: std::ops::Range<usize>) -> Option<Id<Expr>> {
        let tokens = som_lexer::lex_range(text.source.clone(), range.clone());
        let outer = (
            std::mem::replace(&mut self.tokens, tokens),
            std::mem::replace(&mut self.pos, 0),
        );
        self.skip_trivia();
        let value = if self.at_eof() {
            let at = Span::from_range(range.start - 1..range.end + 1, text.source.clone());
            self.error_at(
                at,
                msg![code("{}"), " in text holds nothing"],
                msg![
                    "put the value to write between them, ",
                    code("{total}"),
                    ", or write ",
                    code("{{}}"),
                    " for the braces"
                ],
            );
            None
        } else {
            let value = self.parse_expr();
            if value.is_some() && !self.at_eof() {
                let at = self.current_span();
                self.error_at(
                    at,
                    "expected the end of the value",
                    msg!["a value in text ends with ", code("}")],
                );
            }
            value
        };
        (self.tokens, self.pos) = outer;
        value
    }

    /// The text of a string literal with its escapes resolved: `\n`, `\t`, `\r`, `\0`,
    /// `\\`, `\"`, `\'`, `\x1b` and `\u{1F600}`. An unknown escape is reported, and kept as
    /// written.
    fn unescape(&mut self, raw: &str, span: &Span) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(c @ ('\\' | '"' | '\'')) => out.push(c),
                Some('x') => out.extend(self.unescape_byte(&mut chars, span)),
                Some('u') => out.extend(self.unescape_unicode(&mut chars, span)),
                other => {
                    let escape = format!("\\{}", other.map(String::from).unwrap_or_default());
                    self.error_at(
                        span.clone(),
                        msg!["unknown escape ", code(&escape)],
                        msg![
                            "expected one of ",
                            code("\\n \\t \\r \\0 \\\\ \\\" \\' \\x1b \\u{1F600}")
                        ],
                    );
                    out.push_str(&escape);
                }
            }
        }
        out
    }

    /// `\x1b`, after its `x`: two hex digits, up to `\x7f`.
    fn unescape_byte(&mut self, chars: &mut std::str::Chars, span: &Span) -> Option<char> {
        let digits: String = chars.clone().take(2).collect();
        let value = (digits.len() == 2)
            .then(|| u8::from_str_radix(&digits, 16).ok())
            .flatten();
        match value {
            Some(byte) if byte <= 0x7f => {
                chars.nth(1);
                Some(byte as char)
            }
            Some(_) => {
                chars.nth(1);
                self.error_at(
                    span.clone(),
                    msg![
                        "escape ",
                        code(format!("\\x{digits}")),
                        " is past ",
                        code("\\x7f")
                    ],
                    msg![
                        "text is UTF-8: for a character past ",
                        code("\\x7f"),
                        ", write ",
                        code(format!("\\u{{{digits}}}"))
                    ],
                );
                None
            }
            None => {
                self.error_at(
                    span.clone(),
                    msg![code("\\x"), " needs two hex digits"],
                    msg!["like ", code("\\x1b"), " for escape, or ", code("\\x00")],
                );
                None
            }
        }
    }

    /// `\u{1F600}`, after its `u`: the character with that number, in hex.
    fn unescape_unicode(&mut self, chars: &mut std::str::Chars, span: &Span) -> Option<char> {
        let rest = chars.as_str();
        let digits = (rest.strip_prefix('{'))
            .and_then(|r| r.find('}').map(|end| &r[..end]))
            .filter(|d| (1..=6).contains(&d.len()));
        let character = digits
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .and_then(char::from_u32);
        match (digits, character) {
            (Some(d), Some(c)) => {
                chars.nth(d.len() + 1);
                Some(c)
            }
            (Some(d), None) => {
                let message = msg![code(format!("\\u{{{d}}}")), " isn't a character"];
                chars.nth(d.len() + 1);
                self.error_at(
                    span.clone(),
                    message,
                    "a character's number is at most 10FFFF, and not D800 to DFFF",
                );
                None
            }
            (None, _) => {
                self.error_at(
                    span.clone(),
                    msg![code("\\u"), " needs a character's number in braces"],
                    msg!["like ", code("\\u{1F600}"), ", in hex"],
                );
                None
            }
        }
    }
}

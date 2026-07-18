use std::collections::BTreeMap;

use som_common::{Id, Source, Span};

use crate::{Expr, Layout, Parser, TextPart, TokenKind};

impl Parser<'_> {
    /// A layout item starts with a string (text node) or a tag name followed by
    /// a layout indicator (`@` config or an indented child block). A bare
    /// identifier followed by anything else is an ordinary expression.
    pub(crate) fn at_layout_item(&self) -> bool {
        match self.peek().kind {
            TokenKind::Text => true,
            TokenKind::Ident => matches!(
                self.peek_nth(1).kind,
                TokenKind::At | TokenKind::Indent
            ),
            _ => false,
        }
    }

    pub(crate) fn parse_layout(&mut self) -> Id<Layout> {
        if self.peek().kind == TokenKind::Text {
            self.parse_text()
        } else {
            self.parse_element()
        }
    }

    fn parse_element(&mut self) -> Id<Layout> {
        let tag_token = self.expect(TokenKind::Ident);
        let tag = tag_token.text;
        let mut span = tag_token.span;

        let mut events = BTreeMap::new();
        let attr = BTreeMap::new();

        // same-line config: `@event: <expr>`
        while self.peek().kind == TokenKind::At {
            self.next();
            let name = self.expect(TokenKind::Ident);
            self.expect(TokenKind::Colon);
            let body = self.parse_expr();
            span = span.merge(self.ast[body].span());
            events.insert(name.text, body);
        }

        // children: an indented block of nested layout items
        let mut children = Vec::new();
        if self.peek().kind == TokenKind::Indent {
            self.next();
            loop {
                self.skip_newlines();
                if matches!(self.peek().kind, TokenKind::Dedent | TokenKind::Eof) {
                    break;
                }
                let child = self.parse_layout();
                span = span.merge(self.ast[child].span());
                children.push(child);
            }
            self.try_eat(TokenKind::Dedent);
        }

        self.ast.add_layout(Layout::Element {
            tag,
            events,
            attr,
            children,
            span,
        })
    }

    fn parse_text(&mut self) -> Id<Layout> {
        let token = self.next();
        let span = token.span;
        let text = self.parse_text_parts(&token.text, span);
        self.ast.add_layout(Layout::Text { text, span })
    }

    /// Split a string literal into literal runs and `{expr}` interpolations.
    fn parse_text_parts(&mut self, raw: &str, span: Span) -> Vec<TextPart> {
        let quote = raw.starts_with('"') as u32;
        let inner = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw);
        // Source offset of `inner`'s first byte, used to re-anchor the spans of
        // interpolated fragments back onto the original file.
        let base = span.start + quote;

        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = inner.char_indices();

        while let Some((i, c)) = chars.next() {
            if c == '{' {
                if !literal.is_empty() {
                    parts.push(TextPart::Str {
                        text: std::mem::take(&mut literal).into(),
                        span,
                    });
                }
                let fragment_start = base + i as u32 + c.len_utf8() as u32;
                let mut source = String::new();
                for (_, cc) in chars.by_ref() {
                    if cc == '}' {
                        break;
                    }
                    source.push(cc);
                }
                let value = self.parse_interpolation(&source, span.source, fragment_start);
                parts.push(TextPart::Interp { value, span });
            } else {
                literal.push(c);
            }
        }

        if !literal.is_empty() {
            parts.push(TextPart::Str {
                text: literal.into(),
                span,
            });
        }

        parts
    }

    /// Parse an interpolated expression into the shared arena by temporarily
    /// swapping in a token stream lexed from the fragment. Fragment spans are
    /// shifted by `base` so diagnostics point at the real source location.
    fn parse_interpolation(&mut self, source: &str, src: Id<Source>, base: u32) -> Id<Expr> {
        let mut tokens = crate::lex(src, source);
        for token in &mut tokens {
            token.span.start += base;
            token.span.end += base;
        }
        let saved_tokens = std::mem::replace(&mut self.tokens, tokens);
        let saved_pos = std::mem::replace(&mut self.pos, 0);
        let expr = self.parse_expr();
        self.tokens = saved_tokens;
        self.pos = saved_pos;
        expr
    }
}

#[cfg(test)]
mod tests {
    use som_common::{DiagnosticSink, Id};

    use crate::{Ast, Expr, Layout, Root, TextPart};

    fn parse(src: &str) -> (Ast, DiagnosticSink) {
        let mut diags = DiagnosticSink::new();
        let ast = crate::parse(Id::new(0), src, &mut diags);
        (ast, diags)
    }

    #[test]
    fn counter_program() {
        let src = "\
let count = 0

main
  button @click: count += 1
    \"+1\"
  \"count: {count}\"
  \"doubled: {count * 2}\"
";
        let (ast, diags) = parse(src);
        assert!(!diags.has_errors(), "unexpected parse errors");
        assert_eq!(ast.root.len(), 2);

        assert!(matches!(ast.root[0], Root::Stmt(_)));

        let Root::Layout(main_id) = ast.root[1] else {
            panic!("expected a layout root");
        };
        let Layout::Element {
            tag,
            events,
            children: main_children,
            ..
        } = &ast[main_id]
        else {
            panic!("expected an element");
        };
        assert_eq!(&**tag, "main");
        assert!(events.is_empty());
        assert_eq!(main_children.len(), 3);

        // child 0: `button @click: count += 1` with a single text child
        let Layout::Element {
            tag,
            events,
            children: button_children,
            ..
        } = &ast[main_children[0]]
        else {
            panic!("expected button element");
        };
        assert_eq!(&**tag, "button");
        assert!(events.contains_key("click"));
        let Expr::Assignment { target, .. } = &ast[events["click"]] else {
            panic!("expected an assignment handler");
        };
        assert_eq!(&**target, "count");
        assert_eq!(button_children.len(), 1);
        assert!(matches!(&ast[button_children[0]], Layout::Text { .. }));

        // child 1: interpolated text `count: {count}`
        let Layout::Text { text, .. } = &ast[main_children[1]] else {
            panic!("expected text node");
        };
        assert_eq!(text.len(), 2);
        assert!(matches!(&text[0], TextPart::Str { text, .. } if &**text == "count: "));
        let TextPart::Interp { value, .. } = &text[1] else {
            panic!("expected interpolation");
        };
        assert!(matches!(&ast[*value], Expr::Variable { name, .. } if &**name == "count"));

        // child 2: interpolation wrapping a binary expression `count * 2`
        let Layout::Text { text, .. } = &ast[main_children[2]] else {
            panic!("expected text node");
        };
        let TextPart::Interp { value, .. } = &text[1] else {
            panic!("expected interpolation");
        };
        assert!(matches!(&ast[*value], Expr::Binary { .. }));
    }
}

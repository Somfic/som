use crate::lexer::Syntax;
use crate::parser::{union, Parser};
use std::collections::HashSet;

impl<'a> Parser<'a> {
    pub(super) fn program(&mut self) {
        self.builder.start_node(Syntax::Root.into());

        // Consume leading whitespace/comments
        while matches!(self.input.peek(), Syntax::Whitespace | Syntax::Comment) {
            let (syntax, text) = self.input.advance();
            self.builder.token(syntax.into(), text);
        }

        let anchor = HashSet::from([Syntax::EndOfFile]);

        while !self.input.at(Syntax::EndOfFile) {
            if self.input.at(Syntax::FnKw) {
                self.func_dec(anchor.clone());
            } else {
                // Unexpected token at top level, recover
                self.recover_until(anchor.clone(), vec![Syntax::FnKw]);
            }
        }

        self.builder.finish_node();
    }

    pub(super) fn func_dec(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::FuncDec, |this| {
            let anchor = union(&anchor, [Syntax::LeftParen, Syntax::LeftBrace]);

            this.expect(Syntax::FnKw, union(&anchor, [Syntax::Ident]));
            this.expect(Syntax::Ident, union(&anchor, [Syntax::LeftParen]));

            // Parameters
            this.expect(
                Syntax::LeftParen,
                union(&anchor, [Syntax::RightParen, Syntax::Ident]),
            );

            if !this.input.at(Syntax::RightParen) {
                loop {
                    this.func_param(union(&anchor, [Syntax::Comma, Syntax::RightParen]));

                    if this.eat(Syntax::Comma).is_none() {
                        break;
                    }
                }
            }

            this.expect(
                Syntax::RightParen,
                union(&anchor, [Syntax::Arrow, Syntax::LeftBrace]),
            );

            // Optional return type
            if this.eat(Syntax::Arrow).is_some() {
                this.type_annotation(union(&anchor, [Syntax::LeftBrace]));
            }

            // Body
            this.block(anchor);
        });
    }

    fn func_param(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::FuncParam, |this| {
            this.expect(Syntax::Ident, union(&anchor, [Syntax::Colon]));

            if this.eat(Syntax::Colon).is_some() {
                this.type_annotation(anchor);
            }
        });
    }
}

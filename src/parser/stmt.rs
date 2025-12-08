use crate::lexer::Syntax;
use crate::parser::{union, Parser};
use std::collections::HashSet;

impl<'a> Parser<'a> {
    pub(super) fn block(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::Block, |this| {
            let anchor = union(&anchor, [Syntax::RightBrace]);

            this.expect(Syntax::LeftBrace, union(&anchor, [Syntax::LetKw]));

            while !this.input.at(Syntax::RightBrace) && !this.input.at(Syntax::EndOfFile) {
                // Try let statement
                if this.input.at(Syntax::LetKw) {
                    this.let_stmt(anchor.clone());
                } else {
                    // Parse as expression statement
                    this.expr_stmt(anchor.clone());

                    // Check if we need a semicolon
                    // Last expression in block doesn't need semicolon
                    if !this.input.at(Syntax::RightBrace) && !this.input.at(Syntax::EndOfFile) {
                        // Not the last expression, expect semicolon
                        if this.eat(Syntax::Semicolon).is_none() {
                            break;
                        }
                    }
                }
            }

            this.expect(Syntax::RightBrace, anchor);
        });
    }

    fn let_stmt(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::LetStmt, |this| {
            let anchor = union(&anchor, [Syntax::Semicolon]);

            this.expect(Syntax::LetKw, union(&anchor, [Syntax::Ident]));
            this.expect(Syntax::Ident, union(&anchor, [Syntax::Colon, Syntax::Eq]));

            // Optional type annotation
            if this.eat(Syntax::Colon).is_some() {
                this.type_annotation(union(&anchor, [Syntax::Eq]));
            }

            this.expect(Syntax::Eq, anchor.clone());
            let _ = this.expr(union(&anchor, [Syntax::Semicolon]));
            this.expect(Syntax::Semicolon, anchor);
        });
    }
}

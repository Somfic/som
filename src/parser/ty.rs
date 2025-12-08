use crate::lexer::Syntax;
use crate::parser::Parser;
use std::collections::HashSet;

impl<'a> Parser<'a> {
    pub(super) fn type_annotation(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::TypeAnnotation, |this| {
            this.expect(Syntax::Ident, anchor);
        });
    }
}

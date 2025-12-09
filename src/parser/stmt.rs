use crate::ast::{Stmt, StmtId};
use crate::lexer::TokenKind;
use crate::parser::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_let_stmt(&mut self) -> Option<StmtId> {
        let start_span = self.peek_span();

        // let
        self.expect(TokenKind::LetKw)?;

        // name
        let (name, _) = self.parse_ident()?;

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // = value
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        // ;
        self.expect(TokenKind::Semicolon)?;

        let end_span = self.previous_span();
        let span = start_span.merge(end_span);

        Some(self.ast.alloc_stmt_with_span(
            Stmt::Let { name, ty, value },
            span,
        ))
    }
}

use som_common::Id;

use crate::{Parser, Stmt, TokenKind};

impl Parser<'_> {
    pub(crate) fn parse_stmt(&mut self) -> Id<Stmt> {
        match self.peek().kind {
            TokenKind::Let => self.parse_let(),
            _ => {
                // expression statement
                let expr = self.parse_expr();
                let semi = self.expect(TokenKind::Semicolon);
                let span = self.ast[expr].span().merge(semi.span);
                self.stmt(Stmt::Expr { expr, span })
            }
        }
    }

    fn stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.ast.add_stmt(stmt)
    }

    fn parse_let(&mut self) -> Id<Stmt> {
        let let_token = self.expect(TokenKind::Let);
        let ident_token = self.expect(TokenKind::Ident);
        let eq_token = self.expect(TokenKind::Equals);
        let expr = self.parse_expr();
        let semi_token = self.expect(TokenKind::Semicolon);

        let span = let_token
            .span
            .merge(ident_token.span)
            .merge(eq_token.span)
            .merge(self.ast[expr].span())
            .merge(semi_token.span);

        self.stmt(Stmt::Let {
            ident: ident_token.text,
            expr,
            span,
        })
    }
}

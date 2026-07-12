use som_common::Id;

use crate::{Expr, Parser, Stmt, Ty, TokenKind};

pub enum ExprOrStmt {
    Expr(Id<Expr>),
    Stmt(Id<Stmt>),
}

impl Parser<'_> {
    pub(crate) fn parse_stmt(&mut self) -> ExprOrStmt {
        match self.peek().kind {
            TokenKind::Let => ExprOrStmt::Stmt(self.parse_let()),
            _ => {
                let expr = self.parse_expr();

                if let Some(semi) = self.try_eat(TokenKind::Semicolon) {
                    let span = self.ast[expr].span().merge(semi.span);
                    ExprOrStmt::Stmt(self.stmt(Stmt::Expr { span, expr }))
                } else {
                    ExprOrStmt::Expr(expr)
                }
            }
        }
    }

    fn stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.ast.add_stmt(stmt)
    }

    fn parse_let(&mut self) -> Id<Stmt> {
        let let_token = self.expect(TokenKind::Let);
        let ident_token = self.expect(TokenKind::Ident);
        let ty = self
            .try_eat(TokenKind::Colon)
            .map(|_| self.parse_ty());
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
            ty,
            expr,
            span,
        })
    }

    fn parse_ty(&mut self) -> Ty {
        let token = self.next();
        match token.kind {
            TokenKind::I32 => Ty::I32 { span: token.span },
            TokenKind::Bool => Ty::Bool { span: token.span },
            _ => {
                self.diags.emit_error(
                    token.span,
                    format!("expected a type, found `{}`", token.kind),
                );
                Ty::Error { span: token.span }
            }
        }
    }
}

use som_common::Id;

use crate::{Parser, Stmt, TokenKind, Ty};

impl Parser<'_> {
    pub(crate) fn parse_let(&mut self) -> Id<Stmt> {
        let let_token = self.expect(TokenKind::Let);
        let ident_token = self.expect(TokenKind::Ident);
        let ty = self.try_eat(TokenKind::Colon).map(|_| self.parse_ty());
        let eq_token = self.expect(TokenKind::Equals);
        let expr = self.parse_expr();

        let span = let_token
            .span
            .merge(ident_token.span)
            .merge(eq_token.span)
            .merge(self.ast[expr].span());

        self.ast.add_stmt(Stmt::Let {
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

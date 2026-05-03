use som::{Id, Span};

use crate::{
    Expr, Parser, Stmt,
    parser::rules::{InfixRule, PrefixRule, infix, prefix},
    token::{Token, TokenKind},
};

fn prefix_rule(token: TokenKind) -> Option<PrefixRule> {
    Some(match token {
        TokenKind::Int => prefix(parse_int_literal),
        _ => return None,
    })
}

fn infix_rule(token: TokenKind) -> Option<InfixRule> {
    Some(match token {
        TokenKind::Plus => infix(parse_binary, 50, 51),
        _ => return None,
    })
}

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Id<Expr> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Id<Expr> {
        let prefix = match prefix_rule(self.peek().kind) {
            Some(r) => r,
            None => {
                let token = self.next();
                return self.ast.add_expr(Expr::Error { span: token.span });
            }
        };
        let mut lhs = (prefix.parse)(self);

        while let Some(infix) = infix_rule(self.peek().kind) {
            if infix.l_bp < min_bp {
                break;
            }
            lhs = (infix.parse)(self, lhs);
        }

        lhs
    }

    fn expr(&mut self, expr: Expr) -> Id<Expr> {
        self.ast.add_expr(expr)
    }

    fn stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.ast.add_stmt(stmt)
    }
}

fn parse_int_literal(parser: &mut Parser) -> Id<Expr> {
    let token = parser.next();
    let value = token.text.parse().unwrap_or_else(|_| {
        parser
            .diags
            .emit_error(token.span, "invalid integer literal".to_string());
        0
    });

    parser.expr(Expr::Int {
        value,
        span: token.span,
    })
}

fn parse_binary(parser: &mut Parser, lhs: Id<Expr>) -> Id<Expr> {
    let op = parser.next();
    let rule = infix_rule(op.kind).unwrap();
    let rhs = parser.parse_expr_bp(rule.r_bp);

    parser.expr(Expr::Binary {
        op: op.kind,
        lhs,
        rhs,
        span: parser
            .ast
            .get_expr(lhs)
            .span()
            .merge(parser.ast.get_expr(rhs).span()),
    })
}

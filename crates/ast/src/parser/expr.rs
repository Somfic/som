use som_common::Id;

use crate::{
    BinaryOp, Expr, Parser, Stmt, UnaryOp,
    parser::{
        rules::{InfixRule, PrefixRule, infix, prefix},
        stmt::ExprOrStmt,
    },
    token::TokenKind,
};

/// starts with
fn prefix_rule(token: TokenKind) -> Option<PrefixRule> {
    Some(match token {
        TokenKind::Int => prefix(|p| p.parse_int_literal()),
        TokenKind::True => prefix(|p| p.parse_bool_literal()),
        TokenKind::False => prefix(|p| p.parse_bool_literal()),
        TokenKind::OpenParen => prefix(|p| p.parse_grouping()),
        TokenKind::Minus => prefix(|p| p.parse_unary()),
        TokenKind::Bang => prefix(|p| p.parse_unary()),
        TokenKind::OpenBrace => prefix(|p| p.parse_block()),
        _ => return None,
    })
}

/// continues with
fn infix_rule(token: TokenKind) -> Option<InfixRule> {
    Some(match token {
        TokenKind::Plus => infix(|p, lhs| p.parse_binary(lhs), 50, 51),
        TokenKind::Minus => infix(|p, lhs| p.parse_binary(lhs), 50, 51),
        TokenKind::Star => infix(|p, lhs| p.parse_binary(lhs), 60, 61),
        TokenKind::Slash => infix(|p, lhs| p.parse_binary(lhs), 60, 61),
        TokenKind::DoubleEquals => infix(|p, lhs| p.parse_binary(lhs), 30, 31),
        TokenKind::NotEquals => infix(|p, lhs| p.parse_binary(lhs), 30, 31),
        TokenKind::LessThan => infix(|p, lhs| p.parse_binary(lhs), 40, 41),
        TokenKind::LessThanOrEquals => infix(|p, lhs| p.parse_binary(lhs), 40, 41),
        TokenKind::GreaterThan => infix(|p, lhs| p.parse_binary(lhs), 40, 41),
        TokenKind::GreaterThanOrEquals => infix(|p, lhs| p.parse_binary(lhs), 40, 41),
        TokenKind::And => infix(|p, lhs| p.parse_binary(lhs), 20, 21),
        TokenKind::Or => infix(|p, lhs| p.parse_binary(lhs), 10, 11),
        TokenKind::If => infix(|p, lhs| p.parse_conditional(lhs), 5, 4),
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
                self.diags
                    .emit_error(token.span, format!("unexpected token `{}`", token.text));
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

    fn parse_int_literal(&mut self) -> Id<Expr> {
        let token = self.next();
        let value = token.text.parse().unwrap_or_else(|_| {
            self.diags
                .emit_error(token.span, "invalid integer literal".to_string());
            0
        });

        self.expr(Expr::Int {
            value,
            span: token.span,
        })
    }

    fn parse_bool_literal(&mut self) -> Id<Expr> {
        let token = self.next();
        let value = match token.kind {
            TokenKind::True => true,
            TokenKind::False => false,
            _ => unreachable!(),
        };

        self.expr(Expr::Bool {
            value,
            span: token.span,
        })
    }

    fn parse_grouping(&mut self) -> Id<Expr> {
        self.next();
        let expr = self.parse_expr();
        self.expect(TokenKind::CloseParen);

        expr
    }

    fn parse_conditional(&mut self, truthy: Id<Expr>) -> Id<Expr> {
        self.next();
        let condition = self.parse_expr();
        self.expect(TokenKind::Else);
        let falsy = self.parse_expr_bp(4);

        self.expr(Expr::Condition {
            condition,
            truthy,
            falsy,
            span: self.ast[truthy].span().merge(self.ast[falsy].span()),
        })
    }

    fn parse_unary(&mut self) -> Id<Expr> {
        let token = self.next();

        let op = match token.kind {
            TokenKind::Minus => UnaryOp::Negate,
            TokenKind::Bang => UnaryOp::Not,
            _ => unreachable!(),
        };

        let operand = self.parse_expr_bp(70);

        self.expr(Expr::Unary {
            op,
            operand,
            span: token.span.merge(self.ast[operand].span()),
        })
    }

    fn parse_binary(&mut self, lhs: Id<Expr>) -> Id<Expr> {
        let op = self.next();
        let rule = infix_rule(op.kind).unwrap();
        let rhs = self.parse_expr_bp(rule.r_bp);

        let op = match op.kind {
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Subtract,
            TokenKind::Star => BinaryOp::Multiply,
            TokenKind::Slash => BinaryOp::Divide,
            TokenKind::DoubleEquals => BinaryOp::Equals,
            TokenKind::NotEquals => BinaryOp::NotEquals,
            TokenKind::LessThan => BinaryOp::LessThan,
            TokenKind::LessThanOrEquals => BinaryOp::LessThanOrEquals,
            TokenKind::GreaterThan => BinaryOp::GreaterThan,
            TokenKind::GreaterThanOrEquals => BinaryOp::GreaterThanOrEquals,
            TokenKind::And => BinaryOp::And,
            TokenKind::Or => BinaryOp::Or,
            _ => unreachable!(),
        };

        self.expr(Expr::Binary {
            op,
            lhs,
            rhs,
            span: self.ast[lhs].span().merge(self.ast[rhs].span()),
        })
    }

    fn parse_block(&mut self) -> Id<Expr> {
        let open = self.expect(TokenKind::OpenBrace);

        let (stmts, value) = self.parse_inner_block();

        let close = self.expect(TokenKind::CloseBrace);
        let span = open.span.merge(close.span);

        self.expr(Expr::Block { span, stmts, value })
    }

    pub(crate) fn parse_inner_block(&mut self) -> (Vec<Id<Stmt>>, Option<Id<Expr>>) {
        let mut stmts = vec![];
        let mut value = None;

        while self.peek().kind != TokenKind::CloseBrace && self.peek().kind != TokenKind::Eof {
            match self.parse_stmt() {
                ExprOrStmt::Stmt(s) => stmts.push(s),
                ExprOrStmt::Expr(e) => {
                    value = Some(e);
                    break;
                }
            }
        }

        (stmts, value)
    }
}

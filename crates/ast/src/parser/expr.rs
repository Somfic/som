use som_common::Id;

use crate::{
    BinaryOp, Expr, Parser, UnaryOp,
    parser::rules::{InfixRule, PrefixRule, infix, prefix},
    token::TokenKind,
};

/// starts with
fn prefix_rule(token: TokenKind) -> Option<PrefixRule> {
    Some(match token {
        TokenKind::Int => prefix(parse_int_literal),
        TokenKind::True => prefix(parse_bool_literal),
        TokenKind::False => prefix(parse_bool_literal),
        TokenKind::OpenParen => prefix(parse_grouping),
        TokenKind::Minus => prefix(parse_unary),
        TokenKind::Bang => prefix(parse_unary),
        _ => return None,
    })
}

/// continues with
fn infix_rule(token: TokenKind) -> Option<InfixRule> {
    Some(match token {
        TokenKind::Plus => infix(parse_binary, 50, 51),
        TokenKind::Minus => infix(parse_binary, 50, 51),
        TokenKind::Star => infix(parse_binary, 60, 61),
        TokenKind::Slash => infix(parse_binary, 60, 61),
        TokenKind::DoubleEquals => infix(parse_binary, 30, 31),
        TokenKind::NotEquals => infix(parse_binary, 30, 31),
        TokenKind::LessThan => infix(parse_binary, 40, 41),
        TokenKind::LessThanOrEquals => infix(parse_binary, 40, 41),
        TokenKind::GreaterThan => infix(parse_binary, 40, 41),
        TokenKind::GreaterThanOrEquals => infix(parse_binary, 40, 41),
        TokenKind::And => infix(parse_binary, 20, 21),
        TokenKind::Or => infix(parse_binary, 10, 11),
        TokenKind::If => infix(parse_conditional, 5, 4),
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

fn parse_bool_literal(parser: &mut Parser) -> Id<Expr> {
    let token = parser.next();
    let value = match token.kind {
        TokenKind::True => true,
        TokenKind::False => false,
        _ => unreachable!(),
    };

    parser.expr(Expr::Bool {
        value,
        span: token.span,
    })
}

fn parse_grouping(parser: &mut Parser) -> Id<Expr> {
    parser.next();
    let expr = parser.parse_expr();
    parser.expect(TokenKind::CloseParen);

    expr
}

fn parse_conditional(parser: &mut Parser, truthy: Id<Expr>) -> Id<Expr> {
    parser.next();
    let condition = parser.parse_expr();
    parser.expect(TokenKind::Else);
    let falsy = parser.parse_expr_bp(4);

    parser.expr(Expr::Condition {
        condition,
        truthy,
        falsy,
        span: parser.ast[truthy].span().merge(parser.ast[falsy].span()),
    })
}

fn parse_unary(parser: &mut Parser) -> Id<Expr> {
    let token = parser.next();

    let op = match token.kind {
        TokenKind::Minus => UnaryOp::Negate,
        TokenKind::Bang => UnaryOp::Not,
        _ => unreachable!(),
    };

    let operand = parser.parse_expr_bp(70);

    parser.expr(Expr::Unary {
        op,
        operand,
        span: token.span.merge(parser.ast[operand].span()),
    })
}

fn parse_binary(parser: &mut Parser, lhs: Id<Expr>) -> Id<Expr> {
    let op = parser.next();
    let rule = infix_rule(op.kind).unwrap();
    let rhs = parser.parse_expr_bp(rule.r_bp);

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

    parser.expr(Expr::Binary {
        op,
        lhs,
        rhs,
        span: parser.ast[lhs].span().merge(parser.ast[rhs].span()),
    })
}

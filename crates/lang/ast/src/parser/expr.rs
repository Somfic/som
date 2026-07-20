use som_common::{Diagnostic, Id};

use crate::{
    BinaryOp, Expr, Parser, Stmt, UnaryOp,
    parser::rules::{InfixRule, PrefixRule, infix, prefix},
    token::TokenKind,
};

/// starts with
fn prefix_rule(token: TokenKind) -> Option<PrefixRule> {
    Some(match token {
        TokenKind::Int => prefix(|p| p.parse_int_literal()),
        TokenKind::Text => prefix(|p| p.parse_string_literal()),
        TokenKind::True => prefix(|p| p.parse_bool_literal()),
        TokenKind::False => prefix(|p| p.parse_bool_literal()),
        TokenKind::OpenParen => prefix(|p| p.parse_grouping()),
        TokenKind::Minus => prefix(|p| p.parse_unary()),
        TokenKind::Bang => prefix(|p| p.parse_unary()),
        TokenKind::OpenBrace => prefix(|p| p.parse_block()),
        TokenKind::Ident => prefix(|p| p.parse_variable()),
        _ => return None,
    })
}

/// continues with
fn infix_rule(token: TokenKind) -> Option<InfixRule> {
    Some(match token {
        // assignment binds loosest and is right-associative
        TokenKind::Equals => infix(|p, lhs| p.parse_assignment(lhs), 2, 1),
        TokenKind::PlusEquals => infix(|p, lhs| p.parse_assignment(lhs), 2, 1),
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
                self.diags.emit(
                    Diagnostic::error(token.span, format!("unexpected token `{}`", token.text))
                        .label("expected an expression here"),
                );
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

    fn parse_variable(&mut self) -> Id<Expr> {
        let token = self.next();
        self.expr(Expr::Variable {
            name: token.text,
            span: token.span,
        })
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

    fn parse_string_literal(&mut self) -> Id<Expr> {
        let token = self.next();
        let value = unescape(&token.text);
        self.expr(Expr::Str {
            value: value.into(),
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

    fn parse_assignment(&mut self, lhs: Id<Expr>) -> Id<Expr> {
        let op = self.next();

        let target = match &self.ast[lhs] {
            Expr::Variable { name, .. } => name.clone(),
            _ => {
                let span = self.ast[lhs].span();
                self.diags.emit(
                    Diagnostic::error(span, "cannot assign to this expression".to_string())
                        .label("expected a variable on the left of `=`"),
                );
                let _ = self.parse_expr_bp(1);
                return self.ast.add_expr(Expr::Error { span: op.span });
            }
        };

        let rhs = self.parse_expr_bp(1);

        // `x += e` desugars to `x = x + e`.
        let value = match op.kind {
            TokenKind::PlusEquals => {
                let span = self.ast[lhs].span().merge(self.ast[rhs].span());
                self.ast.add_expr(Expr::Binary {
                    lhs,
                    op: BinaryOp::Add,
                    rhs,
                    span,
                })
            }
            _ => rhs,
        };

        let span = self.ast[lhs].span().merge(self.ast[value].span());
        self.expr(Expr::Assignment {
            target,
            value,
            span,
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

        loop {
            self.skip_layout();
            if self.at_block_end() {
                break;
            }

            if self.peek().kind == TokenKind::Let {
                let stmt = self.parse_let();
                stmts.push(stmt);
                self.try_eat(TokenKind::Semicolon);
                continue;
            }

            let expr = self.parse_expr();

            // An explicit `;` always makes it a statement (value discarded).
            if self.try_eat(TokenKind::Semicolon).is_some() {
                let span = self.ast[expr].span();
                stmts.push(self.ast.add_stmt(Stmt::Expr { span, expr }));
                continue;
            }

            // Otherwise a trailing expr before the block ends is the value;
            // if more items follow, it was a newline-separated statement.
            self.skip_layout();
            if self.at_block_end() {
                value = Some(expr);
                break;
            }
            let span = self.ast[expr].span();
            stmts.push(self.ast.add_stmt(Stmt::Expr { span, expr }));
        }

        (stmts, value)
    }

    fn at_block_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::CloseBrace | TokenKind::Eof)
    }
}

/// Strip a string literal's surrounding quotes and resolve escape sequences.
fn unescape(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

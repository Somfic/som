use crate::ast::{BinOp, Expr, ExprId, FuncId, Stmt};
use crate::lexer::TokenKind;
use crate::parser::Parser;
use crate::span::Span;

impl<'src> Parser<'src> {
    /// Parse an expression using Pratt parsing
    pub(super) fn parse_expr(&mut self) -> Option<ExprId> {
        self.parse_expr_bp(0)
    }

    /// Pratt parser with binding power
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<ExprId> {
        let start_span = self.peek_span();

        // Parse prefix/atom
        let mut lhs = self.parse_atom()?;

        loop {
            // Check for postfix operators (function calls)
            if self.at(TokenKind::OpenParen) {
                lhs = self.parse_call(lhs, start_span.clone())?;
                continue;
            }

            // Check for infix operators
            let Some(op) = self.peek_binop() else { break };
            let (l_bp, r_bp) = op.binding_power();

            if l_bp < min_bp {
                break;
            }

            // Consume operator
            self.advance();

            // Parse right-hand side
            let rhs = self.parse_expr_bp(r_bp)?;

            let end_span = self.previous_span();
            let span = start_span.merge(&end_span);

            lhs = self
                .ast
                .alloc_expr_with_span(Expr::Binary { op, lhs, rhs }, span);
        }

        Some(lhs)
    }

    fn parse_atom(&mut self) -> Option<ExprId> {
        let start_span = self.peek_span();

        match self.peek() {
            TokenKind::Int => {
                let token = self.peek_token();
                let value: i32 = token.text.parse().unwrap_or(0);
                let span = token.span.clone();
                self.advance();
                Some(self.ast.alloc_expr_with_span(Expr::I32(value), span))
            }
            TokenKind::Ident => {
                let token = self.peek_token();
                let text = token.text;
                let span = token.span.clone();
                self.advance();
                let ident = self.make_ident(text);
                Some(self.ast.alloc_expr_with_span(Expr::Var(ident), span))
            }
            TokenKind::True => {
                let token = self.peek_token();
                let span = token.span.clone();
                self.advance();
                Some(self.ast.alloc_expr_with_span(Expr::Bool(true), span))
            }
            TokenKind::False => {
                let token = self.peek_token();
                let span = token.span.clone();
                self.advance();
                Some(self.ast.alloc_expr_with_span(Expr::Bool(false), span))
            }
            TokenKind::Text => {
                let token = self.peek_token();
                let text = token.text;
                let span = token.span.clone();
                self.advance();
                // Remove surrounding quotes
                let unquoted = &text[1..text.len() - 1];
                Some(
                    self.ast
                        .alloc_expr_with_span(Expr::String(unquoted.into()), span),
                )
            }
            TokenKind::OpenParen => {
                self.advance(); // consume (
                let inner = self.parse_expr()?;
                self.expect(TokenKind::CloseParen)?;
                Some(inner)
            }
            TokenKind::OpenBrace => self.parse_block(),
            TokenKind::Ampersand => {
                self.advance(); // consume &
                let mutable = self.eat(TokenKind::Mut);
                let expr = self.parse_expr_bp(15)?;
                let end_span = self.previous_span();
                let span = start_span.merge(&end_span);
                Some(
                    self.ast
                        .alloc_expr_with_span(Expr::Borrow { mutable, expr }, span),
                )
            }
            TokenKind::Star => {
                self.advance(); // consume *
                let expr = self.parse_expr_bp(15)?;
                let end_span = self.previous_span();
                let span = start_span.merge(&end_span);
                Some(self.ast.alloc_expr_with_span(Expr::Deref { expr }, span))
            }
            _ => {
                self.error(vec![
                    TokenKind::Int,
                    TokenKind::Ident,
                    TokenKind::OpenParen,
                    TokenKind::OpenBrace,
                    TokenKind::Ampersand,
                    TokenKind::Star,
                ]);
                // Return a hole expression for error recovery
                Some(self.ast.alloc_expr_with_span(Expr::Hole, start_span))
            }
        }
    }

    fn parse_call(&mut self, callee: ExprId, start_span: Span) -> Option<ExprId> {
        self.expect(TokenKind::OpenParen)?;

        let mut args = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            loop {
                if let Some(arg) = self.parse_expr() {
                    args.push(arg);
                }

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::CloseParen)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        // For now, we need to resolve the function name to a FuncId
        // This is a simplified version - in a real compiler, this would be done
        // in a name resolution pass
        let callee_expr = self.ast.get_expr(&callee);
        let func_id = match callee_expr {
            Expr::Var(ident) => {
                // Look up function by name
                self.find_func_by_name(&ident.value)
            }
            _ => None,
        };

        if let Some(func_id) = func_id {
            Some(self.ast.alloc_expr_with_span(
                Expr::Call {
                    func: func_id,
                    args,
                },
                span,
            ))
        } else {
            // Function not found - emit error and return hole
            self.errors.push(crate::parser::ParseError::new(
                vec![],
                TokenKind::Ident,
                start_span,
            ));
            Some(self.ast.alloc_expr_with_span(Expr::Hole, span))
        }
    }

    fn find_func_by_name(&self, name: &str) -> Option<FuncId> {
        for (idx, func) in self.ast.funcs.iter().enumerate() {
            if &*func.name.value == name {
                return Some(FuncId(idx as u32));
            }
        }
        None
    }

    pub(super) fn parse_block(&mut self) -> Option<ExprId> {
        let start_span = self.peek_span();

        self.expect(TokenKind::OpenBrace)?;

        let mut stmts = Vec::new();
        let mut value = None;

        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Let) {
                // Let statement
                if let Some(stmt_id) = self.parse_let_stmt() {
                    stmts.push(stmt_id);
                }
            } else {
                // Expression (possibly followed by semicolon)
                if let Some(expr_id) = self.parse_expr() {
                    if self.eat(TokenKind::Semicolon) {
                        // Expression statement
                        let stmt_id = self.ast.alloc_stmt(Stmt::Expr { expr: expr_id });
                        stmts.push(stmt_id);
                    } else if self.at(TokenKind::CloseBrace) {
                        // Last expression without semicolon - this is the block's value
                        value = Some(expr_id);
                    } else {
                        // Missing semicolon between statements
                        self.error(vec![TokenKind::Semicolon, TokenKind::CloseBrace]);
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        Some(
            self.ast
                .alloc_expr_with_span(Expr::Block { stmts, value }, span),
        )
    }

    fn peek_binop(&self) -> Option<BinOp> {
        match self.peek() {
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Subtract),
            TokenKind::Star => Some(BinOp::Multiply),
            TokenKind::Slash => Some(BinOp::Divide),
            TokenKind::DoubleEquals => Some(BinOp::Equals),
            TokenKind::NotEquals => Some(BinOp::NotEquals),
            TokenKind::LessThan => Some(BinOp::LessThan),
            TokenKind::GreaterThan => Some(BinOp::GreaterThan),
            TokenKind::LessThanOrEqual => Some(BinOp::LessThan), // TODO: Add LessThanOrEqual
            TokenKind::GreaterThanOnEqual => Some(BinOp::GreaterThan), // TODO: Add GreaterThanOrEqual
            _ => None,
        }
    }
}

impl BinOp {
    /// Returns (left binding power, right binding power)
    fn binding_power(&self) -> (u8, u8) {
        match self {
            BinOp::Or => (1, 2),
            BinOp::And => (3, 4),
            BinOp::Equals | BinOp::NotEquals => (5, 6),
            BinOp::LessThan | BinOp::GreaterThan => (7, 8),
            BinOp::Add | BinOp::Subtract => (9, 10),
            BinOp::Multiply | BinOp::Divide => (11, 12),
        }
    }
}

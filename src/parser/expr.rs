use crate::{
    Expr, Ident,
    arena::Id,
    lexer::TokenKind,
    parser::{Parser, RecoveryLevel, StmtOrExpr, grammar::Grammar},
};

impl<'src> Parser<'src> {
    /// Parse an expression
    pub fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    /// Pratt parser with binding power
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
        let start = self.current_span();

        // Parse prefix or atom
        let mut lhs = self.parse_prefix_or_atom()?;

        loop {
            // Check for field access (highest precedence postfix)
            if self.at(TokenKind::Dot) {
                let postfix_bp = Grammar::POSTFIX * 2;
                if postfix_bp < min_bp {
                    break;
                }
                self.advance(); // consume '.'
                let field = self.parse_ident()?;
                let span = start.merge(&self.previous_span());
                lhs = self.builder.alloc_expr(Expr::FieldAccess { object: lhs, field }, span);
                continue;
            }

            // Check for function call
            if self.at(TokenKind::OpenParen) {
                let postfix_bp = Grammar::POSTFIX * 2;
                if postfix_bp < min_bp {
                    break;
                }
                lhs = self.parse_call(lhs, start.clone())?;
                continue;
            }

            // Check for conditional expression (value if condition else other)
            if self.at(TokenKind::If) {
                let if_bp = 1; // Very low binding power
                if if_bp < min_bp {
                    break;
                }
                lhs = self.parse_conditional(lhs, start.clone())?;
                continue;
            }

            // Check for infix operators
            let Some((op, info)) = Grammar::infix_op(self.peek()) else {
                break;
            };

            let (lbp, rbp) = info.binding_power();
            if lbp < min_bp {
                break;
            }

            self.advance(); // Consume operator
            let rhs = self.parse_expr_bp(rbp)?;

            let span = start.merge(&self.previous_span());
            lhs = self.builder.alloc_expr(Expr::Binary { op, lhs, rhs }, span);
        }

        Some(lhs)
    }

    fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        match self.peek() {
            TokenKind::Bang => {
                self.advance();
                let bp = Grammar::prefix_bp(TokenKind::Bang).unwrap();
                let expr = self.parse_expr_bp(bp)?;
                let span = start.merge(&self.previous_span());
                Some(self.builder.alloc_expr(Expr::Not { expr }, span))
            }

            TokenKind::Ampersand => {
                self.advance();
                let mutable = self.eat(TokenKind::Mut);
                let bp = Grammar::prefix_bp(TokenKind::Ampersand).unwrap();
                let expr = self.parse_expr_bp(bp)?;
                let span = start.merge(&self.previous_span());
                Some(
                    self.builder
                        .alloc_expr(Expr::Borrow { mutable, expr }, span),
                )
            }

            TokenKind::Star => {
                self.advance();
                let bp = Grammar::prefix_bp(TokenKind::Star).unwrap();
                let expr = self.parse_expr_bp(bp)?;
                let span = start.merge(&self.previous_span());
                Some(self.builder.alloc_expr(Expr::Deref { expr }, span))
            }

            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        match self.peek() {
            TokenKind::Int => {
                let value: i32 = self.peek_token().text.parse().unwrap_or(0);
                self.advance();
                Some(self.builder.alloc_expr(Expr::I32(value), start))
            }

            TokenKind::Float => {
                let value: f32 = self.peek_token().text.parse().unwrap_or(0.0);
                self.advance();
                Some(self.builder.alloc_expr(Expr::F32(value), start))
            }

            TokenKind::True => {
                self.advance();
                Some(self.builder.alloc_expr(Expr::Bool(true), start))
            }

            TokenKind::False => {
                self.advance();
                Some(self.builder.alloc_expr(Expr::Bool(false), start))
            }

            TokenKind::Text => {
                let text = self.peek_token().text;
                // Remove surrounding quotes
                let unquoted = &text[1..text.len() - 1];
                self.advance();
                Some(
                    self.builder
                        .alloc_expr(Expr::String(unquoted.into()), start),
                )
            }

            TokenKind::Ident => {
                // Check if this is a constructor: `Name { ... }`
                if self.peek_next() == TokenKind::OpenBrace {
                    self.parse_constructor()
                } else {
                    let name = self.parse_ident()?;
                    Some(self.builder.alloc_expr(Expr::Var(name), start))
                }
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(TokenKind::CloseParen)?;
                Some(inner)
            }

            TokenKind::OpenBrace => self.parse_block(),

            _ => {
                self.error("expected expression".into());
                // Return a hole expression for error recovery
                Some(self.builder.alloc_hole(start))
            }
        }
    }

    /// Parse a conditional expression: `value if condition else other`
    fn parse_conditional(&mut self, truthy: Id<Expr>, start: crate::Span) -> Option<Id<Expr>> {
        self.expect(TokenKind::If)?;

        let condition = self.parse_expr()?;
        self.expect(TokenKind::Else)?;
        let falsy = self.parse_expr()?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_expr(
            Expr::Conditional {
                condition,
                truthy,
                falsy,
            },
            span,
        ))
    }

    /// Parse a function call
    fn parse_call(&mut self, callee: Id<Expr>, start: crate::Span) -> Option<Id<Expr>> {
        self.expect(TokenKind::OpenParen)?;

        let mut args = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            args.push(self.parse_expr()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break; // Trailing comma
                }
                args.push(self.parse_expr()?);
            }
        }

        self.expect(TokenKind::CloseParen)?;

        let span = start.merge(&self.previous_span());

        // Extract function name from callee expression
        let callee_expr = self.builder.ast.exprs.get(&callee);
        match callee_expr {
            Expr::Var(ident) => Some(self.builder.alloc_expr(
                Expr::Call {
                    name: ident.clone(),
                    args,
                },
                span,
            )),
            _ => {
                self.error("expected function name".into());
                Some(self.builder.alloc_hole(span))
            }
        }
    }

    /// Parse a block expression: `{ stmts; value }`
    pub fn parse_block(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::OpenBrace)?;

        let mut stmts = Vec::new();
        let mut value = None;

        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) => {
                    if self.at(TokenKind::CloseBrace) {
                        // Last expression without semicolon - this is the block's value
                        value = Some(expr);
                    } else {
                        self.error("expected `;` or `}`".into());
                        break;
                    }
                }
                StmtOrExpr::Error => {
                    self.recover(RecoveryLevel::Statement);
                }
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_expr(Expr::Block { stmts, value }, span))
    }

    pub fn parse_constructor(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        let struct_name = self.parse_ident()?;

        self.expect(TokenKind::OpenBrace)?;
        let mut fields = Vec::new();

        if !self.at(TokenKind::CloseBrace) {
            fields.push(self.parse_constructor_field()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseBrace) {
                    break; // Trailing comma
                }
                fields.push(self.parse_constructor_field()?);
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_expr(
            Expr::Constructor {
                struct_name,
                fields,
            },
            span,
        ))
    }

    fn parse_constructor_field(&mut self) -> Option<(Ident, Id<Expr>)> {
        let name = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let value = self.parse_expr()?;
        Some((name, value))
    }
}

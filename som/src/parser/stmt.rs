use crate::{
    Stmt,
    arena::Id,
    lexer::TokenKind,
    parser::{Parser, RecoveryLevel, StmtOrExpr},
};

impl Parser<'_> {
    /// Parse either a statement or an expression
    pub fn parse_stmt_or_expr(&mut self) -> StmtOrExpr {
        // Statement keywords take precedence
        match self.peek() {
            TokenKind::Let => match self.parse_let_stmt() {
                Some(stmt) => StmtOrExpr::Stmt(stmt),
                None => StmtOrExpr::Error,
            },
            TokenKind::Loop => match self.parse_loop() {
                Some(stmt) => StmtOrExpr::Stmt(stmt),
                None => StmtOrExpr::Error,
            },
            TokenKind::While => match self.parse_while() {
                Some(stmt) => StmtOrExpr::Stmt(stmt),
                None => StmtOrExpr::Error,
            },
            TokenKind::If => match self.parse_if_stmt() {
                Some(stmt) => StmtOrExpr::Stmt(stmt),
                None => StmtOrExpr::Error,
            },
            _ => {
                // Try expression
                match self.parse_expr() {
                    Some(expr) => {
                        if self.eat(TokenKind::Semicolon) {
                            let span = self.builder.get_expr_span(&expr);
                            let stmt = self.builder.alloc_stmt(Stmt::Expr { expr }, span);
                            StmtOrExpr::Stmt(stmt)
                        } else {
                            StmtOrExpr::Expr(expr)
                        }
                    }
                    None => StmtOrExpr::Error,
                }
            }
        }
    }

    fn parse_let_stmt(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Let, "a variable declaration")?;

        let mutable = self.eat(TokenKind::Mut);
        let name = self.parse_ident("a variable name")?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Equals, "a variable value")?;
        let value = match self.parse_expr() {
            Some(expr) => expr,
            None => {
                self.error_missing("missing expression", "an expression after `=`");
                return None;
            }
        };
        self.expect(
            TokenKind::Semicolon,
            "a semicolon at the end of the variable declaration",
        )?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_stmt(
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            },
            span,
        ))
    }

    fn parse_loop(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Loop, "a loop statement")?;
        self.expect(TokenKind::OpenBrace, "an opening brace for the loop body")?;

        let body = self.parse_stmt_list()?;

        self.expect(TokenKind::CloseBrace, "a closing brace for the loop body")?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_stmt(Stmt::Loop { body }, span))
    }

    fn parse_while(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::While, "a while statement")?;

        let condition = self.parse_expr()?;
        self.expect(TokenKind::OpenBrace, "an opening brace for the while body")?;

        let body = self.parse_stmt_list()?;

        self.expect(TokenKind::CloseBrace, "a closing brace for the while body")?;

        let span = start.merge(&self.previous_span());
        Some(
            self.builder
                .alloc_stmt(Stmt::While { condition, body }, span),
        )
    }

    fn parse_if_stmt(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::If, "an if statement")?;

        let condition = self.parse_expr()?;
        self.expect(TokenKind::OpenBrace, "an opening brace for the if body")?;

        let then_body = self.parse_stmt_list()?;

        self.expect(TokenKind::CloseBrace, "a closing brace for the if body")?;

        let else_body = if self.eat(TokenKind::Else) {
            if self.at(TokenKind::If) {
                // else if - parse as a single If statement in a vec
                let else_if = self.parse_if_stmt()?;
                Some(vec![else_if])
            } else {
                self.expect(TokenKind::OpenBrace, "an opening brace for the else body")?;
                let body = self.parse_stmt_list()?;
                self.expect(TokenKind::CloseBrace, "a closing brace for the else body")?;
                Some(body)
            }
        } else {
            None
        };

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_stmt(
            Stmt::Condition {
                condition,
                then_body,
                else_body,
            },
            span,
        ))
    }

    /// Parse a list of statements (for loop/while bodies)
    pub fn parse_stmt_list(&mut self) -> Option<Vec<Id<Stmt>>> {
        let mut stmts = Vec::new();

        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) => {
                    // Expression without semicolon at end of loop body
                    if self.at(TokenKind::CloseBrace) {
                        // Wrap as expression statement
                        let span = self.builder.get_expr_span(&expr);
                        let stmt = self.builder.alloc_stmt(Stmt::Expr { expr }, span);
                        stmts.push(stmt);
                    } else {
                        self.error_missing("expected `;` or `}`", "a semicolon or closing brace");
                        break;
                    }
                }
                StmtOrExpr::Error => {
                    self.recover(RecoveryLevel::Statement);
                }
            }
        }

        Some(stmts)
    }
}

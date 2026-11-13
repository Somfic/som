use som_ast::{Id, Stmt};
use som_lexer::TokenKind;

use crate::{Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn parse_stmt_or_expr(&mut self) -> StmtOrExpr {
        let stmt = match self.peek() {
            TokenKind::Let => self.parse_let(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::While => self.parse_while(),
            TokenKind::Break => self.parse_break(),
            TokenKind::If => self.parse_if(),
            _ => {
                let Some(expr) = self.parse_expr() else {
                    return StmtOrExpr::Error;
                };
                if !self.eat(TokenKind::Semicolon) {
                    return StmtOrExpr::Expr(expr);
                }
                let span = self.ast.exprs.span(expr).clone();
                Some(self.ast.stmts.alloc(Stmt::Expr(expr), span))
            }
        };
        match stmt {
            Some(stmt) => StmtOrExpr::Stmt(stmt),
            None => StmtOrExpr::Error,
        }
    }

    fn parse_let(&mut self) -> Option<Id<Stmt>> {
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
        let Some(value) = self.parse_expr() else {
            self.error_missing("missing expression", "an expression after `=`");
            return None;
        };
        // Without its `;` (still being typed), it's reported but kept: what's known about
        // it, like its type, stays available to an editor.
        if !self.eat(TokenKind::Semicolon) {
            self.error_expected(
                &[TokenKind::Semicolon],
                "a semicolon at the end of the variable declaration",
            );
        }

        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(
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
        let body = self.parse_stmt_block("the loop body")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::Loop { body }, span))
    }

    fn parse_break(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Break, "a break statement")?;
        self.expect(TokenKind::Semicolon, "a semicolon after break")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::Break, span))
    }

    fn parse_while(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::While, "a while statement")?;
        let condition = self.parse_condition()?;
        let body = self.parse_stmt_block("the while body")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::While { condition, body }, span))
    }

    fn parse_if(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::If, "an if statement")?;
        if self.eat(TokenKind::Let) {
            return self.parse_if_let(start);
        }
        let condition = self.parse_condition()?;
        let then = self.parse_stmt_block("the if body")?;
        let otherwise = self.parse_else()?;

        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(
            Stmt::If {
                condition,
                then,
                otherwise,
            },
            span,
        ))
    }

    /// `if let name = optional { … } else { … }`, after the `if let`.
    fn parse_if_let(&mut self, start: som_ast::Span) -> Option<Id<Stmt>> {
        let name = self.parse_ident("a name for the optional's value")?;
        self.expect(TokenKind::Equals, "an optional to unwrap")?;
        let value = self.parse_condition()?;
        let then = self.parse_stmt_block("the if body")?;
        let otherwise = self.parse_else()?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(
            Stmt::IfLet {
                name,
                value,
                then,
                otherwise,
            },
            span,
        ))
    }

    /// An optional `else { … }` or `else if …`.
    fn parse_else(&mut self) -> Option<Option<Vec<Id<Stmt>>>> {
        Some(if !self.eat(TokenKind::Else) {
            None
        } else if self.at(TokenKind::If) {
            Some(vec![self.parse_if()?])
        } else {
            Some(self.parse_stmt_block("the else body")?)
        })
    }

    /// `{ stmt; stmt; expr }` where a trailing expression becomes an expression statement.
    fn parse_stmt_block(&mut self, what: &str) -> Option<Vec<Id<Stmt>>> {
        self.expect(TokenKind::OpenBrace, format!("an opening brace for {what}"))?;

        let mut stmts = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) if self.at(TokenKind::CloseBrace) => {
                    let span = self.ast.exprs.span(expr).clone();
                    stmts.push(self.ast.stmts.alloc(Stmt::Expr(expr), span));
                }
                StmtOrExpr::Expr(_) => {
                    self.error_missing("expected `;` or `}`", "a semicolon or closing brace");
                    break;
                }
                StmtOrExpr::Error => self.recover(RecoveryLevel::Statement),
            }
        }

        self.expect(TokenKind::CloseBrace, format!("a closing brace for {what}"))?;
        Some(stmts)
    }
}

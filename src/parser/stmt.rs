use crate::arena::Id;
use crate::ast::Stmt;
use crate::lexer::TokenKind;
use crate::parser::Parser;

impl<'src> Parser<'src> {
    pub fn parse_loop(&mut self) -> Option<Id<Stmt>> {
        let start_span = self.peek_span();

        // loop
        self.expect(TokenKind::Loop)?;

        self.expect(TokenKind::OpenBrace)?;

        // body
        let mut body = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Let) {
                if let Some(stmt_id) = self.parse_let_stmt() {
                    body.push(stmt_id);
                }
            } else if self.at(TokenKind::Loop) {
                if let Some(stmt_id) = self.parse_loop() {
                    body.push(stmt_id);
                }
            } else if self.at(TokenKind::While) {
                if let Some(stmt_id) = self.parse_while() {
                    body.push(stmt_id);
                }
            } else if let Some(expr_id) = self.parse_expr() {
                // Expression statement - requires semicolon
                self.expect(TokenKind::Semicolon)?;
                let stmt_id = self.ast.stmts.alloc(Stmt::Expr { expr: expr_id });
                body.push(stmt_id);
            } else {
                // Skip to recover from error
                self.advance();
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        Some(self.ast.alloc_stmt_with_span(Stmt::Loop { body }, span))
    }

    pub fn parse_while(&mut self) -> Option<Id<Stmt>> {
        let start_span = self.peek_span();

        // while
        self.expect(TokenKind::While)?;

        // condition
        let condition = self.parse_expr()?;

        self.expect(TokenKind::OpenBrace)?;

        // body
        let mut body = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Let) {
                if let Some(stmt_id) = self.parse_let_stmt() {
                    body.push(stmt_id);
                }
            } else if self.at(TokenKind::Loop) {
                if let Some(stmt_id) = self.parse_loop() {
                    body.push(stmt_id);
                }
            } else if self.at(TokenKind::While) {
                if let Some(stmt_id) = self.parse_while() {
                    body.push(stmt_id);
                }
            } else if let Some(expr_id) = self.parse_expr() {
                // Expression statement - requires semicolon
                self.expect(TokenKind::Semicolon)?;
                let stmt_id = self.ast.stmts.alloc(Stmt::Expr { expr: expr_id });
                body.push(stmt_id);
            } else {
                // Skip to recover from error
                self.advance();
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        Some(self.ast.alloc_stmt_with_span(Stmt::While { condition, body }, span))
    }

    pub(super) fn parse_let_stmt(&mut self) -> Option<Id<Stmt>> {
        let start_span = self.peek_span();

        // let
        self.expect(TokenKind::Let)?;

        // optional mut
        let mutable = self.eat(TokenKind::Mut);

        // name
        let (name, _) = self.parse_ident()?;

        // Optional type annotation
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // = value
        self.expect(TokenKind::Equals)?;
        let value = self.parse_expr()?;

        // ;
        self.expect(TokenKind::Semicolon)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        Some(self.ast.alloc_stmt_with_span(
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            },
            span,
        ))
    }
}

use crate::lexer::Syntax;
use crate::parser::{union, Parser};
use std::collections::HashSet;
use std::ops::ControlFlow;

impl<'a> Parser<'a> {
    pub(super) fn expr(&mut self, anchor: HashSet<Syntax>) -> ControlFlow<()> {
        self.binary_expr(anchor, 0)
    }

    // Simple left-to-right binary expression parsing
    fn binary_expr(&mut self, anchor: HashSet<Syntax>, _min_prec: u8) -> ControlFlow<()> {
        // Save checkpoint BEFORE parsing anything
        let mut checkpoint = self.builder.checkpoint();

        // Parse left side
        self.call_expr(anchor.clone())?;

        // Parse binary operators (left-to-right, no precedence for now)
        while precedence(self.input.peek()) > 0 {
            // Consume operator
            self.bump();

            // Parse right side
            self.call_expr(anchor.clone())?;

            // Wrap everything from checkpoint in binary expression
            self.builder
                .start_node_at(checkpoint, Syntax::BinaryExpr.into());
            self.builder.finish_node();

            // Update checkpoint for next iteration (to handle a + b + c)
            checkpoint = self.builder.checkpoint();
        }

        ControlFlow::Continue(())
    }

    fn call_expr(&mut self, anchor: HashSet<Syntax>) -> ControlFlow<()> {
        let checkpoint = self.builder.checkpoint();

        self.atom(anchor.clone())?;

        // Check for function call
        if self.input.at(Syntax::LeftParen) {
            self.builder
                .start_node_at(checkpoint, Syntax::CallExpr.into());

            self.bump(); // consume (

            // Parse arguments
            if !self.input.at(Syntax::RightParen) {
                loop {
                    self.expr(union(&anchor, [Syntax::Comma, Syntax::RightParen]))?;

                    if self.eat(Syntax::Comma).is_none() {
                        break;
                    }
                }
            }

            self.expect(Syntax::RightParen, anchor);

            self.builder.finish_node();
        }

        ControlFlow::Continue(())
    }

    fn atom(&mut self, anchor: HashSet<Syntax>) -> ControlFlow<()> {
        match self.input.peek() {
            Syntax::Ident => {
                self.with(Syntax::VarExpr, |this| {
                    this.bump();
                });
                ControlFlow::Continue(())
            }
            Syntax::Int => {
                self.with(Syntax::IntExpr, |this| {
                    this.bump();
                });
                ControlFlow::Continue(())
            }
            Syntax::LeftParen => {
                self.with(Syntax::ParenExpr, |this| {
                    this.bump();
                    let _ = this.expr(union(&anchor, [Syntax::RightParen]));
                    this.expect(Syntax::RightParen, anchor);
                });
                ControlFlow::Continue(())
            }
            Syntax::LeftBrace => {
                self.block(anchor);
                ControlFlow::Continue(())
            }
            _ => {
                if !self.in_error {
                    let pos = self.input.current_pos();
                    let span = crate::span::Span::new(pos as u32, pos as u32);
                    self.errors.push(crate::parser::error::ParseError::new(
                        vec![
                            Syntax::Ident,
                            Syntax::Int,
                            Syntax::LeftParen,
                            Syntax::LeftBrace,
                        ],
                        self.input.peek(),
                        span,
                    ));
                    self.in_error = true;
                }
                ControlFlow::Break(())
            }
        }
    }

    pub(super) fn expr_stmt(&mut self, anchor: HashSet<Syntax>) {
        self.with(Syntax::ExprStmt, |this| {
            let _ = this.expr(anchor);
        });
    }
}

fn precedence(op: Syntax) -> u8 {
    match op {
        Syntax::EqEq | Syntax::NotEq => 1,
        Syntax::Lt | Syntax::Gt | Syntax::LtEq | Syntax::GtEq => 2,
        Syntax::Plus | Syntax::Minus => 3,
        Syntax::Star | Syntax::Slash => 4,
        _ => 0,
    }
}

use som_ast::{Expr, Id, Stmt};
use som_base::Span;
use som_lexer::TokenKind;

use crate::{Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn parse_stmt_or_expr(&mut self) -> StmtOrExpr {
        let stmt = match self.peek() {
            // `use som::data;`, for the rest of the block.
            TokenKind::Use => {
                let start = self.current_span();
                let Some(u) = self.parse_use() else {
                    return StmtOrExpr::Error;
                };
                self.ast.modules[self.module].block_uses.push(u.path.clone());
                let span = self.span_from(&start);
                Some(self.ast.stmts.alloc(Stmt::Use(u), span))
            }
            TokenKind::Let => self.parse_let(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Return => self.parse_return(),
            // An `if` ending a block, each branch ending in a value, gives that value.
            TokenKind::If => {
                let Some(parts) = self.parse_if_parts() else {
                    return StmtOrExpr::Error;
                };
                let ends_block = self.at(TokenKind::CloseBrace) || self.at_eof();
                if ends_block && parts.gives_value() {
                    return StmtOrExpr::Expr(self.if_expr(parts));
                }
                Some(self.if_stmt(parts))
            }
            // Like `if`, a `match` or block that starts a statement ends at its brace: no `;`
            // needed.
            TokenKind::Match | TokenKind::OpenBrace => {
                let expr = match self.peek() {
                    TokenKind::Match => self.parse_match(),
                    _ => self.parse_block(),
                };
                let Some(expr) = expr else {
                    return StmtOrExpr::Error;
                };
                let ends_block = self.at(TokenKind::CloseBrace) || self.at_eof();
                if !self.eat(TokenKind::Semicolon) && ends_block {
                    return StmtOrExpr::Expr(expr);
                }
                let span = self.ast.exprs.span(expr).clone();
                Some(self.ast.stmts.alloc(Stmt::Expr(expr), span))
            }
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
        let target = self.parse_loop_target()?;
        self.expect(TokenKind::Semicolon, "a semicolon after break")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::Break(target), span))
    }

    /// `return value;` or `return;`
    fn parse_return(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Return, "a return statement")?;
        let value = if self.at(TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        // Without its `;` (still being typed), it's reported but kept, like a `let`: an editor
        // still knows what's returned, `self.` while typing a field.
        if !self.eat(TokenKind::Semicolon) {
            self.error_expected(&[TokenKind::Semicolon], "a semicolon after return");
        }
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::Return(value), span))
    }

    fn parse_continue(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Continue, "a continue statement")?;
        let target = self.parse_loop_target()?;
        self.expect(TokenKind::Semicolon, "a semicolon after continue")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::Continue(target), span))
    }

    /// The `x` in `break x;` or `continue x;`: which `for` loop, by its variable.
    fn parse_loop_target(&mut self) -> Option<Option<som_ast::Ident>> {
        if !self.at(TokenKind::Ident) {
            return Some(None);
        }
        Some(Some(self.parse_ident("which loop")?))
    }

    fn parse_while(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::While, "a while statement")?;
        let condition = self.parse_condition()?;
        let body = self.parse_stmt_block("the while body")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(Stmt::While { condition, body }, span))
    }

    /// `for name in iterable { body }`
    fn parse_for(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::For, "a for loop")?;
        let mutable = self.eat(TokenKind::Mut);
        let name = self.parse_ident("a name for each value")?;
        self.expect(TokenKind::In, "`in` and what to loop over")?;
        let iterable = self.parse_condition()?;
        let body = self.parse_stmt_block("the for body")?;
        let span = self.span_from(&start);
        Some(self.ast.stmts.alloc(
            Stmt::For {
                name,
                mutable,
                iterable,
                body,
            },
            span,
        ))
    }

    /// `if condition { … } else …`, before it's known whether it's a statement or gives a
    /// value.
    pub(crate) fn parse_if_parts(&mut self) -> Option<IfParts> {
        let start = self.current_span();
        self.expect(TokenKind::If, "an if")?;
        let condition = self.parse_condition()?;
        let then = self.parse_block_parts("the if body")?;
        let otherwise = if !self.eat(TokenKind::Else) {
            None
        } else if self.at(TokenKind::If) {
            Some(Else::If(Box::new(self.parse_if_parts()?)))
        } else {
            Some(Else::Block(self.parse_block_parts("the else body")?))
        };
        Some(IfParts {
            condition,
            then,
            otherwise,
            span: self.span_from(&start),
        })
    }

    /// An `if` as a statement: values its branches end in are statements too.
    fn if_stmt(&mut self, parts: IfParts) -> Id<Stmt> {
        let then = self.block_stmts(parts.then);
        let otherwise = parts.otherwise.map(|o| match o {
            Else::Block(block) => self.block_stmts(block),
            Else::If(inner) => vec![self.if_stmt(*inner)],
        });
        self.ast.stmts.alloc(
            Stmt::If {
                condition: parts.condition,
                then,
                otherwise,
            },
            parts.span,
        )
    }

    /// An `if` that gives a value: `then if condition else otherwise`, with blocks.
    pub(crate) fn if_expr(&mut self, parts: IfParts) -> Id<Expr> {
        let then = self.block_expr(parts.then);
        let otherwise = match parts.otherwise {
            Some(Else::Block(block)) => self.block_expr(block),
            Some(Else::If(inner)) => self.if_expr(*inner),
            None => {
                let end = parts.span.after();
                self.error_at(
                    end.clone(),
                    "an `if` that gives a value needs an `else`",
                    "the value when the condition is false goes here",
                );
                self.ast.exprs.alloc(Expr::Hole, end)
            }
        };
        self.ast.exprs.alloc(
            Expr::Conditional {
                condition: parts.condition,
                then,
                otherwise,
            },
            parts.span,
        )
    }

    fn block_stmts(&mut self, block: BlockParts) -> Vec<Id<Stmt>> {
        let mut stmts = block.stmts;
        if let Some(value) = block.value {
            let span = self.ast.exprs.span(value).clone();
            stmts.push(self.ast.stmts.alloc(Stmt::Expr(value), span));
        }
        stmts
    }

    pub(crate) fn block_expr(&mut self, block: BlockParts) -> Id<Expr> {
        let expr = Expr::Block {
            stmts: block.stmts,
            value: block.value,
        };
        self.ast.exprs.alloc(expr, block.span)
    }

    /// After an expression missing its `;` (reported already), whether to go on parsing the
    /// statements after it, keeping it as one: so `a.` while typing, with more below, is still
    /// there for an editor, and so is what follows. Only if the expression got somewhere since
    /// `before`, so parsing can't get stuck.
    pub(crate) fn keep_unterminated(
        &mut self,
        expr: som_ast::Id<som_ast::Expr>,
        before: usize,
        stmts: &mut Vec<Id<Stmt>>,
    ) -> bool {
        if self.pos == before {
            return false;
        }
        let span = self.ast.exprs.span(expr).clone();
        stmts.push(self.ast.stmts.alloc(Stmt::Expr(expr), span));
        true
    }

    /// `{ stmt; stmt; expr }` where a trailing expression becomes an expression statement.
    fn parse_stmt_block(&mut self, what: &str) -> Option<Vec<Id<Stmt>>> {
        let block = self.parse_block_parts(what)?;
        Some(self.block_stmts(block))
    }

    /// `{ stmt; stmt; value }`: the statements, and the value it ends with if it does.
    pub(crate) fn parse_block_parts(&mut self, what: &str) -> Option<BlockParts> {
        let start = self.current_span();
        self.expect(TokenKind::OpenBrace, format!("an opening brace for {what}"))?;

        let mut stmts = Vec::new();
        let mut value = None;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            let before = self.pos;
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) if self.at(TokenKind::CloseBrace) => value = Some(expr),
                StmtOrExpr::Expr(expr) => {
                    self.error_missing("expected `;` or `}`", "a semicolon or closing brace");
                    if !self.keep_unterminated(expr, before, &mut stmts) {
                        break;
                    }
                }
                StmtOrExpr::Error => self.recover(RecoveryLevel::Statement),
            }
        }

        self.expect(TokenKind::CloseBrace, format!("a closing brace for {what}"))?;
        Some(BlockParts {
            stmts,
            value,
            span: self.span_from(&start),
        })
    }
}

/// A block's statements, the value it ends with if it does, and where it is.
pub(crate) struct BlockParts {
    stmts: Vec<Id<Stmt>>,
    value: Option<Id<Expr>>,
    span: Span,
}

/// An `if`, parsed before it's known whether it's a statement or gives a value.
pub(crate) struct IfParts {
    condition: Id<Expr>,
    then: BlockParts,
    otherwise: Option<Else>,
    span: Span,
}

enum Else {
    Block(BlockParts),
    If(Box<IfParts>),
}

impl IfParts {
    /// Whether every branch ends in a value, and there's an `else`.
    fn gives_value(&self) -> bool {
        self.then.value.is_some()
            && match &self.otherwise {
                Some(Else::Block(block)) => block.value.is_some(),
                Some(Else::If(inner)) => inner.gives_value(),
                None => false,
            }
    }
}

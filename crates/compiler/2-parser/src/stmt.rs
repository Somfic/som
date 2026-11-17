use som_ast::{Expr, Id, Stmt};
use som_base::message::code;
use som_base::{Fix, Span, msg};
use som_lexer::TokenKind;

use crate::{ParseError, Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn parse_stmt_or_expr(&mut self) -> StmtOrExpr {
        // `pass`, `switch (x) { … }`: other languages' statements, reported and read past.
        let start = self.current_span();
        if self.foreign_statement().is_some() {
            let hole = self.ast.exprs.alloc(Expr::Hole, start.clone());
            return StmtOrExpr::Stmt(self.ast.stmts.alloc(Stmt::Expr(hole), start));
        }
        // `++x`: reported, and `x` read on.
        self.eat_foreign_prefix_step();
        let stmt = match self.peek() {
            // `use som::data;`, for the rest of the block.
            TokenKind::Use => {
                let start = self.current_span();
                let Some(u) = self.parse_use() else {
                    return StmtOrExpr::Error;
                };
                self.ast.modules[self.module]
                    .block_uses
                    .push(u.path.clone());
                let span = self.span_from(&start);
                Some(self.ast.stmts.alloc(Stmt::Use(u), span))
            }
            TokenKind::Let => self.parse_let(),
            TokenKind::Builtin if self.at_const(0) => self.parse_local_const(),
            TokenKind::Ident if self.foreign_let().is_some() || self.go_define() => {
                self.parse_let()
            }
            TokenKind::Loop => self.parse_loop(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            // `@for i in 0..4 { … }`, `@if DEBUG { … }`: settled while compiling.
            // `@for f in Point { … }`: written out once `Point` is known.
            TokenKind::Builtin if self.at_reflected_for() => self.parse_reflected_stmt(),
            TokenKind::Builtin if &*self.peek_token().text == "@for" => self.parse_settled_for(),
            TokenKind::Builtin if &*self.peek_token().text == "@if" => self.parse_settled_if(),
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

    /// `@const LIMIT = 10;` in a block.
    fn parse_local_const(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        let c = self.parse_const()?;
        Some(self.ast.stmts.alloc(Stmt::Const(c), self.span_from(&start)))
    }

    fn parse_let(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        // `name := value` (Go): reported, and read as the `let` it means.
        if self.go_define() {
            let name = self.eat_go_define()?;
            return self.parse_let_value(start, true, name, None);
        }
        // `const name`, `var name`: another language's `let`, reported and read as it.
        let mutable = match self.eat_foreign_let() {
            Some(mutable) => mutable || self.eat(TokenKind::Mut),
            None => {
                self.expect(TokenKind::Let, "a variable declaration")?;
                self.eat(TokenKind::Mut)
            }
        };
        // `let mutable total`: another language's `mut`.
        let mutable = self.eat_foreign_mut() || mutable;
        // `let Some(h) = hit() else { … };`: Rust's, reported.
        if self.ident_ahead(0) == Some("Some") && self.peek_nth(1) == TokenKind::OpenParen {
            return self.let_else(start);
        }
        let name = self.parse_ident("a variable name")?;
        // `let add a b = a + b`: another language's function, not a variable.
        if self.at(TokenKind::Ident) {
            let mut params = Vec::new();
            let mut n = 0;
            while let Some(param) = self.ident_ahead(n) {
                params.push(param.to_string());
                n += 1;
            }
            if self.peek_nth(n) == TokenKind::Equals {
                for _ in 0..=n {
                    self.advance();
                }
                let span = self.span_from(&start);
                let fixed = format!("fn {}({}) =", name.name, params.join(", "));
                self.errors.push(ParseError {
                    message: msg!["som declares a function with ", code("fn")],
                    hint: msg!["with parameters, this is a function, not a variable"],
                    fix: Some(Fix::replace(
                        msg!["change to ", code(&fixed)],
                        span.clone(),
                        fixed,
                    )),
                    span,
                });
                self.in_recovery = true;
                return None;
            }
        }
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Equals, "a variable value")?;
        self.parse_let_value(start, mutable, name, ty)
    }

    /// A `let`'s value and `;`, after its `=`.
    fn parse_let_value(
        &mut self,
        start: Span,
        mutable: bool,
        name: som_ast::Ident,
        ty: Option<Id<som_ast::TypeExpr>>,
    ) -> Option<Id<Stmt>> {
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
    pub(crate) fn parse_loop_target(&mut self) -> Option<Option<som_ast::Ident>> {
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
        // `for (int i = 0; i < n; i++)`: C's loop, reported and read as the range it counts.
        if self.at(TokenKind::OpenParen) {
            let (name, iterable) = self.c_for()?;
            return self.parse_for_body(start, false, name, iterable);
        }
        let mutable = self.eat(TokenKind::Mut);
        let name = self.parse_ident("a name for each value")?;
        // `for i = 1, n do`: Lua's, likewise.
        if self.at(TokenKind::Equals) {
            let iterable = self.lua_for(&name)?;
            return self.parse_for_body(start, mutable, name, iterable);
        }
        self.expect(TokenKind::In, "`in` and what to loop over")?;
        let iterable = self.parse_condition()?;
        self.parse_for_body(start, mutable, name, iterable)
    }

    /// `@for f in Point { … }` among statements: a statement standing for it, which the
    /// copies of its body replace once `Point` is known.
    fn parse_reflected_stmt(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        let found = self.parse_reflected_for();
        let span = self.span_from(&start);
        let hole = self.ast.exprs.alloc(Expr::Hole, span.clone());
        let stmt = self.ast.stmts.alloc(Stmt::Expr(hole), span);
        if let Some((names, of, body)) = found {
            self.ast.reflected.push(som_ast::Reflected {
                module: self.module,
                names,
                of,
                body,
                into: som_ast::ReflectedInto::Stmts { stmt },
            });
        }
        Some(stmt)
    }

    /// `@for i in 0..4 { … }`: a `for` copied once for each value, while compiling.
    fn parse_settled_for(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.advance();
        let mutable = self.eat(TokenKind::Mut);
        let name = self.parse_ident("a name for each value")?;
        self.expect(TokenKind::In, "`in` and what to loop over")?;
        let iterable = self.parse_condition()?;
        let id = self.parse_for_body(start, mutable, name, iterable)?;
        self.ast.settled.insert(id);
        Some(id)
    }

    /// `@if DEBUG { … } else { … }`: only the branch the condition picks, chosen while
    /// compiling. `else @if` goes on choosing.
    fn parse_settled_if(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.advance();
        let condition = self.parse_settled_condition()?;
        let then = self.parse_stmt_block("the @if body")?;
        let otherwise = match self.eat(TokenKind::Else) {
            true if self.at(TokenKind::Builtin) && &*self.peek_token().text == "@if" => {
                Some(vec![self.parse_settled_if()?])
            }
            true => Some(self.parse_stmt_block("the else body")?),
            false => None,
        };
        let span = self.span_from(&start);
        let id = self.ast.stmts.alloc(
            Stmt::If {
                condition,
                then,
                otherwise,
            },
            span,
        );
        self.ast.settled.insert(id);
        Some(id)
    }

    fn parse_for_body(
        &mut self,
        start: Span,
        mutable: bool,
        name: som_ast::Ident,
        iterable: Id<Expr>,
    ) -> Option<Id<Stmt>> {
        // `for line in file` with its body not written yet: reported, and kept as a loop with
        // none, so what it loops over is still checked and `file.` can be completed.
        let from = self.pos;
        let body = match self.parse_stmt_block("the for body") {
            Some(body) => body,
            None if self.pos == from => Vec::new(),
            None => return None,
        };
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
        self.parse_if_rest(start)
    }

    /// An `if` after its `if` (or another language's `elif`), which started at `start`.
    fn parse_if_rest(&mut self, start: Span) -> Option<IfParts> {
        let condition = match self.at(TokenKind::Let) {
            true => self.if_let(&start)?,
            false => self.parse_condition()?,
        };
        let then = self.parse_branch("the if body")?;
        // `elif c`, `elseif c`: another language's `else if`, reported and read as it.
        let otherwise = if self.eat_foreign_else_if() {
            Some(Else::If(Box::new(
                self.parse_if_rest(self.previous_span())?,
            )))
        } else if !self.eat(TokenKind::Else) {
            None
        } else if self.at(TokenKind::If) {
            Some(Else::If(Box::new(self.parse_if_parts()?)))
        } else if self.eat_foreign_else_if() {
            Some(Else::If(Box::new(
                self.parse_if_rest(self.previous_span())?,
            )))
        } else {
            Some(Else::Block(self.parse_branch("the else body")?))
        };
        Some(IfParts {
            condition,
            then,
            otherwise,
            span: self.span_from(&start),
        })
    }

    /// `if let t = f() { … }` or `if let Some(t) = f()`, at the `let`: Rust's, reported, with
    /// the fix of checking the optional itself, `let t = f();` then `if t { … }`. Read as
    /// `if f()`, to go on.
    fn if_let(&mut self, start: &Span) -> Option<Id<Expr>> {
        self.advance();
        let name = self.bound_name()?;
        self.expect(TokenKind::Equals, "`=` and the optional")?;
        let value = self.parse_condition()?;
        let span = start.merge(self.ast.exprs.span(value));
        let value_text = self.ast.exprs.span(value).get_text().to_string();
        let n = &name.name;
        let fix = line_indent(start).map(|indent| {
            let fixed = format!("let {n} = {value_text};\n{indent}if {n}");
            Fix::replace(msg!["check the optional itself: ", code(format!("if {n}"))], span.clone(), fixed)
        });
        self.errors.push(ParseError {
            message: msg!["som has no ", code("if let")],
            hint: msg![
                "an optional in a variable is its own check: inside ",
                code(format!("if {n} {{ … }}")),
                ", ",
                code(n.to_string()),
                " is the value it holds"
            ],
            fix,
            span,
        });
        Some(value)
    }

    /// `let Some(h) = hit() else { return; };`, at `Some`: Rust's, reported, with the fix of
    /// `let h = hit();` then `if !h { return; }`, after which `h` is the value.
    fn let_else(&mut self, start: Span) -> Option<Id<Stmt>> {
        let name = self.bound_name()?;
        self.expect(TokenKind::Equals, "`=` and the optional")?;
        // `hit() else { … }` reads as a fallback.
        let value = self.parse_expr()?;
        let end = self.current_span();
        self.expect(TokenKind::Semicolon, "a semicolon after the `else` block")?;
        let span = self.span_from(&start);
        let n = &name.name;
        let fix = match &self.ast.exprs[value] {
            Expr::Fallback { optional, fallback } => line_indent(&start).map(|indent| {
                let optional = self.ast.exprs.span(*optional).get_text().to_string();
                let block = self.ast.exprs.span(*fallback);
                let before = Span::from_range(start.start_offset..block.start_offset, start.source.clone());
                let mut fix = Fix::replace(
                    msg!["check the optional itself: ", code(format!("if !{n}"))],
                    before,
                    format!("let {n} = {optional};\n{indent}if !{n} "),
                );
                fix.edits.push((end.clone(), String::new()));
                fix
            }),
            _ => None,
        };
        self.errors.push(ParseError {
            message: msg!["som has no ", code("let … else")],
            hint: msg![
                "an optional in a variable is its own check: after ",
                code(format!("if !{n} {{ return …; }}")),
                ", ",
                code(n.to_string()),
                " is the value it holds"
            ],
            fix,
            span,
        });
        None
    }

    /// `t` or `Some(t)`: the name Rust's `if let` and `let … else` bind.
    fn bound_name(&mut self) -> Option<som_ast::Ident> {
        if self.ident_ahead(0) == Some("Some") && self.peek_nth(1) == TokenKind::OpenParen {
            self.advance();
            self.advance();
            let name = self.parse_ident("a name for the value")?;
            self.expect(TokenKind::CloseParen, "`)` after the name")?;
            return Some(name);
        }
        self.parse_ident("a name for the value")
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
            let stmt = self.ast.stmts.alloc(Stmt::Expr(value), span);
            self.ast.unterminated.insert(stmt);
            stmts.push(stmt);
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
    /// An `if`'s or `else`'s body: a block, or one statement without braces,
    /// `if n == 0 return 1;`, `if .keys.size == 0 @unreachable;`.
    fn parse_branch(&mut self, what: &str) -> Option<BlockParts> {
        if self.at(TokenKind::OpenBrace) {
            return self.parse_block_parts(what);
        }
        // `then … end`, Python's `:`: another language's body, reported.
        if let Some(parts) = self.foreign_body() {
            return Some(parts);
        }
        let start = self.current_span();
        match self.parse_stmt_or_expr() {
            StmtOrExpr::Stmt(stmt) => Some(BlockParts::new(vec![stmt], self.span_from(&start))),
            StmtOrExpr::Expr(_) => {
                self.error_missing("expected `;` after the if's statement", "a semicolon");
                None
            }
            StmtOrExpr::Error => None,
        }
    }

    pub(crate) fn parse_block_parts(&mut self, what: &str) -> Option<BlockParts> {
        // `then … end`, Python's `:`: a body as another language writes it.
        if !self.at(TokenKind::OpenBrace)
            && let Some(parts) = self.foreign_body()
        {
            return Some(parts);
        }
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

impl BlockParts {
    /// A block of `stmts`, ending in no value.
    pub(crate) fn new(stmts: Vec<Id<Stmt>>, span: Span) -> Self {
        BlockParts {
            stmts,
            value: None,
            span,
        }
    }
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

/// The indentation of the line `at` starts, if nothing else is before it there.
fn line_indent(at: &Span) -> Option<String> {
    let source = at.source.content();
    let line_start = source[..at.start_offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &source[line_start..at.start_offset];
    before.chars().all(|c| c == ' ' || c == '\t').then(|| before.to_string())
}

use som_ast::{Expr, Id, Ident, Path, Span, Stmt};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lang::Builtin;
use som_lexer::TokenKind;

use crate::Parser;
use crate::ty::primitive;

impl Parser<'_> {
    pub(super) fn parse_atom(&mut self) -> Option<Id<Expr>> {
        self.eat_foreign_f_string();
        let start = self.current_span();
        let expr = match self.peek() {
            TokenKind::Int => self.parse_int_literal(),
            TokenKind::Float => {
                let value = self.peek_token().text.parse().unwrap_or(0.0);
                self.advance();
                Expr::Float(value)
            }
            TokenKind::None => {
                self.advance();
                Expr::None
            }
            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Expr::Bool(value)
            }
            TokenKind::Text => {
                let token = self.peek_token().clone();
                self.advance();
                self.text_literal(&token)
            }
            TokenKind::OpenBracket => self.parse_array_literal()?,
            TokenKind::Return | TokenKind::Break | TokenKind::Continue => {
                self.parse_leave(&start)?
            }
            TokenKind::Builtin => self.parse_builtin_value(&start)?,
            TokenKind::Dot => return self.parse_self_dot(),
            TokenKind::SelfValue => Expr::Var(Path {
                segments: vec![self.parse_self()],
            }),
            // `i32::parse(…)`: a built-in type's own function.
            kind if primitive(kind).is_some() && self.peek_nth(1) == TokenKind::DoubleColon => {
                self.parse_primitive_path()?
            }
            TokenKind::Ident | TokenKind::SelfType => return self.parse_name(&start),
            // `()`: the value of type `()`, as an empty block is.
            TokenKind::OpenParen if self.peek_nth(1) == TokenKind::CloseParen => {
                self.advance_by(2);
                Expr::Block {
                    stmts: Vec::new(),
                    value: None,
                }
            }
            TokenKind::OpenParen => {
                self.advance();
                let inner = self.nested(|p| p.parse_expr())?;
                self.expect(TokenKind::CloseParen, "closing parenthesis")?;
                return Some(inner);
            }
            TokenKind::OpenBrace => return self.nested(|p| p.parse_block()),
            TokenKind::Match => return self.parse_match(),
            TokenKind::If => {
                let parts = self.parse_if_parts()?;
                return Some(self.if_expr(parts));
            }
            // `@if T == f32`: a type, compared with another.
            kind if self.ctx.settled && primitive(kind).is_some() => Expr::Type(self.parse_type()?),
            kind if primitive(kind).is_some() => return Some(self.type_for_value(kind, start)),
            _ => {
                self.error_missing("expected expression", "an expression");
                return Some(self.ast.exprs.alloc(Expr::Hole, start));
            }
        };
        Some(self.alloc_expr(expr, &start))
    }

    fn parse_int_literal(&mut self) -> Expr {
        let start = self.current_span();
        let text = self.peek_token().text.clone();
        self.advance();
        // `0x` or `0b` alone: the digits are missing, not too many.
        if !text.chars().skip(2).any(|c| c.is_ascii_alphanumeric())
            && matches!(text.get(..2), Some("0x" | "0b"))
        {
            self.error_at(
                start,
                "number without digits",
                &format!("`{}` needs digits after it", &text[..2]),
            );
            return Expr::Hole;
        }
        match crate::int_value(&text).and_then(|n| i128::try_from(n).ok()) {
            Some(value) => Expr::Int(value),
            None => {
                self.error_at(
                    start,
                    "integer literal is too large",
                    "this does not fit in 128 bits",
                );
                Expr::Hole
            }
        }
    }

    /// A count written as an integer literal: an array's length, or how many copies.
    pub(crate) fn parse_count(&mut self, what: &str) -> Option<u128> {
        let span = self.current_span();
        if !self.at(TokenKind::Int) {
            self.error_expected(&[TokenKind::Int], what);
            return None;
        }
        let text = self.peek_token().text.clone();
        self.advance();
        let count = crate::int_value(&text);
        if count.is_none() {
            self.error_at(span, "count is too large", "this does not fit in 128 bits");
        }
        count
    }

    /// `[a, b, c]` or `[value; count]`.
    fn parse_array_literal(&mut self) -> Option<Expr> {
        self.advance();
        if self.eat(TokenKind::CloseBracket) {
            return Some(Expr::Array(Vec::new()));
        }
        let first = self.nested(|p| p.parse_expr())?;
        if self.at(TokenKind::Semicolon) {
            return self.parse_array_repeat(first);
        }
        let mut items = vec![first];
        while self.list_continues(TokenKind::CloseBracket) && !self.at(TokenKind::CloseBracket) {
            items.push(self.nested(|p| p.parse_expr())?);
        }
        self.expect(
            TokenKind::CloseBracket,
            "a closing bracket after the elements",
        )?;
        Some(Expr::Array(items))
    }

    /// `[value; count]`, at the `;`.
    fn parse_array_repeat(&mut self, value: Id<Expr>) -> Option<Expr> {
        let mut separators = vec![self.current_span()];
        self.advance();
        let count = self.parse_length("how many copies")?;
        // `[1; 2; 3]`: elements separated as in another language.
        if self.at(TokenKind::Semicolon) {
            while self.at(TokenKind::Semicolon) {
                separators.push(self.current_span());
                self.advance();
                self.nested(|p| p.parse_expr())?;
            }
            let span = separators[0].clone();
            let fix = Fix {
                title: msg!["change each ", code(";"), " to ", code(",")],
                edits: (separators.into_iter()).map(|s| (s, ",".into())).collect(),
            };
            self.error(
                span,
                msg![
                    "elements are separated with ",
                    code(","),
                    ", not ",
                    code(";")
                ],
                msg![
                    code("[value; count]"),
                    " is ",
                    code("count"),
                    " copies of one value"
                ],
                Some(fix),
            );
        }
        self.expect(TokenKind::CloseBracket, "a closing bracket")?;
        Some(Expr::ArrayRepeat { value, count })
    }

    /// `get(i) else return none`: leaving, where a value is expected. It's read as the block
    /// `{ return none; }`, which never ends with a value, so it fits anywhere.
    fn parse_leave(&mut self, start: &Span) -> Option<Expr> {
        let kind = self.peek();
        self.advance();
        let stmt = match kind {
            TokenKind::Return => Stmt::Return(match self.can_start_expr() {
                true => Some(self.parse_expr()?),
                false => None,
            }),
            TokenKind::Break => Stmt::Break(self.parse_loop_target()?),
            _ => Stmt::Continue(self.parse_loop_target()?),
        };
        let stmt = self.ast.stmts.alloc(stmt, self.span_from(start));
        Some(Expr::Block {
            stmts: vec![stmt],
            value: None,
        })
    }

    /// `@unreachable` and `@fail(message)`, the built-ins that are values.
    fn parse_builtin_value(&mut self, start: &Span) -> Option<Expr> {
        let text = self.peek_token().text.clone();
        self.advance();
        match Builtin::from_name(&text) {
            Some(Builtin::Unreachable) => Some(Expr::Unreachable),
            Some(Builtin::Fail) => {
                self.expect(TokenKind::OpenParen, "`(` and the message")?;
                let message = self.parse_expr()?;
                self.expect(TokenKind::CloseParen, "`)` after the message")?;
                Some(Expr::Fail(message))
            }
            _ => {
                self.error_unknown_builtin(start.clone(), &text);
                Some(Expr::Hole)
            }
        }
    }

    pub(crate) fn error_unknown_builtin(&mut self, span: Span, name: &str) {
        let known: Vec<&str> = (Builtin::ALL.iter())
            .filter(|b| b.doc().is_some())
            .map(|b| b.name())
            .collect();
        self.error_at(
            span,
            msg!["unknown built-in ", code(name.to_string())],
            msg!["the built-ins are ", code(known.join(", "))],
        );
    }

    /// `.x` and `.len()` are short for `self.x` and `self.len()`. The `self` they mean is at
    /// the dot, which is where an editor finds it while the name is typed.
    fn parse_self_dot(&mut self) -> Option<Id<Expr>> {
        let dot = self.current_span();
        if !self.ctx.impl_block {
            self.error_at(
                dot.clone(),
                msg![code(".name"), " is short for ", code("self.name")],
                msg![code("self"), " only exists in methods"],
            );
        }
        self.advance();
        let path = Path {
            segments: vec![Ident {
                name: som_lang::SELF_VALUE.into(),
                span: dot.clone(),
            }],
        };
        let receiver = self.ast.exprs.alloc(Expr::Var(path), dot.clone());
        let Some(name) = self.parse_ident("a field or method name") else {
            return Some(receiver);
        };
        let expr = self.member(receiver, name)?;
        Some(self.alloc_expr(expr, &dot))
    }

    fn parse_primitive_path(&mut self) -> Option<Expr> {
        let token = self.peek_token().clone();
        self.advance();
        let mut segments = vec![Ident {
            name: token.text.as_ref().into(),
            span: token.span.clone(),
        }];
        while self.eat(TokenKind::DoubleColon) {
            segments.push(self.parse_ident("a function name")?);
        }
        Some(Expr::Var(Path { segments }))
    }

    /// `foo(i32)`: a type where a value goes.
    fn type_for_value(&mut self, kind: TokenKind, start: Span) -> Id<Expr> {
        self.report(
            start.clone(),
            msg!["expected a value, found type ", code(kind.to_string())],
            "this is a type, where a value goes",
            None,
        );
        self.in_recovery = true;
        // Past it, so whatever reads a list of values goes on.
        self.advance();
        self.ast.exprs.alloc(Expr::Hole, start)
    }

    /// A name, or a path: a variable, a constructor, or a function called with its type
    /// arguments written out.
    fn parse_name(&mut self, start: &Span) -> Option<Id<Expr>> {
        if self.constructor_ahead() && !self.ctx.condition {
            return self.parse_constructor();
        }
        let mut path = self.parse_path("an identifier or constructor name")?;
        // `Alloc<i32>::with_capacity(10)`: the type's arguments, then its function's name.
        // Only when `::` follows the `>`, so a comparison stays one.
        let mut owner_args = Vec::new();
        if self.at(TokenKind::LessThan)
            && let Some(n) = self.past_type_args(0)
            && self.peek_nth(n) == TokenKind::DoubleColon
        {
            owner_args = self.parse_type_args()?;
            while self.eat(TokenKind::DoubleColon) {
                path.segments.push(self.parse_ident("a function name")?);
            }
        }
        // `null<i32>()`: type arguments, then the call. Only when `(` follows the `>`, so
        // `a < b` and `a < b > c` stay comparisons.
        let type_args = self.at(TokenKind::LessThan)
            && self
                .past_type_args(0)
                .is_some_and(|n| self.peek_nth(n) == TokenKind::OpenParen);
        let expr = if type_args || !owner_args.is_empty() {
            let type_args = match type_args {
                true => self.parse_type_args()?,
                false => Vec::new(),
            };
            // A type's arguments are only written to call one of its functions.
            let args = self.parse_args()?;
            Expr::Call {
                callee: path,
                owner_args,
                type_args,
                args,
            }
        } else {
            Expr::Var(path)
        };
        Some(self.alloc_expr(expr, start))
    }

    /// `Name { field: …`, `Name {}`, `module::Name { … }` or `Self { … }`; not `name { … }`.
    fn constructor_ahead(&self) -> bool {
        let mut n = 1;
        while self.peek_nth(n) == TokenKind::DoubleColon && self.peek_nth(n + 1) == TokenKind::Ident
        {
            n += 2;
        }
        // `Pair<i32> { … }`: type arguments before the brace.
        if self.peek_nth(n) == TokenKind::LessThan {
            n = self.past_type_args(n).unwrap_or(n);
        }
        // `Pair { a, b: 2 }`: `a` short for `a: a`. Only for a capitalised name, which a
        // struct's is: `for x in items { x }` is a loop.
        let capitalised = (n.checked_sub(1).and_then(|last| self.ident_ahead(last)))
            .is_some_and(|name| name.starts_with(|c: char| c.is_uppercase()));
        let shorthand = capitalised
            && self.peek_nth(n + 1) == TokenKind::Ident
            && matches!(
                self.peek_nth(n + 2),
                TokenKind::Comma | TokenKind::CloseBrace
            );
        self.peek_nth(n) == TokenKind::OpenBrace
            && (self.peek_nth(n + 1) == TokenKind::CloseBrace
                || shorthand
                || (self.peek_nth(n + 1) == TokenKind::Ident
                    && self.peek_nth(n + 2) == TokenKind::Colon))
    }

    /// `Point { x: 1, y: 2 }`, `Pair<i32> { a: 1, b: 2 }`, or `Point { x, y }` of variables
    /// named like the fields.
    fn parse_constructor(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        let ty = self.parse_path("a constructor name")?;
        let args = self.parse_type_args()?;
        self.expect(TokenKind::OpenBrace, "constructor fields")?;
        let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
            let name = p.parse_ident("a constructor field name")?;
            let value = if p.eat(TokenKind::Colon) {
                p.parse_expr()?
            } else {
                let path = Path {
                    segments: vec![name.clone()],
                };
                p.ast.exprs.alloc(Expr::Var(path), name.span.clone())
            };
            Some((name, value))
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after constructor fields",
        )?;
        Some(self.alloc_expr(Expr::Construct { ty, args, fields }, &start))
    }

    /// An argument: an expression, or `out name`.
    fn parse_arg(&mut self) -> Option<Id<Expr>> {
        if !self.at(TokenKind::Out) {
            return self.parse_expr();
        }
        let start = self.current_span();
        self.advance();
        let name = self.parse_ident("a name for the out variable")?;
        Some(self.alloc_expr(Expr::Out(name), &start))
    }

    /// `(a, b, c)`
    pub(super) fn parse_args(&mut self) -> Option<Vec<Id<Expr>>> {
        self.expect(TokenKind::OpenParen, "a list of arguments")?;
        let args = self.nested(|p| p.parse_comma_list(TokenKind::CloseParen, |p| p.parse_arg()))?;
        // `free(alloc(10).` while typing: report the missing `)`, but keep the call and its
        // arguments, so an editor still knows them.
        if !self.eat(TokenKind::CloseParen) {
            self.error_expected(
                &[TokenKind::CloseParen],
                "a closing parenthesis after the arguments",
            );
        }
        Some(args)
    }

    /// `foo(..)` or `math::add(..)`, by name; or what an expression gives, `ops[0](..)`.
    pub(super) fn parse_call(&mut self, callee: Id<Expr>, start: &Span) -> Option<Id<Expr>> {
        let args = self.parse_args()?;
        let expr = match &self.ast.exprs[callee] {
            Expr::Var(path) => Expr::Call {
                callee: path.clone(),
                owner_args: Vec::new(),
                type_args: Vec::new(),
                args,
            },
            // `str(5)`: what isn't a value is already reported.
            Expr::Hole => Expr::Hole,
            _ => {
                let call = Expr::CallValue { callee, args };
                return Some(self.alloc_expr(call, start));
            }
        };
        // The callee node becomes the call, so no unreferenced node is left behind.
        let span = self.span_from(start);
        self.ast.exprs.replace(callee, expr, span);
        Some(callee)
    }

    /// How far ahead the `>` closing type arguments that open at `n` (a `<`) is, plus one;
    /// `None` if what's there can't be type arguments, as in `a < b`.
    pub(crate) fn past_type_args(&self, mut n: usize) -> Option<usize> {
        let mut depth = 0;
        loop {
            match self.peek_nth(n) {
                TokenKind::LessThan => depth += 1,
                TokenKind::GreaterThan => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(n + 1);
                    }
                }
                // What a type can be made of.
                TokenKind::Ident
                | TokenKind::DoubleColon
                | TokenKind::Comma
                | TokenKind::Ampersand
                | TokenKind::Mut
                | TokenKind::Question
                | TokenKind::OpenBracket
                | TokenKind::CloseBracket
                | TokenKind::Semicolon
                | TokenKind::Int
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Plus
                | TokenKind::Star
                | TokenKind::Primitive(_) => {}
                _ => return None,
            }
            n += 1;
        }
    }
}

use som_ast::{Arm, Expr, Id, Ident, Part, Pattern, PatternFields, Span, Stmt};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::{ParseError, Parser};
use crate::grammar::{Association, Grammar, OpInfo};
use som_ast::BinOp;

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    /// The condition of an `if` or `while`, or the optional of an `if let`: `x { … }` there
    /// is `x` followed by the body. Write `(Point { … })` for a constructor.
    pub(crate) fn parse_condition(&mut self) -> Option<Id<Expr>> {
        self.with_condition(true, |p| p.parse_expr())
    }

    pub(crate) fn with_condition<T>(
        &mut self,
        in_condition: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer = std::mem::replace(&mut self.in_condition, in_condition);
        let result = f(self);
        self.in_condition = outer;
        result
    }

    fn alloc_expr(&mut self, expr: Expr, start: &Span) -> Id<Expr> {
        let span = self.span_from(start);
        self.ast.exprs.alloc(expr, span)
    }

    /// Pratt parser: parses an expression whose operators all bind at least `min_bp` tightly.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
        let start = self.current_span();
        let mut lhs = self.parse_prefix_or_atom()?;

        loop {
            // `x++`, ending a statement: reported, and read past.
            if self.eat_foreign_step(lhs) {
                continue;
            }
            // `a && b`, `a === b`: read as som's operators.
            self.rewrite_foreign_binary_op();
            // `.field` or `.method(args)`
            if self.at(TokenKind::Dot) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                self.advance();
                // `alloc(10).` while typing: report the missing name, but keep what's
                // before the dot, so an editor still knows its type and can list fields.
                // `System.out`: after a `.`, the keyword `out` can only be a name.
                if self.at(TokenKind::Out) {
                    self.tokens[self.pos].kind = TokenKind::Ident;
                }
                // `text.cstr()`: a `string`'s text for C is its `c_str`, a property.
                if self.at(TokenKind::CStr) {
                    self.c_str_misspelt();
                }
                let Some(name) = self.parse_ident("a field or method name") else {
                    break;
                };
                lhs = if self.at(TokenKind::OpenParen) {
                    let args = self.parse_args()?;
                    self.alloc_expr(
                        Expr::MethodCall {
                            receiver: lhs,
                            method: name,
                            args,
                        },
                        &start,
                    )
                } else {
                    self.alloc_expr(
                        Expr::Field {
                            object: lhs,
                            field: name,
                        },
                        &start,
                    )
                };
                continue;
            }

            // `values[0]`
            if self.at(TokenKind::OpenBracket) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::CloseBracket, "a closing bracket after the index")?;
                lhs = self.alloc_expr(Expr::Index { object: lhs, index }, &start);
                continue;
            }

            // `callee(args)`
            if self.at(TokenKind::OpenParen) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                lhs = self.parse_call(lhs, &start)?;
                continue;
            }

            // `then if condition else otherwise`, on one line: an `if` starting a line is a
            // statement of its own, after a forgotten `;`.
            if self.at(TokenKind::If) {
                if 1 < min_bp || self.current_span().start.line > self.previous_span().end.line {
                    break;
                }
                self.advance();
                // Stops before `else`, which belongs to this conditional, not to a fallback.
                let condition = self.parse_expr_bp(Grammar::FALLBACK + 1)?;
                self.expect(TokenKind::Else, "an else branch")?;
                let otherwise = self.parse_expr()?;
                lhs = self.alloc_expr(
                    Expr::Conditional {
                        condition,
                        then: lhs,
                        otherwise,
                    },
                    &start,
                );
                continue;
            }

            // `optional else fallback`
            // In a body without braces, an `else` starting a line continues the `if`.
            let continues_if = self.foreign_depth > 0
                && self.current_span().start.line > self.previous_span().end.line;
            if self.at(TokenKind::Else) && !continues_if {
                if Grammar::FALLBACK < min_bp {
                    break;
                }
                self.advance();
                let fallback = self.parse_expr_bp(Grammar::FALLBACK)?;
                lhs = self.alloc_expr(
                    Expr::Fallback {
                        optional: lhs,
                        fallback,
                    },
                    &start,
                );
                continue;
            }

            // `target = value`
            if self.at(TokenKind::Equals) {
                if Grammar::ASSIGNMENT < min_bp {
                    break;
                }
                // `if n = 0`: a comparison, surely, not an assignment.
                if self.in_condition && !self.in_recovery {
                    let span = self.current_span();
                    self.errors.push(crate::ParseError {
                        message: msg![code("="), " assigns; to compare, use ", code("==")],
                        hint: "an assignment in a condition".into(),
                        fix: Some(som_base::Fix::replace(
                            msg!["change to ", code("==")],
                            span.clone(),
                            "==",
                        )),
                        span,
                    });
                }
                self.advance();
                let value = self.parse_expr_bp(Grammar::ASSIGNMENT)?;
                lhs = self.alloc_expr(Expr::Assign { target: lhs, value }, &start);
                continue;
            }

            // `target += value`: `target = target + value`.
            if matches!(self.peek(), TokenKind::PlusEquals | TokenKind::MinusEquals) {
                if Grammar::ASSIGNMENT < min_bp {
                    break;
                }
                let op = match self.peek() {
                    TokenKind::PlusEquals => BinOp::Add,
                    _ => BinOp::Subtract,
                };
                let at = self.current_span();
                self.advance();
                let value = self.parse_expr_bp(Grammar::ASSIGNMENT)?;
                let Some(read) = self.copy_place(lhs) else {
                    self.error_at(
                        at,
                        msg![code(op.symbol().to_string() + "="), " changes a variable, field or element"],
                        msg!["write it out: ", code("target = target + value")],
                    );
                    continue;
                };
                let sum = self.alloc_expr(Expr::Binary { op, lhs: read, rhs: value }, &start);
                lhs = self.alloc_expr(Expr::Assign { target: lhs, value: sum }, &start);
                continue;
            }

            // `value as ty`
            if self.at(TokenKind::As) {
                if Grammar::AS * 2 < min_bp {
                    break;
                }
                self.advance();
                let ty = self.parse_type()?;
                lhs = self.alloc_expr(Expr::Cast { value: lhs, ty }, &start);
                continue;
            }

            // `start..=end`: up to and including `end`, which is `start..end + 1`.
            if self.at(TokenKind::DotDotEquals) {
                if Grammar::RANGE * 2 < min_bp {
                    break;
                }
                self.advance();
                let end = self.parse_expr_bp(Grammar::RANGE * 2 + 1)?;
                let end = self.past(end);
                lhs = self.alloc_expr(
                    Expr::Range {
                        start: Some(lhs),
                        end: Some(end),
                    },
                    &start,
                );
                continue;
            }

            // `start..end`, or `start..` with the end left open.
            if self.at(TokenKind::DotDot) {
                if Grammar::RANGE * 2 < min_bp {
                    break;
                }
                self.advance();
                let end = if self.can_start_expr() {
                    Some(self.parse_expr_bp(Grammar::RANGE * 2 + 1)?)
                } else {
                    None
                };
                lhs = self.alloc_expr(
                    Expr::Range {
                        start: Some(lhs),
                        end,
                    },
                    &start,
                );
                continue;
            }

            // `<<` and `>>` are two `<` or `>` right next to each other: one token would end
            // `List<List<i32>>` too soon.
            let shift = match self.peek() {
                TokenKind::LessThan => Some(BinOp::ShiftLeft),
                TokenKind::GreaterThan => Some(BinOp::ShiftRight),
                _ => None,
            }
            .filter(|_| self.tokens.get(self.pos + 1).map(|t| t.kind) == Some(self.peek()));
            let (op, info) = match shift {
                Some(op) => (
                    op,
                    OpInfo {
                        precedence: Grammar::SHIFT,
                        association: Association::Left,
                    },
                ),
                None => match Grammar::infix_op(self.peek()) {
                    Some(found) => found,
                    None => break,
                },
            };
            let (lbp, rbp) = info.binding_power();
            if lbp < min_bp {
                break;
            }
            if shift.is_some() {
                self.advance();
            }
            self.advance();
            let rhs = self.parse_expr_bp(rbp)?;
            lhs = self.alloc_expr(Expr::Binary { op, lhs, rhs }, &start);
        }

        Some(lhs)
    }

    fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        // Other languages' ways, reported: an anonymous function, which som doesn't have,
        // stops the expression; `new Point(…)` and `not x` are read as what they mean.
        if self.foreign_lambda() || self.at(TokenKind::OpenParen) && self.foreign_lambda_params() {
            return Some(self.ast.exprs.alloc(Expr::Hole, start));
        }
        self.eat_foreign_new();
        self.rewrite_foreign_not();
        // `&&x`: a borrow of a borrow, lexed as one `&&`.
        if self.at(TokenKind::AndAnd) {
            self.advance();
            let bp = Grammar::prefix_bp(TokenKind::Ampersand).expect("`&` is a prefix");
            let inner = self.parse_expr_bp(bp)?;
            let inner = self.alloc_expr(
                Expr::Borrow {
                    mutable: false,
                    expr: inner,
                },
                &start,
            );
            return Some(self.alloc_expr(
                Expr::Borrow {
                    mutable: false,
                    expr: inner,
                },
                &start,
            ));
        }
        let kind = self.peek();

        // `..=end`: from the start, up to and including `end`.
        if kind == TokenKind::DotDotEquals {
            self.advance();
            let end = self.parse_expr_bp(Grammar::RANGE * 2 + 1)?;
            let end = self.past(end);
            return Some(self.alloc_expr(Expr::Range { start: None, end: Some(end) }, &start));
        }

        // `..end`, or `..`: a range from the start.
        if kind == TokenKind::DotDot {
            self.advance();
            let end = if self.can_start_expr() {
                Some(self.parse_expr_bp(Grammar::RANGE * 2 + 1)?)
            } else {
                None
            };
            return Some(self.alloc_expr(Expr::Range { start: None, end }, &start));
        }

        // `@kilobytes(4)`, `@som::assert(…)`: a function called while compiling, like
        // `@const kilobytes(4)`. A built-in's name is the built-in's.
        if self.at_comptime_call(0) {
            let name = self.peek_token().text.clone();
            let first = som_ast::Ident {
                name: name[1..].into(),
                span: self.current_span(),
            };
            self.advance();
            let mut segments = vec![first];
            while self.eat(TokenKind::DoubleColon) {
                segments.push(self.parse_ident("a name after `::`")?);
            }
            // `@make_identity<N>()`: type arguments, then the call.
            let type_args = match self.at(TokenKind::LessThan) {
                true => self.parse_type_args()?,
                false => Vec::new(),
            };
            let args = self.parse_args()?;
            let call = self.alloc_expr(
                Expr::Call {
                    callee: som_ast::Path { segments },
                    owner_args: Vec::new(),
                    type_args,
                    args,
                },
                &start,
            );
            let span = self.span_from(&start);
            let c = self.ast.consts.alloc(
                som_ast::Const {
                    doc: None,
                    visibility: som_ast::Visibility::Private,
                    name: som_ast::Ident {
                        name: som_ast::Const::UNNAMED.into(),
                        span: start.clone(),
                    },
                    ty: None,
                    value: call,
                    in_place: true,
                },
                span,
            );
            return Some(self.alloc_expr(Expr::InPlace(c), &start));
        }

        // `@const f(x)`, or `@const { … }`: binds like `-`, so `@const f(x) + 1` adds to it.
        if kind == TokenKind::Builtin && &*self.peek_token().text == "@const" {
            self.advance();
            let value = match self.at(TokenKind::OpenBrace) {
                true => self.parse_block()?,
                false => {
                    let bp = Grammar::prefix_bp(TokenKind::Minus).expect("`-` is a prefix");
                    self.parse_expr_bp(bp)?
                }
            };
            let span = self.span_from(&start);
            let c = self.ast.consts.alloc(
                som_ast::Const {
                    doc: None,
                    visibility: som_ast::Visibility::Private,
                    name: som_ast::Ident {
                        name: "@const".into(),
                        span: start.clone(),
                    },
                    ty: None,
                    value,
                    in_place: true,
                },
                span,
            );
            return Some(self.alloc_expr(Expr::InPlace(c), &start));
        }

        let Some(bp) = Grammar::prefix_bp(kind) else {
            return self.parse_atom();
        };

        self.advance();
        let mutable = kind == TokenKind::Ampersand && self.eat(TokenKind::Mut);
        let expr = self.parse_expr_bp(bp)?;
        let expr = match kind {
            TokenKind::Bang => Expr::Not(expr),
            TokenKind::Tilde => Expr::BitNot(expr),
            TokenKind::Ampersand => Expr::Borrow { mutable, expr },
            TokenKind::Star => Expr::Deref(expr),
            // `-1` is the literal `-1`, not `1` negated: so it can be the smallest `i64`, and
            // a `u8` can't be it.
            TokenKind::Minus => match self.ast.exprs[expr] {
                Expr::Int(n)
                    if self.ast.exprs.span(expr).start_offset == start.start_offset + 1 =>
                {
                    Expr::Int(n.wrapping_neg())
                }
                Expr::Float(x)
                    if self.ast.exprs.span(expr).start_offset == start.start_offset + 1 =>
                {
                    Expr::Float(-x)
                }
                _ => Expr::Neg(expr),
            },
            _ => unreachable!("prefix_bp only accepts prefix operators"),
        };
        Some(self.alloc_expr(expr, &start))
    }

    /// At `@name(` or `@name::`, `at` tokens ahead, where `@name` isn't a built-in: a call
    /// made while compiling.
    pub(crate) fn at_comptime_call(&self, at: usize) -> bool {
        // `@make_identity<N>(`: type arguments, then the call.
        let generic = self.peek_nth(at + 1) == TokenKind::LessThan
            && (self.past_type_args(at + 1)).is_some_and(|n| self.peek_nth(n) == TokenKind::OpenParen);
        self.peek_nth(at) == TokenKind::Builtin
            && (generic
                || matches!(
                    self.peek_nth(at + 1),
                    TokenKind::OpenParen | TokenKind::DoubleColon
                ))
            && (self.tokens[self.pos..].iter())
                .filter(|t| !crate::is_trivia(t.kind))
                .nth(at)
                .is_some_and(|t| !som_lexer::is_builtin(&t.text))
    }

    fn parse_atom(&mut self) -> Option<Id<Expr>> {
        // `f"total: {n}"`: Python's f-string. som's text writes values in it as it is.
        if self.at(TokenKind::Ident)
            && matches!(&*self.peek_token().text, "f" | "F")
            && self.peek_nth(1) == TokenKind::Text
            && self.span_ahead(1).start_offset == self.current_span().start_offset + 1
        {
            let prefix = self.current_span();
            if !self.in_recovery {
                self.errors.push(ParseError {
                    message: msg!["som's text has no ", code("f"), " in front"],
                    hint: msg!["any text writes the values in its ", code("{…}")],
                    fix: Some(Fix::replace(msg!["remove the ", code("f")], prefix.clone(), "")),
                    span: prefix,
                });
            }
            self.advance();
        }
        let start = self.current_span();

        let expr = match self.peek() {
            TokenKind::Int => {
                let text = self.peek_token().text.clone();
                self.advance();
                // `0x` or `0b` alone: the digits are missing, not too many.
                if !text.chars().skip(2).any(|c| c.is_ascii_alphanumeric())
                    && matches!(text.get(..2), Some("0x" | "0b"))
                {
                    self.error_at(
                        start.clone(),
                        "number without digits",
                        &format!("`{}` needs digits after it", &text[..2]),
                    );
                    return Some(self.alloc_expr(Expr::Hole, &start));
                }
                match crate::int_value(&text).and_then(|n| i128::try_from(n).ok()) {
                    Some(value) => Expr::Int(value),
                    None => {
                        self.error_at(
                            start.clone(),
                            "integer literal is too large",
                            "this does not fit in 128 bits",
                        );
                        Expr::Hole
                    }
                }
            }

            TokenKind::Float => {
                let value = self.peek_token().text.parse().unwrap_or(0.0);
                self.advance();
                Expr::Float(value)
            }

            TokenKind::None => {
                self.advance();
                Expr::None
            }

            // `[a, b, c]` or `[value; count]`
            TokenKind::OpenBracket => {
                self.advance();
                if self.eat(TokenKind::CloseBracket) {
                    Expr::Array(Vec::new())
                } else {
                    let first = self.with_condition(false, |p| p.parse_expr())?;
                    if self.at(TokenKind::Semicolon) {
                        let semicolon = self.current_span();
                        self.advance();
                        let count = self.parse_length("how many copies")?;
                        // `[1; 2; 3]`: elements separated as in another language.
                        if self.at(TokenKind::Semicolon) {
                            let mut separators = vec![semicolon];
                            while self.at(TokenKind::Semicolon) {
                                separators.push(self.current_span());
                                self.advance();
                                self.with_condition(false, |p| p.parse_expr())?;
                            }
                            let span = separators[0].clone();
                            self.errors.push(crate::ParseError {
                                message: msg![
                                    "elements are separated with ",
                                    code(","),
                                    ", not ",
                                    code(";")
                                ],
                                hint: msg![
                                    code("[value; count]"),
                                    " is ",
                                    code("count"),
                                    " copies of one value"
                                ],
                                fix: Some(som_base::Fix {
                                    title: msg!["change each ", code(";"), " to ", code(",")],
                                    edits: separators
                                        .into_iter()
                                        .map(|s| (s, ",".into()))
                                        .collect(),
                                }),
                                span,
                            });
                        }
                        self.expect(TokenKind::CloseBracket, "a closing bracket")?;
                        Expr::ArrayRepeat {
                            value: first,
                            count,
                        }
                    } else {
                        let mut items = vec![first];
                        while self.list_continues(TokenKind::CloseBracket)
                            && !self.at(TokenKind::CloseBracket)
                        {
                            items.push(self.with_condition(false, |p| p.parse_expr())?);
                        }
                        self.expect(
                            TokenKind::CloseBracket,
                            "a closing bracket after the elements",
                        )?;
                        Expr::Array(items)
                    }
                }
            }

            // `get(i) else return none`: leaving, where a value is expected. It's read as the
            // block `{ return none; }`, which never ends with a value, so it fits anywhere.
            TokenKind::Return | TokenKind::Break | TokenKind::Continue => {
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
                let stmt = self.ast.stmts.alloc(stmt, self.span_from(&start));
                Expr::Block {
                    stmts: vec![stmt],
                    value: None,
                }
            }

            // `@unreachable`, the only built-in a value can be so far.
            TokenKind::Builtin => {
                let text = self.peek_token().text.clone();
                self.advance();
                match &*text {
                    "@unreachable" => Expr::Unreachable,
                    "@fail" => {
                        self.expect(TokenKind::OpenParen, "`(` and the message")?;
                        let message = self.parse_expr()?;
                        self.expect(TokenKind::CloseParen, "`)` after the message")?;
                        Expr::Fail(message)
                    }
                    _ => {
                        let known: Vec<&str> = som_lexer::BUILTINS.iter().map(|b| b.0).collect();
                        self.error_at(
                            start.clone(),
                            msg!["unknown built-in ", code(text.to_string())],
                            msg!["the built-ins are ", code(known.join(", "))],
                        );
                        Expr::Hole
                    }
                }
            }

            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Expr::Bool(value)
            }

            TokenKind::Text => {
                let token = self.peek_token().clone();
                self.advance();
                let raw = &token.text[1..token.text.len() - 1];
                if raw.contains('{') || raw.contains('}') {
                    self.interpolated(&token)
                } else {
                    Expr::Str(self.unescape(raw, &start).into())
                }
            }

            // `.x` and `.len()` are short for `self.x` and `self.len()`. The `self` they mean
            // is at the dot, which is where an editor finds it while the name is typed.
            TokenKind::Dot => {
                let dot = self.current_span();
                if !self.in_impl {
                    self.error_at(
                        dot.clone(),
                        msg![code(".name"), " is short for ", code("self.name")],
                        msg![code("self"), " only exists in methods"],
                    );
                }
                self.advance();
                let path = som_ast::Path {
                    segments: vec![som_ast::Ident {
                        name: "self".into(),
                        span: dot.clone(),
                    }],
                };
                let receiver = self.ast.exprs.alloc(Expr::Var(path), dot);
                let Some(name) = self.parse_ident("a field or method name") else {
                    return Some(receiver);
                };
                if self.at(TokenKind::OpenParen) {
                    let args = self.parse_args()?;
                    Expr::MethodCall {
                        receiver,
                        method: name,
                        args,
                    }
                } else {
                    Expr::Field {
                        object: receiver,
                        field: name,
                    }
                }
            }

            // `self`, in a method.
            TokenKind::SelfValue => {
                let name = self.parse_self();
                Expr::Var(som_ast::Path {
                    segments: vec![name],
                })
            }

            // `i32::parse(…)`: a built-in type's own function.
            kind if crate::ty::primitive(kind).is_some()
                && self.peek_nth(1) == TokenKind::DoubleColon =>
            {
                let token = self.peek_token().clone();
                self.advance();
                let mut segments = vec![som_ast::Ident {
                    name: token.text.as_ref().into(),
                    span: token.span.clone(),
                }];
                while self.eat(TokenKind::DoubleColon) {
                    segments.push(self.parse_ident("a function name")?);
                }
                Expr::Var(som_ast::Path { segments })
            }
            TokenKind::Ident | TokenKind::SelfType => {
                // `Name { field: ...` or `Name {}` is a constructor, and so is
                // `module::Name { ... }` and `Self { ... }`; `name { ... }` is not.
                let mut n = 1;
                while self.peek_nth(n) == TokenKind::DoubleColon
                    && self.peek_nth(n + 1) == TokenKind::Ident
                {
                    n += 2;
                }
                // `Pair<i32> { … }`: type arguments before the brace.
                if self.peek_nth(n) == TokenKind::LessThan {
                    n = self.past_type_args(n).unwrap_or(n);
                }
                // `Pair { a, b: 2 }`: `a` short for `a: a`. Only for a capitalised name, which
                // a struct's is: `for x in items { x }` is a loop.
                let capitalised = (n.checked_sub(1).and_then(|last| self.ident_ahead(last)))
                    .is_some_and(|name| name.starts_with(|c: char| c.is_uppercase()));
                let shorthand = capitalised
                    && self.peek_nth(n + 1) == TokenKind::Ident
                    && matches!(
                        self.peek_nth(n + 2),
                        TokenKind::Comma | TokenKind::CloseBrace
                    );
                let is_constructor = self.peek_nth(n) == TokenKind::OpenBrace
                    && (self.peek_nth(n + 1) == TokenKind::CloseBrace
                        || shorthand
                        || (self.peek_nth(n + 1) == TokenKind::Ident
                            && self.peek_nth(n + 2) == TokenKind::Colon));
                if is_constructor && !self.in_condition {
                    return self.parse_constructor();
                }
                let mut path = self.parse_path("an identifier or constructor name")?;
                // `Alloc<i32>::with_capacity(10)`: the type's arguments, then its function's
                // name. Only when `::` follows the `>`, so a comparison stays one.
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
                // `null<i32>()`: type arguments, then the call. Only when `(` follows the `>`,
                // so `a < b` and `a < b > c` stay comparisons.
                let type_args = self.at(TokenKind::LessThan)
                    && self
                        .past_type_args(0)
                        .is_some_and(|n| self.peek_nth(n) == TokenKind::OpenParen);
                if type_args || !owner_args.is_empty() {
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
                }
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.with_condition(false, |p| p.parse_expr())?;
                self.expect(TokenKind::CloseParen, "closing parenthesis")?;
                return Some(inner);
            }

            TokenKind::OpenBrace => return self.with_condition(false, |p| p.parse_block()),

            TokenKind::Match => return self.parse_match(),
            // `if c { a } else { b }`, giving a value.
            TokenKind::If => {
                let parts = self.parse_if_parts()?;
                return Some(self.if_expr(parts));
            }

            // `foo(i32)`: a type where a value goes.
            kind if crate::ty::primitive(kind).is_some() => {
                if !self.in_recovery {
                    self.error_at(
                        start.clone(),
                        msg!["expected a value, found type ", code(kind.to_string())],
                        "this is a type, where a value goes",
                    );
                }
                self.in_recovery = true;
                // Past it, so whatever reads a list of values goes on.
                self.advance();
                return Some(self.ast.exprs.alloc(Expr::Hole, start));
            }

            _ => {
                self.error_missing("expected expression", "an expression");
                return Some(self.ast.exprs.alloc(Expr::Hole, start));
            }
        };

        Some(self.alloc_expr(expr, &start))
    }

    /// Whether an expression can start here: after `..`, whether a range has an end.
    fn can_start_expr(&self) -> bool {
        !matches!(
            self.peek(),
            TokenKind::CloseParen
                | TokenKind::CloseBracket
                | TokenKind::CloseBrace
                | TokenKind::OpenBrace
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::Else
                | TokenKind::Eof
        )
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
        match crate::int_value(&text) {
            Some(count) => Some(count),
            None => {
                self.error_at(span, "count is too large", "this does not fit in 128 bits");
                None
            }
        }
    }

    /// `"total: {n}"`: the text in pieces, each `{…}` read as an expression. `{{` and `}}`
    /// are the braces themselves.
    fn interpolated(&mut self, token: &som_lexer::Token) -> Expr {
        let raw = &token.text[1..token.text.len() - 1];
        // Where the text inside the quotes starts in the file.
        let base = token.span.start_offset + 1;
        let mut parts = Vec::new();
        let mut piece = String::new();
        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                // An escape is text, `\u{41}`'s braces too: unescaped with the rest of it.
                b'\\' => {
                    let len = escape_len(&raw[i..]);
                    piece.push_str(&raw[i..i + len]);
                    i += len;
                }
                b'{' if bytes.get(i + 1) == Some(&b'{') => {
                    piece.push('{');
                    i += 2;
                }
                b'}' if bytes.get(i + 1) == Some(&b'}') => {
                    piece.push('}');
                    i += 2;
                }
                b'{' => {
                    let Some(close) = raw[i..].find('}').map(|c| i + c) else {
                        let at = Span::from_range(base + i..base + i + 1, token.span.source.clone());
                        self.error_at(
                            at,
                            msg!["a ", code("{"), " in text starts a value, and needs a ", code("}")],
                            msg!["for the brace itself, write ", code("{{")],
                        );
                        return Expr::Hole;
                    };
                    if !piece.is_empty() {
                        let text = self.unescape(&std::mem::take(&mut piece), &token.span);
                        parts.push(Part::Text(self.ast.exprs.alloc(Expr::Str(text.into()), token.span.clone())));
                    }
                    let range = base + i + 1..base + close;
                    match self.text_value(&token.span, range) {
                        Some(value) => parts.push(Part::Value(value)),
                        None => return Expr::Hole,
                    }
                    i = close + 1;
                }
                b'}' => {
                    let at = Span::from_range(base + i..base + i + 1, token.span.source.clone());
                    self.error_at(
                        at,
                        msg!["a ", code("}"), " in text ends a value, and there's none"],
                        msg!["for the brace itself, write ", code("}}")],
                    );
                    return Expr::Hole;
                }
                _ => {
                    let len = raw[i..].chars().next().map_or(1, char::len_utf8);
                    piece.push_str(&raw[i..i + len]);
                    i += len;
                }
            }
        }
        // Only text after all, `"{{}}"` or `"\u{41}"`: plain text.
        if parts.is_empty() {
            return Expr::Str(self.unescape(&piece, &token.span).into());
        }
        if !piece.is_empty() {
            let text = self.unescape(&piece, &token.span);
            parts.push(Part::Text(self.ast.exprs.alloc(Expr::Str(text.into()), token.span.clone())));
        }
        Expr::Interpolated(parts)
    }

    /// The expression `{…}` in text holds, at `range` of the file: read on its own.
    fn text_value(&mut self, text: &Span, range: std::ops::Range<usize>) -> Option<Id<Expr>> {
        let tokens = som_lexer::lex_range(text.source.clone(), range.clone());
        let outer = (std::mem::replace(&mut self.tokens, tokens), std::mem::replace(&mut self.pos, 0));
        self.skip_trivia();
        let value = if self.at_eof() {
            let at = Span::from_range(range.start - 1..range.end + 1, text.source.clone());
            self.error_at(
                at,
                msg![code("{}"), " in text holds nothing"],
                msg!["put the value to write between them, ", code("{total}"), ", or write ", code("{{}}"), " for the braces"],
            );
            None
        } else {
            let value = self.parse_expr();
            if value.is_some() && !self.at_eof() {
                let at = self.current_span();
                self.error_at(at, "expected the end of the value", msg!["a value in text ends with ", code("}")]);
            }
            value
        };
        (self.tokens, self.pos) = outer;
        value
    }

    /// The text of a string literal with its escapes resolved: `\n`, `\t`, `\r`, `\0`,
    /// `\\`, `\"`, `\'`, `\x1b` and `\u{1F600}`. An unknown escape is reported, and kept as
    /// written.
    fn unescape(&mut self, raw: &str, span: &Span) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(c @ ('\\' | '"' | '\'')) => out.push(c),
                // `\x1b`: the character with that number, two hex digits, up to `\x7f`.
                Some('x') => {
                    let digits: String = chars.clone().take(2).collect();
                    let value = (digits.len() == 2)
                        .then(|| u8::from_str_radix(&digits, 16).ok())
                        .flatten();
                    match value {
                        Some(byte) if byte <= 0x7f => {
                            chars.nth(1);
                            out.push(byte as char);
                        }
                        Some(_) => {
                            chars.nth(1);
                            self.error_at(
                                span.clone(),
                                msg!["escape ", code(format!("\\x{digits}")), " is past ", code("\\x7f")],
                                msg![
                                    "text is UTF-8: for a character past ",
                                    code("\\x7f"),
                                    ", write ",
                                    code(format!("\\u{{{digits}}}"))
                                ],
                            );
                        }
                        None => self.error_at(
                            span.clone(),
                            msg![code("\\x"), " needs two hex digits"],
                            msg!["like ", code("\\x1b"), " for escape, or ", code("\\x00")],
                        ),
                    }
                }
                // `\u{1F600}`: the character with that number, in hex.
                Some('u') => {
                    let rest = chars.as_str();
                    let digits = (rest.strip_prefix('{'))
                        .and_then(|r| r.find('}').map(|end| &r[..end]))
                        .filter(|d| (1..=6).contains(&d.len()));
                    let character = digits
                        .and_then(|d| u32::from_str_radix(d, 16).ok())
                        .and_then(char::from_u32);
                    match (digits, character) {
                        (Some(d), Some(c)) => {
                            chars.nth(d.len() + 1);
                            out.push(c);
                        }
                        (Some(d), None) => {
                            chars.nth(d.len() + 1);
                            self.error_at(
                                span.clone(),
                                msg![code(format!("\\u{{{d}}}")), " isn't a character"],
                                "a character's number is at most 10FFFF, and not D800 to DFFF",
                            );
                        }
                        (None, _) => self.error_at(
                            span.clone(),
                            msg![code("\\u"), " needs a character's number in braces"],
                            msg!["like ", code("\\u{1F600}"), ", in hex"],
                        ),
                    }
                }
                other => {
                    let escape = format!("\\{}", other.map(String::from).unwrap_or_default());
                    self.error_at(
                        span.clone(),
                        msg!["unknown escape ", code(&escape)],
                        msg!["expected one of ", code("\\n \\t \\r \\0 \\\\ \\\" \\' \\x1b \\u{1F600}")],
                    );
                    out.push_str(&escape);
                }
            }
        }
        out
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
    fn parse_args(&mut self) -> Option<Vec<Id<Expr>>> {
        self.expect(TokenKind::OpenParen, "a list of arguments")?;
        let args = self.with_condition(false, |p| {
            p.parse_comma_list(TokenKind::CloseParen, |p| p.parse_arg())
        })?;
        // `free(alloc(10).` while typing: report the missing `)`, but keep the call and
        // its arguments, so an editor still knows them.
        if !self.eat(TokenKind::CloseParen) {
            self.error_expected(
                &[TokenKind::CloseParen],
                "a closing parenthesis after the arguments",
            );
        }
        Some(args)
    }

    /// Only named functions can be called: `foo(..)` or `math::add(..)`.
    fn parse_call(&mut self, callee: Id<Expr>, start: &Span) -> Option<Id<Expr>> {
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
                self.error_missing("expected function name", "a function name");
                Expr::Hole
            }
        };
        // The callee was parsed as a variable; turn that node into the call, so no
        // unreferenced node is left behind.
        let span = self.span_from(start);
        self.ast.exprs.replace(callee, expr, span);
        Some(callee)
    }

    /// `end + 1`: where an inclusive range's end makes it stop, as a range's end does.
    fn past(&mut self, end: Id<Expr>) -> Id<Expr> {
        let span = self.ast.exprs.span(end).clone();
        let one = self.ast.exprs.alloc(Expr::Int(1), span.clone());
        self.ast.exprs.alloc(
            Expr::Binary {
                op: BinOp::Add,
                lhs: end,
                rhs: one,
            },
            span,
        )
    }

    /// A copy of place `expr`, to read it as well as assign it: the `x` in `x = x + 1` that
    /// `x += 1` is. A variable, a field, a dereference or an element whose index is a
    /// variable or number. `None` for anything else, which could do something twice.
    fn copy_place(&mut self, expr: Id<Expr>) -> Option<Id<Expr>> {
        let span = self.ast.exprs.span(expr).clone();
        let copy = match &self.ast.exprs[expr] {
            Expr::Var(path) => Expr::Var(path.clone()),
            Expr::Int(n) => Expr::Int(*n),
            Expr::Field { object, field } => {
                let (object, field) = (*object, field.clone());
                Expr::Field {
                    object: self.copy_place(object)?,
                    field,
                }
            }
            Expr::Deref(inner) => {
                let inner = *inner;
                Expr::Deref(self.copy_place(inner)?)
            }
            Expr::Index { object, index } => {
                let (object, index) = (*object, *index);
                if !matches!(self.ast.exprs[index], Expr::Var(_) | Expr::Int(_)) {
                    return None;
                }
                Expr::Index {
                    object: self.copy_place(object)?,
                    index: self.copy_place(index)?,
                }
            }
            _ => return None,
        };
        Some(self.ast.exprs.alloc(copy, span))
    }

    /// `{ stmts; value }`
    pub(crate) fn parse_block(&mut self) -> Option<Id<Expr>> {
        let block = self.parse_block_parts("the block")?;
        Some(self.block_expr(block))
    }

    /// How far ahead the `>` closing type arguments that open at `n` (a `<`) is, plus one; `None`
    /// if what's there can't be type arguments, as in `a < b`. Only looks ahead.
    fn past_type_args(&self, mut n: usize) -> Option<usize> {
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
                | TokenKind::I8
                | TokenKind::I16
                | TokenKind::I32
                | TokenKind::I64
                | TokenKind::I128
                | TokenKind::ISize
                | TokenKind::U8
                | TokenKind::U16
                | TokenKind::U32
                | TokenKind::U64
                | TokenKind::U128
                | TokenKind::USize
                | TokenKind::F32
                | TokenKind::F64
                | TokenKind::Bool
                | TokenKind::Char
                | TokenKind::Str
                | TokenKind::CStr => {}
                _ => return None,
            }
            n += 1;
        }
    }

    /// `match scrutinee { pattern => value, … }`. An arm whose value is a block needs no comma
    /// after it.
    pub(crate) fn parse_match(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::Match, "a match")?;
        let scrutinee = self.parse_condition()?;
        self.expect(TokenKind::OpenBrace, "the match arms")?;
        let mut arms = Vec::new();
        // `@for (i, v) in Kind { v => i, }`: written out once `Kind` is known, before the arm
        // at the index each has.
        let mut reflected = Vec::new();
        self.with_condition(false, |p| {
            while !p.at(TokenKind::CloseBrace) && !p.at_eof() {
                if p.at_reflected_for() {
                    let start = p.current_span();
                    match p.parse_reflected_for() {
                        Some(found) => reflected.push((arms.len(), found)),
                        // Reported: an arm matching anything stands in, so nothing else is
                        // said of a `match` it leaves without arms.
                        None => {
                            let span = p.span_from(&start);
                            let pattern = p.ast.patterns.alloc(Pattern::Wildcard, span.clone());
                            let body = p.ast.exprs.alloc(Expr::Hole, span);
                            arms.push(Arm { pattern, body });
                        }
                    }
                    p.eat(TokenKind::Comma);
                    continue;
                }
                // `@for x in [a, b] { x => … }`: written out here.
                if p.at(TokenKind::Builtin) && &*p.peek_token().text == "@for" {
                    p.expand_for();
                    continue;
                }
                let pattern = p.parse_pattern()?;
                p.expect(TokenKind::FatArrow, "`=>` and the arm's value")?;
                let body = p.parse_expr()?;
                arms.push(Arm { pattern, body });
                let ends_in_block = matches!(p.ast.exprs[body], Expr::Block { .. });
                if !p.eat(TokenKind::Comma) && !ends_in_block {
                    break;
                }
            }
            Some(())
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after the match arms",
        )?;
        let scrutinee_and_arms = Expr::Match { scrutinee, arms };
        let id = self.alloc_expr(scrutinee_and_arms, &start);
        for (at, (names, of, body)) in reflected {
            self.ast.reflected.push(som_ast::Reflected {
                module: self.module,
                names,
                of,
                body,
                into: som_ast::ReflectedInto::Arms { expr: id, at },
            });
        }
        Some(id)
    }

    /// A `match` arm's pattern: `_`, `name`, `3`, `true`, `"yes"`, `none`, `Circle(r)`,
    /// `Rect { w, h }` or `Shape::Empty`.
    pub(crate) fn parse_pattern(&mut self) -> Option<Id<Pattern>> {
        let start = self.current_span();
        let pattern = match self.peek() {
            TokenKind::Int | TokenKind::Minus => {
                let negative = self.eat(TokenKind::Minus);
                let span = self.current_span();
                let count = self.parse_count("a number")?;
                match i128::try_from(count) {
                    Ok(n) => Pattern::Int(if negative { -n } else { n }),
                    Err(_) => {
                        self.error_at(
                            span,
                            "integer literal is too large",
                            "this does not fit in 128 bits",
                        );
                        Pattern::Wildcard
                    }
                }
            }
            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Pattern::Bool(value)
            }
            // `"yes"`: plain text. A value in it, `"{x}"`, isn't known until the program runs.
            TokenKind::Text => {
                let token = self.peek_token().clone();
                self.advance();
                let raw = &token.text[1..token.text.len() - 1];
                let text = match raw.contains('{') || raw.contains('}') {
                    true => self.interpolated(&token),
                    false => Expr::Str(self.unescape(raw, &token.span).into()),
                };
                match text {
                    Expr::Str(text) => Pattern::Str(text),
                    _ => {
                        self.error_at(
                            token.span.clone(),
                            "a pattern's text can't hold values",
                            msg!["for a brace itself, write ", code("{{"), " or ", code("}}")],
                        );
                        Pattern::Wildcard
                    }
                }
            }
            TokenKind::None => {
                self.advance();
                Pattern::None
            }
            TokenKind::Mut => {
                self.advance();
                let name = self.parse_ident("a name to bind")?;
                Pattern::Binding {
                    name,
                    mutable: true,
                }
            }
            TokenKind::Ident => {
                let path = self.parse_path("a pattern")?;
                if self.eat(TokenKind::OpenParen) {
                    let fields =
                        self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_pattern())?;
                    self.expect(TokenKind::CloseParen, "a closing parenthesis")?;
                    Pattern::Variant {
                        path,
                        fields: PatternFields::Tuple(fields),
                    }
                } else if self.eat(TokenKind::OpenBrace) {
                    let fields = self.parse_named_field_patterns()?;
                    Pattern::Variant { path, fields }
                } else if path.segments.len() > 1 {
                    Pattern::Variant {
                        path,
                        fields: PatternFields::Unit,
                    }
                } else if &*path.segments[0].name == "_" {
                    Pattern::Wildcard
                } else {
                    let name = path
                        .segments
                        .into_iter()
                        .next()
                        .expect("paths aren't empty");
                    Pattern::Binding {
                        name,
                        mutable: false,
                    }
                }
            }
            _ => {
                self.error_missing("expected a pattern", "a pattern");
                return None;
            }
        };
        let span = self.span_from(&start);
        Some(self.ast.patterns.alloc(pattern, span))
    }

    /// `w, h: height, .. }` after the `{` of a variant pattern.
    fn parse_named_field_patterns(&mut self) -> Option<PatternFields> {
        let mut fields = Vec::new();
        let mut rest = false;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.eat(TokenKind::DotDot) {
                rest = true;
                break;
            }
            let name: Ident = self.parse_ident("a field name")?;
            let pattern = if self.eat(TokenKind::Colon) {
                self.parse_pattern()?
            } else {
                // `w` is short for `w: w`.
                let binding = Pattern::Binding {
                    name: name.clone(),
                    mutable: false,
                };
                self.ast.patterns.alloc(binding, name.span.clone())
            };
            fields.push((name, pattern));
            if !self.list_continues(TokenKind::CloseBrace) {
                break;
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
        Some(PatternFields::Named { fields, rest })
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
                // `a` is short for `a: a`.
                let path = som_ast::Path {
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
}

/// How long the escape at the start of `text`, a `\`, is as written: `\n` is 2, `\u{41}` 6.
pub(crate) fn escape_len(text: &str) -> usize {
    match text[1..].chars().next() {
        Some('u') if text[2..].starts_with('{') => text.find('}').map_or(2, |end| end + 1),
        Some(c) => 1 + c.len_utf8(),
        None => 1,
    }
}

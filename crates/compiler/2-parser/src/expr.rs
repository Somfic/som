use som_ast::{Expr, Id, Span};
use som_base::message::code;
use som_base::msg;
use som_lexer::TokenKind;

use crate::grammar::Grammar;
use crate::{Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    /// The condition of an `if` or `while`, or the optional of an `if let`: `x { … }` there
    /// is `x` followed by the body. Write `(Point { … })` for a constructor.
    pub(crate) fn parse_condition(&mut self) -> Option<Id<Expr>> {
        self.with_condition(true, |p| p.parse_expr())
    }

    fn with_condition<T>(&mut self, in_condition: bool, f: impl FnOnce(&mut Self) -> T) -> T {
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
            // `.field` or `.method(args)`
            if self.at(TokenKind::Dot) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                self.advance();
                // `alloc(10).` while typing: report the missing name, but keep what's
                // before the dot, so an editor still knows its type and can list fields.
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

            // `callee(args)`
            if self.at(TokenKind::OpenParen) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                lhs = self.parse_call(lhs, &start)?;
                continue;
            }

            // `then if condition else otherwise`
            if self.at(TokenKind::If) {
                if 1 < min_bp {
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
            if self.at(TokenKind::Else) {
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
                self.advance();
                let value = self.parse_expr_bp(Grammar::ASSIGNMENT)?;
                lhs = self.alloc_expr(Expr::Assign { target: lhs, value }, &start);
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

            let Some((op, info)) = Grammar::infix_op(self.peek()) else {
                break;
            };
            let (lbp, rbp) = info.binding_power();
            if lbp < min_bp {
                break;
            }
            self.advance();
            let rhs = self.parse_expr_bp(rbp)?;
            lhs = self.alloc_expr(Expr::Binary { op, lhs, rhs }, &start);
        }

        Some(lhs)
    }

    fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        let kind = self.peek();

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

        let Some(bp) = Grammar::prefix_bp(kind).filter(|_| kind != TokenKind::Minus) else {
            return self.parse_atom();
        };

        self.advance();
        let mutable = kind == TokenKind::Ampersand && self.eat(TokenKind::Mut);
        let expr = self.parse_expr_bp(bp)?;
        let expr = match kind {
            TokenKind::Bang => Expr::Not(expr),
            TokenKind::Ampersand => Expr::Borrow { mutable, expr },
            TokenKind::Star => Expr::Deref(expr),
            _ => unreachable!("prefix_bp only accepts prefix operators"),
        };
        Some(self.alloc_expr(expr, &start))
    }

    fn parse_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        let expr = match self.peek() {
            TokenKind::Int => {
                let text = self.peek_token().text.clone();
                self.advance();
                match text.parse() {
                    Ok(value) => Expr::Int(value),
                    Err(_) => {
                        self.error_at(
                            start.clone(),
                            "integer literal is too large",
                            "this does not fit in 64 bits",
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
                    if self.eat(TokenKind::Semicolon) {
                        let count = self.parse_count("how many copies")?;
                        self.expect(TokenKind::CloseBracket, "a closing bracket")?;
                        Expr::ArrayRepeat {
                            value: first,
                            count,
                        }
                    } else {
                        let mut items = vec![first];
                        while self.eat(TokenKind::Comma) && !self.at(TokenKind::CloseBracket) {
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

            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Expr::Bool(value)
            }

            TokenKind::Text => {
                let text = self.peek_token().text.clone();
                self.advance();
                Expr::Str(self.unescape(&text[1..text.len() - 1], &start).into())
            }

            TokenKind::Ident => {
                // `Name { field: ...` or `Name {}` is a constructor, and so is
                // `module::Name { ... }`; `name { ... }` is not.
                let mut n = 1;
                while self.peek_nth(n) == TokenKind::DoubleColon
                    && self.peek_nth(n + 1) == TokenKind::Ident
                {
                    n += 2;
                }
                let is_constructor = self.peek_nth(n) == TokenKind::OpenBrace
                    && (self.peek_nth(n + 1) == TokenKind::CloseBrace
                        || (self.peek_nth(n + 1) == TokenKind::Ident
                            && self.peek_nth(n + 2) == TokenKind::Colon));
                if is_constructor && !self.in_condition {
                    return self.parse_constructor();
                }
                Expr::Var(self.parse_path("an identifier or constructor name")?)
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.with_condition(false, |p| p.parse_expr())?;
                self.expect(TokenKind::CloseParen, "closing parenthesis")?;
                return Some(inner);
            }

            TokenKind::OpenBrace => return self.with_condition(false, |p| p.parse_block()),

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
    pub(crate) fn parse_count(&mut self, what: &str) -> Option<u64> {
        let span = self.current_span();
        if !self.at(TokenKind::Int) {
            self.error_expected(&[TokenKind::Int], what);
            return None;
        }
        let text = self.peek_token().text.clone();
        self.advance();
        match text.parse() {
            Ok(count) => Some(count),
            Err(_) => {
                self.error_at(span, "count is too large", "this does not fit in 64 bits");
                None
            }
        }
    }

    /// The text of a string literal with its escapes resolved: `\n`, `\t`, `\r`, `\0`,
    /// `\\`, `\"` and `\'`. An unknown escape is reported, and kept as written.
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
                other => {
                    let escape = format!("\\{}", other.map(String::from).unwrap_or_default());
                    self.error_at(
                        span.clone(),
                        msg!["unknown escape ", code(&escape)],
                        msg!["expected one of ", code("\\n \\t \\r \\0 \\\\ \\\" \\'")],
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
                args,
            },
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

    /// `{ stmts; value }`
    pub(crate) fn parse_block(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::OpenBrace, "an opening brace for the block")?;

        let mut stmts = Vec::new();
        let mut value = None;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) if self.at(TokenKind::CloseBrace) => value = Some(expr),
                StmtOrExpr::Expr(_) => {
                    self.error_missing("expected `;` or `}`", "a semicolon or closing brace");
                    break;
                }
                StmtOrExpr::Error => self.recover(RecoveryLevel::Statement),
            }
        }

        self.expect(TokenKind::CloseBrace, "a closing brace after the block")?;
        Some(self.alloc_expr(Expr::Block { stmts, value }, &start))
    }

    /// `Point { x: 1, y: 2 }`
    fn parse_constructor(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        let ty = self.parse_path("a constructor name")?;

        self.expect(TokenKind::OpenBrace, "constructor fields")?;
        let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
            let name = p.parse_ident("a constructor field name")?;
            p.expect(TokenKind::Colon, "a constructor field value")?;
            let value = p.parse_expr()?;
            Some((name, value))
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after constructor fields",
        )?;

        Some(self.alloc_expr(Expr::Construct { ty, fields }, &start))
    }
}

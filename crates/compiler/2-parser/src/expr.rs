use som_ast::{Expr, Id, Span};
use som_lexer::TokenKind;

use crate::grammar::Grammar;
use crate::{Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
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
                let name = self.parse_ident("a field or method name")?;
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
                let condition = self.parse_expr()?;
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

            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Expr::Bool(value)
            }

            TokenKind::Text => {
                let text = &self.peek_token().text;
                let unquoted = text[1..text.len() - 1].into();
                self.advance();
                Expr::Str(unquoted)
            }

            TokenKind::Ident => {
                // `Name { field: ...` or `Name {}` is a constructor; `name { ... }` is not.
                let is_constructor = self.peek_next() == TokenKind::OpenBrace
                    && (self.peek_nth(2) == TokenKind::CloseBrace
                        || (self.peek_nth(2) == TokenKind::Ident
                            && self.peek_nth(3) == TokenKind::Colon));
                if is_constructor {
                    return self.parse_constructor();
                }
                Expr::Var(self.parse_path("an identifier or constructor name")?)
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(TokenKind::CloseParen, "closing parenthesis")?;
                return Some(inner);
            }

            TokenKind::OpenBrace => return self.parse_block(),

            _ => {
                self.error_missing("expected expression", "an expression");
                return Some(self.ast.exprs.alloc(Expr::Hole, start));
            }
        };

        Some(self.alloc_expr(expr, &start))
    }

    /// `(a, b, c)`
    fn parse_args(&mut self) -> Option<Vec<Id<Expr>>> {
        self.expect(TokenKind::OpenParen, "a list of arguments")?;
        let args = self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_expr())?;
        self.expect(
            TokenKind::CloseParen,
            "a closing parenthesis after the arguments",
        )?;
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

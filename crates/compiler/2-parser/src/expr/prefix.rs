use som_ast::{Const, Expr, Id, Ident, Path, Span, Visibility};
use som_lang::Builtin;
use som_lexer::TokenKind;

use crate::Parser;
use crate::grammar::{Bp, is_prefix_op};

impl Parser<'_> {
    pub(super) fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        if let Some(params_end) = self.closure_ahead() {
            return self.parse_closure(params_end, &start);
        }
        // Another language's anonymous function stops the expression; `new Point(…)` and
        // `not x` are read as what they mean.
        if self.foreign_lambda() || self.at(TokenKind::OpenParen) && self.foreign_lambda_params() {
            return Some(self.ast.exprs.alloc(Expr::Hole, start));
        }
        self.eat_foreign_new();
        self.rewrite_foreign_not();
        match self.peek() {
            TokenKind::AndAnd => self.parse_double_borrow(&start),
            TokenKind::DotDot | TokenKind::DotDotEquals => {
                let end = self.parse_range_end()?;
                Some(self.alloc_expr(Expr::Range { start: None, end }, &start))
            }
            _ if self.at_comptime_call(0) => self.parse_comptime_call(&start),
            _ if self.at_builtin(Builtin::Const) => self.parse_const_expr(&start),
            kind if is_prefix_op(kind) => self.parse_unary(kind, &start),
            _ => self.parse_atom(),
        }
    }

    /// How far ahead the `=>` of an anonymous function starting here is: `x =>`, or `(…) =>`.
    fn closure_ahead(&self) -> Option<usize> {
        match self.peek() {
            TokenKind::Ident if self.peek_nth(1) == TokenKind::FatArrow => Some(1),
            TokenKind::OpenParen => {
                let mut n = 1;
                let mut depth = 1;
                while depth > 0 {
                    match self.peek_nth(n) {
                        TokenKind::OpenParen => depth += 1,
                        TokenKind::CloseParen => depth -= 1,
                        TokenKind::Eof => return None,
                        _ => {}
                    }
                    n += 1;
                }
                (self.peek_nth(n) == TokenKind::FatArrow).then_some(n)
            }
            _ => None,
        }
    }

    /// `x => body` or `(a, b: i32) => body`, the `=>` `arrow` tokens ahead.
    fn parse_closure(&mut self, arrow: usize, start: &Span) -> Option<Id<Expr>> {
        let params = if arrow == 1 && self.at(TokenKind::Ident) {
            let name = self.parse_ident("a parameter name")?;
            vec![closure_param(name, None)]
        } else {
            self.expect(TokenKind::OpenParen, "the parameters")?;
            let params = self.parse_comma_list(TokenKind::CloseParen, |p| {
                let name = p.parse_ident("a parameter name")?;
                let ty = match p.eat(TokenKind::Colon) {
                    true => Some(p.parse_type()?),
                    false => None,
                };
                Some(closure_param(name, ty))
            })?;
            self.expect(TokenKind::CloseParen, "a closing parenthesis after the parameters")?;
            params
        };
        self.expect(TokenKind::FatArrow, "`=>` and the body")?;
        let body = self.parse_expr()?;
        Some(self.alloc_expr(Expr::Closure { params, body }, start))
    }

    /// `&&x`: a borrow of a borrow, lexed as one `&&`.
    fn parse_double_borrow(&mut self, start: &Span) -> Option<Id<Expr>> {
        self.advance();
        let mut expr = self.parse_expr_bp(Bp::PREFIX)?;
        for _ in 0..2 {
            let borrow = Expr::Borrow {
                mutable: false,
                expr,
            };
            expr = self.alloc_expr(borrow, start);
        }
        Some(expr)
    }

    fn parse_unary(&mut self, kind: TokenKind, start: &Span) -> Option<Id<Expr>> {
        self.advance();
        let mutable = kind == TokenKind::Ampersand && self.eat(TokenKind::Mut);
        let expr = self.parse_expr_bp(Bp::PREFIX)?;
        let expr = match kind {
            TokenKind::Bang => Expr::Not(expr),
            TokenKind::Tilde => Expr::BitNot(expr),
            TokenKind::Ampersand => Expr::Borrow { mutable, expr },
            TokenKind::Star => Expr::Deref(expr),
            TokenKind::Minus => self.negated(expr, start),
            _ => unreachable!("only prefix operators get here"),
        };
        Some(self.alloc_expr(expr, start))
    }

    /// `-1` is the literal `-1`, not `1` negated: so it can be the smallest `i64`, and a
    /// `u8` can't be it.
    fn negated(&self, expr: Id<Expr>, minus: &Span) -> Expr {
        let literal = self.ast.exprs.span(expr).start_offset == minus.start_offset + 1;
        match self.ast.exprs[expr] {
            Expr::Int(n) if literal => Expr::Int(n.wrapping_neg()),
            Expr::Float(x) if literal => Expr::Float(-x),
            _ => Expr::Neg(expr),
        }
    }

    /// At `@name(` or `@name::`, `at` tokens ahead, where `@name` isn't a built-in: a call
    /// made while compiling.
    pub(crate) fn at_comptime_call(&self, at: usize) -> bool {
        // `@make_identity<N>(`: type arguments, then the call.
        let generic = self.peek_nth(at + 1) == TokenKind::LessThan
            && (self.past_type_args(at + 1))
                .is_some_and(|n| self.peek_nth(n) == TokenKind::OpenParen);
        self.peek_nth(at) == TokenKind::Builtin
            && (generic
                || matches!(
                    self.peek_nth(at + 1),
                    TokenKind::OpenParen | TokenKind::DoubleColon
                ))
            && self
                .ahead(at)
                .is_some_and(|t| Builtin::from_name(&t.text).is_none())
    }

    /// `@kilobytes(4)`, `@som::assert(…)`: a function called while compiling, like
    /// `@const kilobytes(4)`.
    fn parse_comptime_call(&mut self, start: &Span) -> Option<Id<Expr>> {
        let name = self.peek_token().text.clone();
        let first = Ident {
            name: name[1..].into(),
            span: self.current_span(),
        };
        self.advance();
        let mut segments = vec![first];
        while self.eat(TokenKind::DoubleColon) {
            segments.push(self.parse_ident("a name after `::`")?);
        }
        let type_args = match self.at(TokenKind::LessThan) {
            true => self.parse_type_args()?,
            false => Vec::new(),
        };
        let args = self.parse_args()?;
        let call = Expr::Call {
            callee: Path { segments },
            owner_args: Vec::new(),
            type_args,
            args,
        };
        let call = self.alloc_expr(call, start);
        Some(self.in_place_const(call, start))
    }

    /// `@const f(x)`, or `@const { … }`: binds like `-`, so `@const f(x) + 1` adds to it.
    fn parse_const_expr(&mut self, start: &Span) -> Option<Id<Expr>> {
        self.advance();
        let value = match self.at(TokenKind::OpenBrace) {
            true => self.parse_block()?,
            false => self.parse_expr_bp(Bp::PREFIX)?,
        };
        Some(self.in_place_const(value, start))
    }

    /// `value`, worked out while compiling where it's written, from `start`.
    fn in_place_const(&mut self, value: Id<Expr>, start: &Span) -> Id<Expr> {
        let span = self.span_from(start);
        let name = Ident {
            name: Const::UNNAMED.into(),
            span: start.clone(),
        };
        let c = Const {
            doc: None,
            visibility: Visibility::Private,
            name,
            ty: None,
            value,
            in_place: true,
        };
        let c = self.ast.consts.alloc(c, span);
        self.alloc_expr(Expr::InPlace(c), start)
    }
}

/// A parameter of an anonymous function: never `out`, and only changed through a reference.
fn closure_param(name: som_ast::Ident, ty: Option<Id<som_ast::TypeExpr>>) -> som_ast::Param {
    som_ast::Param {
        name,
        ty,
        out: false,
        mutable: false,
    }
}

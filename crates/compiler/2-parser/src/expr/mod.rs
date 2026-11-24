//! Expressions: a Pratt parser over the operators, with prefixes and atoms below it.

mod atom;
mod pattern;
mod prefix;
mod text;

use som_ast::{BinOp, Expr, Id, Pattern, Span};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::Parser;
use crate::grammar::{self, Bp};

/// What reading an operator after an expression came to.
enum Step {
    /// The expression it made, to read on after.
    Next(Id<Expr>),
    /// It binds too loosely, or isn't an operator: the expression ends.
    Stop,
}

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    /// The condition of an `if` or `while`, or the optional of an `if let`: `x { … }` there
    /// is `x` followed by the body. Write `(Point { … })` for a constructor.
    pub(crate) fn parse_condition(&mut self) -> Option<Id<Expr>> {
        self.with_condition(true, |p| p.parse_expr())
    }

    /// An `@if`'s condition, which can compare types: `N >= 3`, `T == f32`.
    pub(crate) fn parse_settled_condition(&mut self) -> Option<Id<Expr>> {
        self.with_flag(|c| &mut c.settled, true, |p| p.parse_condition())
    }

    pub(crate) fn alloc_expr(&mut self, expr: Expr, start: &Span) -> Id<Expr> {
        let span = self.span_from(start);
        self.ast.exprs.alloc(expr, span)
    }

    /// An expression whose operators all bind at least `min_bp` tightly.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
        let start = self.current_span();
        let mut lhs = self.parse_prefix_or_atom()?;
        loop {
            // `x++`, ending a statement: reported, and read past.
            if self.eat_foreign_step(lhs) {
                continue;
            }
            // `a and b`, `a === b`: read as som's operators.
            self.rewrite_foreign_binary_op();
            match self.parse_operator(lhs, &start, min_bp)? {
                Step::Next(expr) => lhs = expr,
                Step::Stop => break,
            }
        }
        Some(lhs)
    }

    fn parse_operator(&mut self, lhs: Id<Expr>, start: &Span, min_bp: u8) -> Option<Step> {
        let binds = |left: u8| left >= min_bp;
        match self.peek() {
            TokenKind::Dot | TokenKind::OpenBracket | TokenKind::OpenParen
                if !binds(Bp::POSTFIX) =>
            {
                Some(Step::Stop)
            }
            TokenKind::Dot => self.parse_member(lhs, start),
            TokenKind::OpenBracket => {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::CloseBracket, "a closing bracket after the index")?;
                Some(Step::Next(
                    self.alloc_expr(Expr::Index { object: lhs, index }, start),
                ))
            }
            TokenKind::OpenParen => self.parse_call(lhs, start).map(Step::Next),
            // `value?`: what a `T!` holds, or the function fails with what it failed with.
            TokenKind::Question if !binds(Bp::POSTFIX) => Some(Step::Stop),
            TokenKind::Question => {
                self.advance();
                Some(Step::Next(self.alloc_expr(Expr::Propagate(lhs), start)))
            }
            // `then if condition else otherwise`, on one line: an `if` starting a line is a
            // statement of its own, after a forgotten `;`.
            TokenKind::If if !binds(Bp::CONDITIONAL.left) || self.on_new_line() => Some(Step::Stop),
            TokenKind::If => self.parse_conditional(lhs, start),
            // In a body without braces, an `else` starting a line continues the `if`.
            TokenKind::Else if self.ctx.foreign_depth > 0 && self.on_new_line() => Some(Step::Stop),
            TokenKind::Else if !binds(Bp::FALLBACK.left) => Some(Step::Stop),
            // `parse(text) else e => …`: what to do with why it failed.
            TokenKind::Else
                if self.peek_nth(1) == TokenKind::Ident && self.peek_nth(2) == TokenKind::FatArrow =>
            {
                self.advance();
                let name = self.parse_ident("a name for why it failed")?;
                let span = name.span.clone();
                let error = (self.ast.patterns).alloc(Pattern::Binding { name, mutable: false }, span);
                self.advance();
                let body = self.parse_expr_bp(Bp::FALLBACK.right)?;
                let expr = Expr::Recover {
                    failable: lhs,
                    error,
                    body,
                };
                Some(Step::Next(self.alloc_expr(expr, start)))
            }
            TokenKind::Else => {
                self.advance();
                let fallback = self.parse_expr_bp(Bp::FALLBACK.right)?;
                let expr = Expr::Fallback {
                    optional: lhs,
                    fallback,
                };
                Some(Step::Next(self.alloc_expr(expr, start)))
            }
            TokenKind::Equals | TokenKind::PlusEquals | TokenKind::MinusEquals
                if !binds(Bp::ASSIGN.left) =>
            {
                Some(Step::Stop)
            }
            TokenKind::Equals => self.parse_assignment(lhs, start),
            TokenKind::PlusEquals => self.parse_compound_assignment(BinOp::Add, lhs, start),
            TokenKind::MinusEquals => self.parse_compound_assignment(BinOp::Subtract, lhs, start),
            TokenKind::As if !binds(Bp::AS) => Some(Step::Stop),
            TokenKind::As => {
                self.advance();
                let ty = self.parse_type()?;
                Some(Step::Next(
                    self.alloc_expr(Expr::Cast { value: lhs, ty }, start),
                ))
            }
            TokenKind::DotDot | TokenKind::DotDotEquals if !binds(Bp::RANGE.left) => {
                Some(Step::Stop)
            }
            TokenKind::DotDot | TokenKind::DotDotEquals => {
                let end = self.parse_range_end()?;
                let expr = Expr::Range {
                    start: Some(lhs),
                    end,
                };
                Some(Step::Next(self.alloc_expr(expr, start)))
            }
            _ => self.parse_binary(lhs, start, min_bp),
        }
    }

    /// `.field` or `.method(args)`.
    fn parse_member(&mut self, lhs: Id<Expr>, start: &Span) -> Option<Step> {
        self.advance();
        // `System.out`: after a `.`, the keyword `out` can only be a name.
        if self.at(TokenKind::Out) {
            self.tokens[self.pos].kind = TokenKind::Ident;
        }
        if self.at(TokenKind::Primitive(som_ast::Primitive::CStr)) {
            self.c_str_misspelt();
        }
        // `alloc(10).` while typing: report the missing name, but keep what's before the
        // dot, so an editor still knows its type and can list fields.
        let Some(name) = self.parse_ident("a field or method name") else {
            return Some(Step::Stop);
        };
        let expr = self.member(lhs, name)?;
        Some(Step::Next(self.alloc_expr(expr, start)))
    }

    /// `receiver.name`, or with arguments after it, a method call.
    fn member(&mut self, receiver: Id<Expr>, name: som_ast::Ident) -> Option<Expr> {
        if !self.at(TokenKind::OpenParen) {
            return Some(Expr::Field {
                object: receiver,
                field: name,
            });
        }
        let args = self.parse_args()?;
        Some(Expr::MethodCall {
            receiver,
            method: name,
            args,
        })
    }

    fn parse_conditional(&mut self, then: Id<Expr>, start: &Span) -> Option<Step> {
        self.advance();
        let condition = self.parse_expr_bp(Bp::CONDITION)?;
        self.expect(TokenKind::Else, "an else branch")?;
        let otherwise = self.parse_expr()?;
        let expr = Expr::Conditional {
            condition,
            then,
            otherwise,
        };
        Some(Step::Next(self.alloc_expr(expr, start)))
    }

    fn parse_assignment(&mut self, target: Id<Expr>, start: &Span) -> Option<Step> {
        // `if n = 0`: a comparison, surely, not an assignment.
        if self.ctx.condition {
            let span = self.current_span();
            self.report(
                span.clone(),
                msg![code("="), " assigns; to compare, use ", code("==")],
                "an assignment in a condition",
                Some(Fix::change_to(span, "==")),
            );
        }
        self.advance();
        let value = self.parse_expr_bp(Bp::ASSIGN.right)?;
        Some(Step::Next(
            self.alloc_expr(Expr::Assign { target, value }, start),
        ))
    }

    /// `target += value`: `target = target + value`.
    fn parse_compound_assignment(
        &mut self,
        op: BinOp,
        target: Id<Expr>,
        start: &Span,
    ) -> Option<Step> {
        let at = self.current_span();
        self.advance();
        let value = self.parse_expr_bp(Bp::ASSIGN.right)?;
        let Some(read) = self.copy_place(target) else {
            self.error_at(
                at,
                msg![
                    code(op.symbol().to_string() + "="),
                    " changes a variable, field or element"
                ],
                msg!["write it out: ", code("target = target + value")],
            );
            return Some(Step::Next(target));
        };
        let sum = Expr::Binary {
            op,
            lhs: read,
            rhs: value,
        };
        let sum = self.alloc_expr(sum, start);
        Some(Step::Next(
            self.alloc_expr(Expr::Assign { target, value: sum }, start),
        ))
    }

    fn parse_binary(&mut self, lhs: Id<Expr>, start: &Span, min_bp: u8) -> Option<Step> {
        let shift = grammar::shift_op(self.peek())
            .filter(|_| self.tokens.get(self.pos + 1).map(|t| t.kind) == Some(self.peek()));
        let (op, bp) = match shift {
            Some(op) => (op, Bp::SHIFT),
            None => match grammar::infix_op(self.peek()) {
                Some(found) => found,
                None => return Some(Step::Stop),
            },
        };
        if bp.left < min_bp {
            return Some(Step::Stop);
        }
        self.advance_by(if shift.is_some() { 2 } else { 1 });
        let rhs = self.parse_expr_bp(bp.right)?;
        Some(Step::Next(
            self.alloc_expr(Expr::Binary { op, lhs, rhs }, start),
        ))
    }

    /// After `..` or `..=`, at it: the range's end, if it has one. `..=end` goes up to and
    /// including `end`, which is `..end + 1`.
    fn parse_range_end(&mut self) -> Option<Option<Id<Expr>>> {
        let inclusive = self.at(TokenKind::DotDotEquals);
        self.advance();
        if inclusive {
            let end = self.parse_expr_bp(Bp::RANGE.right)?;
            return Some(Some(self.past(end)));
        }
        match self.can_start_expr() {
            true => Some(Some(self.parse_expr_bp(Bp::RANGE.right)?)),
            false => Some(None),
        }
    }

    fn can_start_expr(&self) -> bool {
        !self.at_any(grammar::ENDS_EXPR)
    }

    /// `end + 1`: where an inclusive range's end makes it stop, as a range's end does.
    fn past(&mut self, end: Id<Expr>) -> Id<Expr> {
        let span = self.ast.exprs.span(end).clone();
        let one = self.ast.exprs.alloc(Expr::Int(1), span.clone());
        let sum = Expr::Binary {
            op: BinOp::Add,
            lhs: end,
            rhs: one,
        };
        self.ast.exprs.alloc(sum, span)
    }

    /// A copy of place `expr`, to read it as well as assign it: the `x` in `x = x + 1` that
    /// `x += 1` is. `None` for what could do something twice: only a variable, a field, a
    /// dereference or an element whose index is a variable or number is copied.
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

    pub(crate) fn parse_block(&mut self) -> Option<Id<Expr>> {
        let block = self.parse_block_parts("the block")?;
        Some(self.block_expr(block))
    }
}

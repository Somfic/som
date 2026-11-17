use som_ast::{BinOp, Expr, Id, Span};
use som_lang::LangTrait;
use som_mir::*;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    pub(super) fn is_optional(&mut self, expr: Id<Expr>) -> bool {
        let ty = self.expr_ty(expr);
        matches!(self.l.mir.types.get(ty), Ty::Optional(_))
    }

    /// `expr`, `lhs == rhs`, where at least one of them is optional. What a `none` holds is
    /// zeroed, so reading it is harmless; only the result ignores it. Values compared with
    /// `Eq`'s `eq` are compared only when both are there.
    pub(super) fn optional_equality(
        &mut self,
        expr: Id<Expr>,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
        span: Span,
    ) -> Operand {
        if self.typed.operator_trait(expr) == Some(LangTrait::Eq) {
            return self.optional_eq_call(lhs, rhs, span);
        }
        let bool = self.l.mir.types.bool;
        let (lhs_some, lhs_value) = self.equality_side(lhs);
        let (rhs_some, rhs_value) = self.equality_side(rhs);
        let compute = |b: &mut Self, rvalue| b.temp_copy(bool, rvalue, span.clone());
        let values_equal = compute(
            self,
            Rvalue::Binary {
                op: BinOp::Equals,
                lhs: lhs_value,
                rhs: rhs_value,
            },
        );
        match (lhs_some, rhs_some) {
            // Both: the same presence, and if present, the same value.
            (Some(lhs_some), Some(rhs_some)) => {
                let same_presence = compute(
                    self,
                    Rvalue::Binary {
                        op: BinOp::Equals,
                        lhs: lhs_some.clone(),
                        rhs: rhs_some,
                    },
                );
                let absent = compute(self, Rvalue::Not(lhs_some));
                let values_match = compute(
                    self,
                    Rvalue::Binary {
                        op: BinOp::Or,
                        lhs: absent,
                        rhs: values_equal,
                    },
                );
                compute(
                    self,
                    Rvalue::Binary {
                        op: BinOp::And,
                        lhs: same_presence,
                        rhs: values_match,
                    },
                )
            }
            // One: it holds a value, and that's the other side.
            (Some(some), None) | (None, Some(some)) => compute(
                self,
                Rvalue::Binary {
                    op: BinOp::And,
                    lhs: some,
                    rhs: values_equal,
                },
            ),
            (None, None) => unreachable!("one side is optional"),
        }
    }

    /// `lhs == rhs` of values compared with `Eq`'s `eq`, as `string`s are, where one or both
    /// are optional: `eq` is called only when both hold one, since a `none`'s zeroes aren't a
    /// value it could read. Otherwise they're equal if neither holds one.
    fn optional_eq_call(&mut self, lhs: Id<Expr>, rhs: Id<Expr>, span: Span) -> Operand {
        let bool = self.l.mir.types.bool;
        let mut present = Vec::new();
        let mut values = Vec::new();
        let mut value_ty = None;
        for side in [lhs, rhs] {
            let ty = self.expr_ty(side);
            let place = self.place(side);
            match *self.l.mir.types.get(ty) {
                Ty::Optional(inner) => {
                    present.push(Operand::Copy(place.clone().project(Projection::Field(0))));
                    values.push(place.project(Projection::Field(1)));
                    value_ty = Some(inner);
                }
                _ => {
                    values.push(place);
                    value_ty = value_ty.or(Some(ty));
                }
            }
        }
        let value_ty = value_ty.expect("two sides");
        let compute = |b: &mut Self, rvalue| b.temp_copy(bool, rvalue, span.clone());
        // Whether `eq` is called, and what they are if it isn't.
        let (both, otherwise) = match &present[..] {
            [l, r] => {
                let both = compute(
                    self,
                    Rvalue::Binary {
                        op: BinOp::And,
                        lhs: l.clone(),
                        rhs: r.clone(),
                    },
                );
                let either = compute(
                    self,
                    Rvalue::Binary {
                        op: BinOp::Or,
                        lhs: l.clone(),
                        rhs: r.clone(),
                    },
                );
                let neither = compute(self, Rvalue::Not(either));
                (both, neither)
            }
            [some] => (some.clone(), self.constant(ConstKind::Bool(false), bool)),
            _ => unreachable!("one side is optional"),
        };
        let equal = self.new_local(bool, None, true, span.clone());
        self.assign(Place::local(equal), Rvalue::Use(otherwise), span.clone());
        let (call, join) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: both,
                then: call,
                otherwise: join,
            },
            span.clone(),
        );

        self.current = call;
        let reference = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: value_ty,
        });
        let args = (values.into_iter())
            .map(|place| {
                let borrow = Rvalue::Ref {
                    mutable: false,
                    place,
                };
                self.temp_copy(reference, borrow, span.clone())
            })
            .collect();
        let method = self.l.lang_method(LangTrait::Eq);
        let func = self.l.instance(method, vec![value_ty]);
        let call = Rvalue::Call { func, args };
        self.assign(Place::local(equal), call, span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());

        self.current = join;
        Operand::Copy(Place::local(equal))
    }

    /// One side of an `==` with an optional: whether it holds a value, if it's optional, and
    /// the value.
    pub(super) fn equality_side(&mut self, expr: Id<Expr>) -> (Option<Operand>, Operand) {
        if !self.is_optional(expr) {
            return (None, self.operand(expr));
        }
        let place = self.place(expr);
        (
            Some(Operand::Copy(place.clone().project(Projection::Field(0)))),
            Operand::Copy(place.project(Projection::Field(1))),
        )
    }

    /// A `bool`, or whether an optional holds a value.
    pub(super) fn condition(&mut self, expr: Id<Expr>) -> Operand {
        if self.is_optional(expr) {
            let place = self.place(expr);
            return Operand::Copy(place.project(Projection::Field(0)));
        }
        self.operand(expr)
    }

    /// Branches on whether the optional `expr` holds a value. Returns the value, of type `ty`,
    /// to read in the first block, and the blocks for each case.
    ///
    /// Through a `&T?`, the value is a reference to what it holds, taken before the branch
    /// reads whether there is one: reading through a temporary hands its loans on, and they
    /// have to end up in the reference.
    pub(super) fn branch_on_some(
        &mut self,
        expr: Id<Expr>,
        ty: TypeId,
        span: Span,
    ) -> (Operand, BlockId, BlockId) {
        let expr_ty = self.expr_ty(expr);
        let mut place = self.place(expr);
        let by_ref = match self.l.mir.types.get(expr_ty) {
            Ty::Ref { mutable, .. } => Some(*mutable),
            _ => None,
        };
        let value = match by_ref {
            Some(mutable) => {
                place = place.project(Projection::Deref);
                let reference = self.temp(ty, span.clone());
                let borrow = Rvalue::Ref {
                    mutable,
                    place: place.clone().project(Projection::Field(1)),
                };
                self.assign(Place::local(reference), borrow, span.clone());
                Some(reference)
            }
            None => None,
        };
        let (some_block, none_block) = (self.new_block(), self.new_block());
        let condition = Operand::Copy(place.clone().project(Projection::Field(0)));
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: some_block,
                otherwise: none_block,
            },
            span.clone(),
        );
        let value = match value {
            Some(reference) => {
                // Without a value, the reference is never used: end it, and its loans.
                self.blocks[none_block].stmts.push(Statement {
                    kind: StatementKind::StorageDead(reference),
                    span,
                    reads: Vec::new(),
                });
                Operand::Move(Place::local(reference))
            }
            None => self.read(place.project(Projection::Field(1)), ty),
        };
        (value, some_block, none_block)
    }
}

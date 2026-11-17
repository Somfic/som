use som_ast::{BinOp, Expr, Id, Span};
use som_mir::*;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    pub(super) fn is_optional(&mut self, expr: Id<Expr>) -> bool {
        let ty = self.expr_ty(expr);
        matches!(self.l.mir.types.get(ty), Ty::Optional(_))
    }

    /// Whether `lhs` and `rhs`, at least one of them optional, are equal: an optional and a
    /// value when it holds that value, two optionals when neither holds one, or both hold
    /// equal ones. What a `none` holds is zeroed, so reading it is harmless; only the result
    /// ignores it.
    pub(super) fn optional_equality(
        &mut self,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
        span: Span,
    ) -> Operand {
        let bool = self.l.mir.types.bool;
        let (lhs_some, lhs_value) = self.equality_side(lhs);
        let (rhs_some, rhs_value) = self.equality_side(rhs);
        let compute = |b: &mut Self, rvalue: Rvalue| {
            let temp = b.temp(bool, span.clone());
            b.assign(Place::local(temp), rvalue, span.clone());
            Operand::Copy(Place::local(temp))
        };
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

    /// Evaluates an `if` condition: a `bool`, or whether an optional holds a value.
    pub(super) fn condition(&mut self, expr: Id<Expr>) -> Operand {
        let ty = self.expr_ty(expr);
        if matches!(self.l.mir.types.get(ty), Ty::Optional(_)) {
            let place = self.place(expr);
            return Operand::Copy(place.project(Projection::Field(0)));
        }
        self.operand(expr)
    }

    /// Evaluates the optional `expr` and branches on whether it holds a value. Returns the
    /// value, of type `ty`, to read in the first block, and the blocks for each case.
    ///
    /// Through a reference to an optional (`&T?`), the value is a reference to what it holds.
    /// That reference is taken before the branch reads whether there is a value: reading
    /// through a temporary hands its loans on, and they have to end up in the reference.
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

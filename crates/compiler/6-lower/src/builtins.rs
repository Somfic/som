use som_ast::{BinOp, Expr, Id, Span};
use som_mir::*;
use som_typeck::ArrayMethod;

use super::FnBuilder;

/// The reference a slice's (or text's) elements are reached through: `s` for `(*s)`.
pub(super) fn elements_ref(elements: &Place) -> Place {
    let mut reference = elements.clone();
    reference.projection.pop();
    reference
}

/// How many elements an array or slice has.
pub(super) enum Length {
    /// An array's, known at compile time.
    Fixed(u64),
    /// A slice's, read from it at run time: a `usize`.
    Held(Operand),
}

impl FnBuilder<'_, '_> {
    /// A call of one of the methods every array has. Each that takes an index checks it's in
    /// bounds first, comparing in the index's own type, so any integer type works.
    pub(super) fn array_method(
        &mut self,
        expr: Id<Expr>,
        method: ArrayMethod,
        receiver: Id<Expr>,
        args: &[Id<Expr>],
    ) -> Operand {
        let span = self.span(expr);
        let ty = self.expr_ty(expr);
        let receiver_ty = self.expr_ty(receiver);
        let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
        let place = self.place(receiver);
        // Where the elements are, and how many: fixed for an array, held by a slice.
        let types = &self.l.mir.types;
        let (elements, len) = match *types.get(receiver_ty) {
            Ty::Ref { to, .. } => match *types.get(to) {
                // Text holds its length the way a slice does.
                Ty::Slice(_) | Ty::Str => {
                    let count = Operand::Copy(place.clone().project(Projection::Field(1)));
                    (place.project(Projection::Deref), Length::Held(count))
                }
                Ty::Array { len, .. } => (
                    place.project(Projection::Deref),
                    Length::Fixed(types.len(len)),
                ),
                _ => unreachable!("checked: array methods are called on arrays and slices"),
            },
            Ty::Array { len, .. } => (place, Length::Fixed(types.len(len))),
            _ => unreachable!("checked: array methods are called on arrays and slices"),
        };
        if matches!(method, ArrayMethod::Slice | ArrayMethod::SliceMut) {
            return self.slice_method(method, ty, elements, len, args[0], span);
        }
        // The same view, as bytes.
        if method == ArrayMethod::Bytes {
            debug_assert!(
                matches!(len, Length::Held(_)),
                "checked: only text has bytes"
            );
            let view = Operand::Copy(elements_ref(&elements));
            let temp = self.temp(ty, span.clone());
            let cast = Rvalue::Cast { operand: view, ty };
            self.assign(Place::local(temp), cast, span);
            return Operand::Copy(Place::local(temp));
        }
        if method == ArrayMethod::Len {
            return match len {
                Length::Fixed(len) => self.constant(ConstKind::Int(len as i128), ty),
                Length::Held(count) if ty == usize => count,
                // Any integer type: converted, like any other.
                Length::Held(count) => {
                    let temp = self.temp(ty, span.clone());
                    let cast = Rvalue::Cast { operand: count, ty };
                    self.assign(Place::local(temp), cast, span);
                    Operand::Copy(Place::local(temp))
                }
            };
        }

        // Arguments in order, before anything is checked.
        let index_ty = self.expr_ty(args[0]);
        let index = self.operand(args[0]);
        let value = (method == ArrayMethod::Set).then(|| self.operand(args[1]));
        let position = self.temp(usize, span.clone());
        let index_copy = index.clone();
        let converted = if index_ty == usize {
            Rvalue::Use(index)
        } else {
            Rvalue::Cast {
                operand: index,
                ty: usize,
            }
        };
        self.assign(Place::local(position), converted, span.clone());
        let in_bounds = self.in_bounds(index_copy, index_ty, &len, position, span.clone());
        let element = elements.project(Projection::Index(position));

        let result = self.temp(ty, span.clone());
        let (inside, outside, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: in_bounds,
                then: inside,
                otherwise: outside,
            },
            span.clone(),
        );
        self.current = inside;
        let found = match method {
            ArrayMethod::Get => Rvalue::Some(Operand::Copy(element)),
            ArrayMethod::GetRef | ArrayMethod::GetMut => {
                let Ty::Optional(reference_ty) = *self.l.mir.types.get(ty) else {
                    unreachable!("checked: get_ref returns an optional reference")
                };
                let reference = self.temp(reference_ty, span.clone());
                let borrow = Rvalue::Ref {
                    mutable: method == ArrayMethod::GetMut,
                    place: element,
                };
                self.assign(Place::local(reference), borrow, span.clone());
                Rvalue::Some(Operand::Move(Place::local(reference)))
            }
            ArrayMethod::Set => {
                let value = value.expect("set has a value");
                self.assign(element, Rvalue::Use(value), span.clone());
                Rvalue::Use(self.constant(ConstKind::Bool(true), ty))
            }
            ArrayMethod::Len | ArrayMethod::Bytes | ArrayMethod::Slice | ArrayMethod::SliceMut => {
                unreachable!("handled above")
            }
        };
        self.assign(Place::local(result), found, span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        self.current = outside;
        let missing = match method {
            ArrayMethod::Set => ConstKind::Bool(false),
            _ => ConstKind::Zeroed,
        };
        let missing = self.constant(missing, ty);
        self.assign(Place::local(result), Rvalue::Use(missing), span.clone());
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
        self.read(Place::local(result), ty)
    }

    /// `slice(range)` or `slice_mut(range)` of `elements`, which has `len` of them: a view of
    /// those in the range, of type `ty` (an optional view), or `none` unless
    /// `0 <= start <= end <= len`. An end left out of the range is the elements' own.
    pub(super) fn slice_method(
        &mut self,
        method: ArrayMethod,
        ty: TypeId,
        elements: Place,
        len: Length,
        range: Id<Expr>,
        span: Span,
    ) -> Operand {
        let ast = self.l.ast;
        let bool = self.l.mir.types.bool;
        let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
        let range_ty = self.expr_ty(range);
        let Ty::Range(int_ty) = *self.l.mir.types.get(range_ty) else {
            unreachable!("checked: slices take ranges")
        };
        let Ty::Int(int) = *self.l.mir.types.get(int_ty) else {
            unreachable!("checked: ranges are of integers")
        };
        let (start, end) = match &ast.exprs[range] {
            Expr::Range { start, end } if start.is_none() || end.is_none() => {
                let start = match start {
                    Some(start) => self.operand(*start),
                    None => self.constant(ConstKind::Int(0), int_ty),
                };
                (start, end.map(|end| self.operand(end)))
            }
            _ => {
                let place = self.place(range);
                let field = |i| Operand::Copy(place.clone().project(Projection::Field(i)));
                (field(0), Some(field(1)))
            }
        };

        // Where the range stops: its end, or the elements' own.
        let stop = end.clone().unwrap_or_else(|| match &len {
            Length::Fixed(n) => self.constant(ConstKind::Int(*n as i128), usize),
            Length::Held(count) => count.clone(),
        });
        let to_usize = |b: &mut Self, value: Operand, from: TypeId| -> Operand {
            if from == usize {
                return value;
            }
            let temp = b.temp(usize, span.clone());
            let cast = Rvalue::Cast {
                operand: value,
                ty: usize,
            };
            b.assign(Place::local(temp), cast, span.clone());
            Operand::Copy(Place::local(temp))
        };
        let first = to_usize(self, start.clone(), int_ty);
        let stop_usize = match &end {
            Some(end) => to_usize(self, end.clone(), int_ty),
            None => stop.clone(),
        };

        let mut checks = Vec::new();
        let signed = matches!(
            int,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::ISize | IntTy::I128
        );
        if signed {
            let zero = self.constant(ConstKind::Int(0), int_ty);
            checks.push(Rvalue::Binary {
                op: BinOp::GreaterThanOrEqual,
                lhs: start.clone(),
                rhs: zero,
            });
        }
        if let Some(end) = &end {
            checks.push(Rvalue::Binary {
                op: BinOp::LessThanOrEqual,
                lhs: start.clone(),
                rhs: end.clone(),
            });
        }
        // Within the elements: compared as `usize`s, which the checks above make exact.
        // (A 128-bit range past what a `usize` holds is the one case that isn't.)
        let last_usize = match &end {
            Some(_) => stop_usize.clone(),
            None => first.clone(),
        };
        let len_usize = match &len {
            Length::Fixed(n) => self.constant(ConstKind::Int(*n as i128), usize),
            Length::Held(count) => count.clone(),
        };
        checks.push(Rvalue::Binary {
            op: BinOp::LessThanOrEqual,
            lhs: last_usize,
            rhs: len_usize,
        });
        let mut in_bounds: Option<Operand> = None;
        for check in checks {
            let temp = self.temp(bool, span.clone());
            self.assign(Place::local(temp), check, span.clone());
            let check = Operand::Copy(Place::local(temp));
            in_bounds = Some(match in_bounds {
                None => check,
                Some(earlier) => {
                    let both = self.temp(bool, span.clone());
                    let and = Rvalue::Binary {
                        op: BinOp::And,
                        lhs: earlier,
                        rhs: check,
                    };
                    self.assign(Place::local(both), and, span.clone());
                    Operand::Copy(Place::local(both))
                }
            });
        }
        let in_bounds = in_bounds.expect("there's always an upper bound");

        let result = self.temp(ty, span.clone());
        let (inside, outside, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: in_bounds,
                then: inside,
                otherwise: outside,
            },
            span.clone(),
        );
        self.current = inside;
        let Ty::Optional(view_ty) = *self.l.mir.types.get(ty) else {
            unreachable!("checked: slicing gives an optional view")
        };
        // The first element in the range (one past the last, for an empty one), and how many.
        let position = self.temp(usize, span.clone());
        self.assign(Place::local(position), Rvalue::Use(first), span.clone());
        let mutable = method == ArrayMethod::SliceMut;
        let elements_ty = self.place_ty(&elements);
        let element_ty = (self.l.mir)
            .project(elements_ty, Projection::Index(position))
            .expect("checked: slices are sliced by element");
        let reference_ty = self.l.mir.types.intern(Ty::Ref {
            mutable,
            to: element_ty,
        });
        let reference = self.temp(reference_ty, span.clone());
        let borrow = Rvalue::Ref {
            mutable,
            place: elements.project(Projection::Index(position)),
        };
        self.assign(Place::local(reference), borrow, span.clone());
        let count = self.temp(usize, span.clone());
        let difference = Rvalue::Binary {
            op: BinOp::Subtract,
            lhs: stop_usize,
            rhs: Operand::Copy(Place::local(position)),
        };
        self.assign(Place::local(count), difference, span.clone());
        let view = self.temp(view_ty, span.clone());
        let made = Rvalue::MakeSlice {
            ty: view_ty,
            start: Operand::Move(Place::local(reference)),
            len: Operand::Copy(Place::local(count)),
        };
        self.assign(Place::local(view), made, span.clone());
        let some = Rvalue::Some(Operand::Move(Place::local(view)));
        self.assign(Place::local(result), some, span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        self.current = outside;
        let missing = self.constant(ConstKind::Zeroed, ty);
        self.assign(Place::local(result), Rvalue::Use(missing), span.clone());
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
        self.read(Place::local(result), ty)
    }

    /// Whether `index`, of integer type `index_ty`, is at least 0 and less than `len`.
    /// `position` holds it converted to a `usize`, which a held length is compared against;
    /// a fixed one is compared in the index's own type, and not at all if that type can't
    /// reach it: an `i8` is always less than 300.
    pub(super) fn in_bounds(
        &mut self,
        index: Operand,
        index_ty: TypeId,
        len: &Length,
        position: Local,
        span: Span,
    ) -> Operand {
        let bool = self.l.mir.types.bool;
        let Ty::Int(int) = *self.l.mir.types.get(index_ty) else {
            unreachable!("checked: indexes are integers")
        };
        let (signed, max): (bool, u128) = match int {
            IntTy::I8 => (true, i8::MAX as u128),
            IntTy::I16 => (true, i16::MAX as u128),
            IntTy::I32 => (true, i32::MAX as u128),
            IntTy::I64 | IntTy::ISize => (true, i64::MAX as u128),
            IntTy::I128 => (true, i128::MAX as u128),
            IntTy::U8 => (false, u8::MAX as u128),
            IntTy::U16 => (false, u16::MAX as u128),
            IntTy::U32 => (false, u32::MAX as u128),
            IntTy::U64 | IntTy::USize => (false, u64::MAX as u128),
            IntTy::U128 => (false, u128::MAX),
        };
        let mut checks = Vec::new();
        if signed {
            let zero = self.constant(ConstKind::Int(0), index_ty);
            checks.push(Rvalue::Binary {
                op: BinOp::GreaterThanOrEqual,
                lhs: index.clone(),
                rhs: zero,
            });
        }
        match len {
            Length::Fixed(len) if (*len as u128) <= max => {
                let len = self.constant(ConstKind::Int(*len as i128), index_ty);
                checks.push(Rvalue::Binary {
                    op: BinOp::LessThan,
                    lhs: index,
                    rhs: len,
                });
            }
            Length::Fixed(_) => {}
            // A 128-bit index could be past what a `usize` holds: compare in its own type.
            Length::Held(count) if matches!(int, IntTy::I128 | IntTy::U128) => {
                let wide = self.temp(index_ty, span.clone());
                let cast = Rvalue::Cast {
                    operand: count.clone(),
                    ty: index_ty,
                };
                self.assign(Place::local(wide), cast, span.clone());
                checks.push(Rvalue::Binary {
                    op: BinOp::LessThan,
                    lhs: index,
                    rhs: Operand::Copy(Place::local(wide)),
                });
            }
            // Anything else that's at least 0 fits in a `usize`.
            Length::Held(count) => checks.push(Rvalue::Binary {
                op: BinOp::LessThan,
                lhs: Operand::Copy(Place::local(position)),
                rhs: count.clone(),
            }),
        }
        let mut result: Option<Operand> = None;
        for check in checks {
            let temp = self.temp(bool, span.clone());
            self.assign(Place::local(temp), check, span.clone());
            let check = Operand::Copy(Place::local(temp));
            result = Some(match result {
                None => check,
                Some(earlier) => {
                    let both = self.temp(bool, span.clone());
                    let and = Rvalue::Binary {
                        op: BinOp::And,
                        lhs: earlier,
                        rhs: check,
                    };
                    self.assign(Place::local(both), and, span.clone());
                    Operand::Copy(Place::local(both))
                }
            });
        }
        result.unwrap_or_else(|| self.constant(ConstKind::Bool(true), bool))
    }
}

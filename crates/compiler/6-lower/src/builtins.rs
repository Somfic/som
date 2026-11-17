//! The methods the language has built in: those of arrays, slices and text, and those of
//! raw pointers, which are intrinsics.

use som_ast::{BinOp, Expr, Id, Span};
use som_mir::*;
use som_typeck::{ArrayMethod, PointerMethod};

use super::FnBuilder;

/// The reference a slice's (or text's) elements are reached through: `s` for `(*s)`.
fn elements_ref(elements: &Place) -> Place {
    let mut reference = elements.clone();
    reference.projection.pop();
    reference
}

/// How many elements an array or slice has.
enum Length {
    Fixed(u64),
    /// A slice's, read from it at run time: a `usize`.
    Held(Operand),
}

impl FnBuilder<'_, '_> {
    /// `p.resize(n)` assigns `p = realloc(p, …)`; the others are intrinsics of their own.
    pub(super) fn pointer_method(
        &mut self,
        expr: Id<Expr>,
        method: PointerMethod,
        receiver: Id<Expr>,
        args: &[Id<Expr>],
    ) -> Rvalue {
        if method == PointerMethod::Resize {
            let place = self.place(receiver);
            let ty = self.place_ty(&place);
            let count = self.operand(args[0]);
            let resized = Rvalue::Intrinsic {
                intrinsic: Intrinsic::Resize,
                args: vec![Operand::Copy(place.clone()), count],
                ty,
            };
            self.assign(place, resized, self.span(expr));
            let unit = self.l.mir.types.unit;
            return Rvalue::Use(self.constant(ConstKind::Unit, unit));
        }
        let intrinsic = match method {
            PointerMethod::Resize => Intrinsic::Resize,
            PointerMethod::Free => Intrinsic::Free,
            PointerMethod::Read => Intrinsic::Read,
            PointerMethod::Write => Intrinsic::Write,
            PointerMethod::At | PointerMethod::AtMut => Intrinsic::Offset,
            PointerMethod::View | PointerMethod::ViewMut => Intrinsic::View,
            PointerMethod::IsNull => Intrinsic::IsNull,
        };
        let args = std::iter::once(receiver)
            .chain(args.iter().copied())
            .map(|a| self.operand(a))
            .collect();
        let ty = self.expr_ty(expr);
        Rvalue::Intrinsic {
            intrinsic,
            args,
            ty,
        }
    }

    /// Each that takes an index checks it's in bounds first, comparing in the index's own
    /// type, so any integer type works.
    pub(super) fn array_method(
        &mut self,
        expr: Id<Expr>,
        method: ArrayMethod,
        receiver: Id<Expr>,
        args: &[Id<Expr>],
    ) -> Operand {
        let span = self.span(expr);
        let ty = self.expr_ty(expr);
        let (elements, len) = self.elements_of(receiver);
        match method {
            ArrayMethod::Slice | ArrayMethod::SliceMut => {
                self.slice_method(method, ty, elements, len, args[0], span)
            }
            // The same view, as bytes.
            ArrayMethod::Bytes => {
                debug_assert!(
                    matches!(len, Length::Held(_)),
                    "checked: only text has bytes"
                );
                let view = Operand::Copy(elements_ref(&elements));
                self.temp_copy(ty, Rvalue::Cast { operand: view, ty }, span)
            }
            ArrayMethod::Len => match len {
                Length::Fixed(len) => self.constant(ConstKind::Int(len as i128), ty),
                Length::Held(count) if ty == self.l.mir.types.usize => count,
                // Any integer type: converted, like any other.
                Length::Held(count) => {
                    self.temp_copy(ty, Rvalue::Cast { operand: count, ty }, span)
                }
            },
            ArrayMethod::Get | ArrayMethod::GetRef | ArrayMethod::GetMut | ArrayMethod::Set => {
                self.element_method(method, ty, elements, len, args, span)
            }
        }
    }

    /// Where the elements are, and how many: fixed for an array, held by a slice.
    fn elements_of(&mut self, receiver: Id<Expr>) -> (Place, Length) {
        let receiver_ty = self.expr_ty(receiver);
        let place = self.place(receiver);
        let types = &self.l.mir.types;
        match *types.get(receiver_ty) {
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
        }
    }

    /// `get`, `get_ref`, `get_mut` and `set`, which do nothing out of bounds.
    fn element_method(
        &mut self,
        method: ArrayMethod,
        ty: TypeId,
        elements: Place,
        len: Length,
        args: &[Id<Expr>],
        span: Span,
    ) -> Operand {
        let usize = self.l.mir.types.usize;
        // Arguments in order, before anything is checked.
        let index_ty = self.expr_ty(args[0]);
        let index = self.operand(args[0]);
        let value = (method == ArrayMethod::Set).then(|| self.operand(args[1]));
        let position = self.temp(usize, span.clone());
        let converted = self.usize_rvalue(index.clone(), index_ty);
        self.assign(Place::local(position), converted, span.clone());
        let in_bounds = self.in_bounds(index, index_ty, &len, position, span.clone());
        let element = elements.project(Projection::Index(position));
        let missing = match method {
            ArrayMethod::Set => ConstKind::Bool(false),
            _ => ConstKind::Zeroed,
        };
        self.checked(in_bounds, ty, missing, span.clone(), |b| match method {
            ArrayMethod::Get => Rvalue::Some(Operand::Copy(element)),
            ArrayMethod::GetRef | ArrayMethod::GetMut => {
                let Ty::Optional(reference_ty) = *b.l.mir.types.get(ty) else {
                    unreachable!("checked: get_ref returns an optional reference")
                };
                let borrow = Rvalue::Ref {
                    mutable: method == ArrayMethod::GetMut,
                    place: element,
                };
                let reference = b.temp(reference_ty, span.clone());
                b.assign(Place::local(reference), borrow, span.clone());
                Rvalue::Some(Operand::Move(Place::local(reference)))
            }
            ArrayMethod::Set => {
                let value = value.expect("set has a value");
                b.assign(element, Rvalue::Use(value), span.clone());
                Rvalue::Use(b.constant(ConstKind::Bool(true), ty))
            }
            _ => unreachable!("only element methods"),
        })
    }

    /// `if condition { inside } else { missing }`, as a `ty`.
    fn checked(
        &mut self,
        condition: Operand,
        ty: TypeId,
        missing: ConstKind,
        span: Span,
        inside: impl FnOnce(&mut Self) -> Rvalue,
    ) -> Operand {
        let result = self.temp(ty, span.clone());
        let (inside_block, outside, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: inside_block,
                otherwise: outside,
            },
            span.clone(),
        );
        self.current = inside_block;
        let found = inside(self);
        self.assign(Place::local(result), found, span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        self.current = outside;
        let missing = self.constant(missing, ty);
        self.assign(Place::local(result), Rvalue::Use(missing), span.clone());
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
        self.read(Place::local(result), ty)
    }

    /// `slice(range)` or `slice_mut(range)`: a view of the elements in the range, of type `ty`
    /// (an optional view), or `none` unless `0 <= start <= end <= len`. An end left out of
    /// the range is the elements' own.
    fn slice_method(
        &mut self,
        method: ArrayMethod,
        ty: TypeId,
        elements: Place,
        len: Length,
        range: Id<Expr>,
        span: Span,
    ) -> Operand {
        let usize = self.l.mir.types.usize;
        let range_ty = self.expr_ty(range);
        let Ty::Range(int_ty) = *self.l.mir.types.get(range_ty) else {
            unreachable!("checked: slices take ranges")
        };
        let Ty::Int(int) = *self.l.mir.types.get(int_ty) else {
            unreachable!("checked: ranges are of integers")
        };
        let (start, end) = self.range_ends(range, int_ty);
        let len = match len {
            Length::Fixed(n) => self.constant(ConstKind::Int(n as i128), usize),
            Length::Held(count) => count,
        };
        let first = self.to_usize(start.clone(), int_ty, &span);
        let stop = match &end {
            Some(end) => self.to_usize(end.clone(), int_ty, &span),
            None => len.clone(),
        };

        let mut checks = Vec::new();
        if int.is_signed() {
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
                lhs: start,
                rhs: end.clone(),
            });
        }
        // Compared as `usize`s, which the checks above make exact. (A 128-bit range past
        // what a `usize` holds is the one case that isn't.)
        let last = match &end {
            Some(_) => stop.clone(),
            None => first.clone(),
        };
        checks.push(Rvalue::Binary {
            op: BinOp::LessThanOrEqual,
            lhs: last,
            rhs: len,
        });
        let in_bounds = (self.all(checks, &span)).expect("there's always an upper bound");

        let mutable = method == ArrayMethod::SliceMut;
        self.checked(in_bounds, ty, ConstKind::Zeroed, span.clone(), |b| {
            let Ty::Optional(view_ty) = *b.l.mir.types.get(ty) else {
                unreachable!("checked: slicing gives an optional view")
            };
            let view = b.view(view_ty, elements, first, stop, mutable, span);
            Rvalue::Some(Operand::Move(Place::local(view)))
        })
    }

    /// Where a range starts, and where it ends, if it says.
    fn range_ends(&mut self, range: Id<Expr>, int_ty: TypeId) -> (Operand, Option<Operand>) {
        match &self.l.ast.exprs[range] {
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
        }
    }

    /// A slice of `elements` from `first` (one past the last, for an empty one) up to `stop`.
    fn view(
        &mut self,
        view_ty: TypeId,
        elements: Place,
        first: Operand,
        stop: Operand,
        mutable: bool,
        span: Span,
    ) -> Local {
        let usize = self.l.mir.types.usize;
        let position = self.temp(usize, span.clone());
        self.assign(Place::local(position), Rvalue::Use(first), span.clone());
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
            lhs: stop,
            rhs: Operand::Copy(Place::local(position)),
        };
        self.assign(Place::local(count), difference, span.clone());
        let view = self.temp(view_ty, span.clone());
        let made = Rvalue::MakeSlice {
            ty: view_ty,
            start: Operand::Move(Place::local(reference)),
            len: Operand::Copy(Place::local(count)),
        };
        self.assign(Place::local(view), made, span);
        view
    }

    fn to_usize(&mut self, value: Operand, from: TypeId, span: &Span) -> Operand {
        let usize = self.l.mir.types.usize;
        if from == usize {
            return value;
        }
        let cast = Rvalue::Cast {
            operand: value,
            ty: usize,
        };
        self.temp_copy(usize, cast, span.clone())
    }

    /// An integer of type `from` as a `usize`.
    pub(super) fn usize_rvalue(&self, value: Operand, from: TypeId) -> Rvalue {
        let usize = self.l.mir.types.usize;
        if from == usize {
            Rvalue::Use(value)
        } else {
            Rvalue::Cast {
                operand: value,
                ty: usize,
            }
        }
    }

    /// Whether `index`, of integer type `index_ty`, is at least 0 and less than `len`.
    /// `position` holds it as a `usize`, which a held length is compared against; a fixed one
    /// is compared in the index's own type, and not at all if that type can't reach it: an
    /// `i8` is always less than 300.
    fn in_bounds(
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
        let mut checks = Vec::new();
        if int.is_signed() {
            let zero = self.constant(ConstKind::Int(0), index_ty);
            checks.push(Rvalue::Binary {
                op: BinOp::GreaterThanOrEqual,
                lhs: index.clone(),
                rhs: zero,
            });
        }
        match len {
            Length::Fixed(len) if (*len as u128) <= int.max() => {
                let len = self.constant(ConstKind::Int(*len as i128), index_ty);
                checks.push(Rvalue::Binary {
                    op: BinOp::LessThan,
                    lhs: index,
                    rhs: len,
                });
            }
            Length::Fixed(_) => {}
            // A 128-bit index could be past what a `usize` holds: compared in its own type.
            Length::Held(count) if int.bits() == 128 => {
                let cast = Rvalue::Cast {
                    operand: count.clone(),
                    ty: index_ty,
                };
                let wide = self.temp_copy(index_ty, cast, span.clone());
                checks.push(Rvalue::Binary {
                    op: BinOp::LessThan,
                    lhs: index,
                    rhs: wide,
                });
            }
            // Anything else that's at least 0 fits in a `usize`.
            Length::Held(count) => checks.push(Rvalue::Binary {
                op: BinOp::LessThan,
                lhs: Operand::Copy(Place::local(position)),
                rhs: count.clone(),
            }),
        }
        (self.all(checks, &span)).unwrap_or_else(|| self.constant(ConstKind::Bool(true), bool))
    }

    /// Whether all of `checks` hold; `None` for no checks.
    fn all(&mut self, checks: Vec<Rvalue>, span: &Span) -> Option<Operand> {
        let bool = self.l.mir.types.bool;
        let mut result: Option<Operand> = None;
        for check in checks {
            let check = self.temp_copy(bool, check, span.clone());
            result = Some(match result {
                None => check,
                Some(earlier) => {
                    let and = Rvalue::Binary {
                        op: BinOp::And,
                        lhs: earlier,
                        rhs: check,
                    };
                    self.temp_copy(bool, and, span.clone())
                }
            });
        }
        result
    }
}

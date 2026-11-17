use som_ast::{BinOp, Expr, Id, Ident, Span, Struct};
use som_mir::*;
use som_resolve::DefKind;
use som_typeck::Value;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    pub(super) fn rvalue_unwrapped(&mut self, expr: Id<Expr>) -> Rvalue {
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Interpolated(parts) => self.interpolated(expr, parts),
            Expr::Binary {
                op: BinOp::And | BinOp::Or,
                ..
            } => Rvalue::Use(self.operand_unwrapped(expr)),
            Expr::Binary { op, lhs, rhs }
                if matches!(op, BinOp::Equals | BinOp::NotEquals)
                    && (matches!(ast.exprs[*lhs], Expr::None)
                        || matches!(ast.exprs[*rhs], Expr::None)) =>
            {
                self.compare_to_none(*op, *lhs, *rhs)
            }
            Expr::Binary {
                op: op @ (BinOp::Equals | BinOp::NotEquals),
                lhs,
                rhs,
            } if self.is_optional(*lhs) || self.is_optional(*rhs) => {
                let equal = self.optional_equality(*lhs, *rhs, self.span(expr));
                match op {
                    BinOp::Equals => Rvalue::Use(equal),
                    _ => Rvalue::Not(equal),
                }
            }
            // Values of a type implementing `Eq` compare with its `eq`.
            Expr::Binary { op, lhs, rhs }
                if let Some(lang) = self.typed.operator_trait(expr)
                    && let Some(call) =
                        self.operator_call(expr, lang, *op, *lhs, *rhs, self.span(expr)) =>
            {
                call
            }
            Expr::Binary { op, lhs, rhs } => Rvalue::Binary {
                op: *op,
                lhs: self.number(*lhs),
                rhs: self.number(*rhs),
            },
            Expr::Not(inner) => Rvalue::Not(self.condition(*inner)),
            // `~x`: `Not` of an integer flips its bits.
            Expr::BitNot(inner) => Rvalue::Not(self.number(*inner)),
            Expr::Unreachable => self.failing(expr, None),
            Expr::Fail(message) => self.failing(expr, Some(*message)),
            Expr::Neg(inner) => self.negated(*inner),
            Expr::Borrow {
                mutable,
                expr: inner,
            } => {
                // Makes sure the reference type exists in MIR's table, even after substitution.
                self.expr_ty(expr);
                Rvalue::Ref {
                    mutable: *mutable,
                    place: self.place(*inner),
                }
            }
            Expr::Out(name) => self.out_arg(expr, name),
            Expr::Construct { fields, .. } => self.construct(expr, fields),
            Expr::Call { args, .. } => self.call_expr(expr, args),
            Expr::Array(items) => {
                let items: Vec<Operand> = items.iter().map(|&item| self.operand(item)).collect();
                let len = Ty::Value(Value::Int(items.len() as u128));
                let len = self.l.mir.types.intern(len);
                self.collection(expr, len, |ty| Rvalue::Array { ty, items })
            }
            Expr::Cast { value, .. } => self.cast(expr, *value),
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => {
                let ty = self.expr_ty(expr);
                let (start, end) = (self.operand(*start), self.operand(*end));
                Rvalue::Range { ty, start, end }
            }
            Expr::ArrayRepeat { value, .. } => {
                let operand = self.operand(*value);
                let len = self.typed.count(expr);
                let len = self.value_of(len);
                self.collection(expr, len, |ty| Rvalue::Repeat { ty, operand })
            }
            Expr::MethodCall { receiver, args, .. }
                if let Some(method) = self.typed.pointer_method(expr) =>
            {
                self.pointer_method(expr, method, *receiver, args)
            }
            Expr::MethodCall { receiver, args, .. }
                if let Some(method) = self.typed.array_method(expr) =>
            {
                Rvalue::Use(self.array_method(expr, method, *receiver, args))
            }
            Expr::MethodCall { receiver, args, .. } => {
                let def = (self.typed.method(expr)).expect("checked method calls are resolved");
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                self.call(expr, func, Some(*receiver), args)
            }
            // `items.len` of an array, slice or text.
            Expr::Field { object, .. } if let Some(method) = self.typed.array_method(expr) => {
                Rvalue::Use(self.array_method(expr, method, *object, &[]))
            }
            // `list.len`: property `len`, called with `list` as its `self`.
            Expr::Field { object, .. } if let Some(def) = self.typed.method(expr) => {
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("properties are functions")
                };
                self.call(expr, func, Some(*object), &[])
            }
            _ => Rvalue::Use(self.operand_unwrapped(expr)),
        }
    }

    /// `x != none` is whether `x` holds a value.
    fn compare_to_none(&mut self, op: BinOp, lhs: Id<Expr>, rhs: Id<Expr>) -> Rvalue {
        let optional = if matches!(self.l.ast.exprs[lhs], Expr::None) {
            rhs
        } else {
            lhs
        };
        let place = self.place(optional);
        let holds_value = Operand::Copy(place.project(Projection::Field(0)));
        match op {
            BinOp::NotEquals => Rvalue::Use(holds_value),
            _ => Rvalue::Not(holds_value),
        }
    }

    /// `@unreachable` and `@fail(message)`. Nothing after runs, so the value is never read:
    /// zeroes stand in.
    fn failing(&mut self, expr: Id<Expr>, message: Option<Id<Expr>>) -> Rvalue {
        let span = self.span(expr);
        let message = message.map(|message| self.operand(message));
        self.fail(message, span);
        let ty = self.expr_ty(expr);
        Rvalue::Use(self.constant(ConstKind::Zeroed, ty))
    }

    /// `-x` is `0 - x`.
    fn negated(&mut self, inner: Id<Expr>) -> Rvalue {
        let ty = self.expr_ty(inner);
        let zero = match self.l.mir.types.get(ty) {
            Ty::Float(_) => ConstKind::Float(0.0),
            _ => ConstKind::Int(0),
        };
        Rvalue::Binary {
            op: BinOp::Subtract,
            lhs: self.constant(zero, ty),
            rhs: self.operand(inner),
        }
    }

    /// A variable living to the end of the surrounding scope, like a `let` before the call.
    /// It starts zeroed: C functions don't always fill in their out-parameters.
    fn out_arg(&mut self, expr: Id<Expr>, name: &Ident) -> Rvalue {
        let def = (self.l.names.expr(expr)).expect("checked programs are fully resolved");
        let ty = self.ty(self.typed.local(def));
        let local = self.let_local(def, name, ty, true, name.span.clone());
        let zero = self.constant(ConstKind::Zeroed, ty);
        self.assign(Place::local(local), Rvalue::Use(zero), name.span.clone());
        // Makes sure the reference type exists in MIR's table, even after substitution.
        self.expr_ty(expr);
        Rvalue::Ref {
            mutable: true,
            place: Place::local(local),
        }
    }

    /// `Point { x: 1, y: 2 }`, or a variant's, `Shape::Rect { w: 1.0, h: 2.0 }`.
    fn construct(&mut self, expr: Id<Expr>, fields: &[(Ident, Id<Expr>)]) -> Rvalue {
        let ast = self.l.ast;
        let def = (self.l.names.expr(expr)).expect("checked programs are fully resolved");
        let (declared, variant) = match self.l.names.def(def).kind {
            DefKind::Struct(s) => (&ast.structs[s].fields, None),
            DefKind::Variant { strukt, index } => {
                (&ast.structs[strukt].variants()[index].fields, Some(index))
            }
            _ => unreachable!("constructors name structs and variants"),
        };
        // Evaluated in source order, then put in declaration order.
        let mut values: Vec<Option<Operand>> = vec![None; declared.len()];
        for (name, value) in fields {
            let index = (declared.iter())
                .position(|f| f.name.name == name.name)
                .expect("checked field");
            values[index] = Some(self.operand(*value));
        }
        let ty = self.expr_ty(expr);
        Rvalue::Struct {
            ty,
            variant,
            fields: (values.into_iter())
                .map(|v| v.expect("checked: no missing fields"))
                .collect(),
        }
    }

    fn call_expr(&mut self, expr: Id<Expr>, args: &[Id<Expr>]) -> Rvalue {
        // An instance it asks for is first used here.
        self.l.at = Some(self.span(expr));
        let def = (self.typed)
            .callee(expr, self.l.names)
            .expect("checked programs are fully resolved");
        match self.l.names.def(def).kind {
            DefKind::Func(func) => self.call(expr, func, None, args),
            // `Shape::Circle(1.0)`
            DefKind::Variant { index, .. } => {
                let fields = args.iter().map(|&a| self.operand(a)).collect();
                let ty = self.expr_ty(expr);
                Rvalue::Struct {
                    ty,
                    variant: Some(index),
                    fields,
                }
            }
            _ => unreachable!("calls name functions and variants"),
        }
    }

    fn cast(&mut self, expr: Id<Expr>, value: Id<Expr>) -> Rvalue {
        let ty = self.expr_ty(expr);
        let from = self.expr_ty(value);
        // A slice as a pointer: where its elements start.
        if let Ty::Ref { to, .. } = *self.l.mir.types.get(from)
            && matches!(self.l.mir.types.get(to), Ty::Slice(_) | Ty::Str)
        {
            let start = self.place(value).project(Projection::Field(0));
            return Rvalue::Cast {
                operand: Operand::Copy(start),
                ty,
            };
        }
        let operand = self.number(value);
        if let Ty::Struct(def, _) = *self.l.mir.types.get(from)
            && let DefKind::Struct(s) = self.l.names.def(def).kind
            && self.l.ast.structs[s].has_values()
        {
            return self.variant_number(s, operand, ty, self.span(expr));
        }
        if from == ty {
            Rvalue::Use(operand)
        } else {
            Rvalue::Cast { operand, ty }
        }
    }

    /// The number written for variant `tag` of enum `s`, `W = 87`, as a `ty`: read from a
    /// table of them all.
    fn variant_number(&mut self, s: Id<Struct>, tag: Operand, ty: TypeId, span: Span) -> Rvalue {
        let strukt = &self.l.ast.structs[s];
        let items: Vec<Operand> = (0..strukt.variants().len())
            .map(|i| self.constant(ConstKind::Int(strukt.variant_value(i)), ty))
            .collect();
        let len = Ty::Value(Value::Int(items.len() as u128));
        let len = self.l.mir.types.intern(len);
        let table_ty = self.l.mir.types.intern(Ty::Array { element: ty, len });
        let table = self.temp(table_ty, span.clone());
        let array = Rvalue::Array {
            ty: table_ty,
            items,
        };
        self.assign(Place::local(table), array, span.clone());
        let usize = self.l.mir.types.usize;
        let index = self.temp(usize, span.clone());
        let cast = Rvalue::Cast {
            operand: tag,
            ty: usize,
        };
        self.assign(Place::local(index), cast, span);
        let element = Place::local(table).project(Projection::Index(index));
        Rvalue::Use(Operand::Copy(element))
    }

    /// Whether `expr` reads a property, `list.len`, which is a call, rather than a field; or
    /// an array's length, `items.len`.
    pub(super) fn is_prop(&self, expr: Id<Expr>) -> bool {
        match self.l.ast.exprs[expr] {
            Expr::Field { .. } => {
                self.typed.method(expr).is_some() || self.typed.array_method(expr).is_some()
            }
            _ => false,
        }
    }

    /// A collection literal of `len` elements, made by `make` given its array type. Where a
    /// slice is expected, the array is kept in storage of its own until the end of the
    /// enclosing block, and the slice views it.
    fn collection(
        &mut self,
        expr: Id<Expr>,
        len: TypeId,
        make: impl FnOnce(TypeId) -> Rvalue,
    ) -> Rvalue {
        let ty = self.expr_ty(expr);
        let types = &mut self.l.mir.types;
        let Ty::Ref { mutable, to } = *types.get(ty) else {
            return make(ty);
        };
        let Ty::Slice(element) = *types.get(to) else {
            unreachable!("checked: collection literals are arrays or slices")
        };
        let array_ty = types.intern(Ty::Array { element, len });
        let reference_ty = types.intern(Ty::Ref {
            mutable,
            to: array_ty,
        });
        let span = self.span(expr);
        let storage = self.new_local(array_ty, None, mutable, span.clone());
        self.declare(storage, span.clone());
        self.assign(Place::local(storage), make(array_ty), span.clone());
        let reference = self.temp(reference_ty, span.clone());
        let borrow = Rvalue::Ref {
            mutable,
            place: Place::local(storage),
        };
        self.assign(Place::local(reference), borrow, span);
        Rvalue::Cast {
            operand: Operand::Move(Place::local(reference)),
            ty,
        }
    }
}

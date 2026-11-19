use crate::Emitter;
use crate::fold::{Expr, FnState, Prec, is_zero};
use crate::runtime::{FREE, REALLOC, c_string};
use som_mir::*;

impl Emitter<'_> {
    pub(crate) fn rvalue(&self, f: &mut FnState, rvalue: &Rvalue) -> Expr {
        match rvalue {
            Rvalue::Use(op) => self.operand(f, op),
            // `&*p` is `p`: for a slice, that's the only way to write it.
            Rvalue::Ref { place, .. } if place.projection.last() == Some(&Projection::Deref) => {
                let mut inner = place.clone();
                inner.projection.pop();
                Expr::atom(self.place(f, &inner))
            }
            Rvalue::Ref { place, .. } if self.is_absent(f, place) => {
                Expr::atom("(&som_unit_place)".to_string())
            }
            Rvalue::Ref { place, .. } => Expr::compound(format!("&{}", self.place(f, place))),
            Rvalue::Not(op) => self.not(f, op),
            Rvalue::Array { ty, items } => {
                let items: Vec<String> = items.iter().map(|op| self.operand(f, op).text).collect();
                let items = if items.is_empty() {
                    "0".into()
                } else {
                    format!("{{{}}}", items.join(", "))
                };
                Expr::atom(format!("({}){{{items}}}", self.ty(*ty)))
            }
            // Anything other than all zeroes is a loop, written by `block`.
            Rvalue::Repeat { ty, operand } => {
                debug_assert!(is_zero(operand), "only zeroes repeat in an expression");
                Expr::atom(format!("({}){{0}}", self.ty(*ty)))
            }
            Rvalue::Range { ty, start, end } => {
                let (start, end) = (self.operand(f, start).text, self.operand(f, end).text);
                Expr::atom(format!("({}){{{start}, {end}}}", self.ty(*ty)))
            }
            Rvalue::MakeSlice { ty, start, len } => {
                let (start, len) = (self.operand(f, start).text, self.operand(f, len).text);
                Expr::atom(format!("({}){{{start}, {len}}}", self.ty(*ty)))
            }
            Rvalue::Cast { operand, ty } => self.cast(f, operand, *ty),
            Rvalue::Some(op) => {
                let ty = self.operand_ty(f, op);
                let optional = (self.mir.types.lookup(&Ty::Optional(ty)))
                    .expect("validated MIR interns the optional types it builds");
                let value = self.operand(f, op).text;
                Expr::atom(format!("({}){{true, {value}}}", self.ty(optional)))
            }
            Rvalue::Binary { op, lhs, rhs } => self.binary(f, *op, lhs, rhs),
            Rvalue::Struct {
                ty,
                variant: Some(variant),
                fields,
            } => self.variant(f, *ty, *variant, fields),
            Rvalue::Struct { ty, fields, .. } => {
                let present = self.present_fields(*ty);
                let mut kept = Vec::new();
                // A `()` field's value isn't stored; one worked out by a call still runs.
                let mut effects = Vec::new();
                for (i, op) in fields.iter().enumerate() {
                    let text = self.operand(f, op).text;
                    match present.as_ref().is_some_and(|p| !p[i]) {
                        true if text.contains('(') => effects.push(text),
                        true => {}
                        false => kept.push(text),
                    }
                }
                let kept = if kept.is_empty() {
                    "0".into()
                } else {
                    kept.join(", ")
                };
                let made = format!("({}){{{kept}}}", self.ty(*ty));
                match effects.is_empty() {
                    true => Expr::atom(made),
                    false => Expr::atom(format!("({}, {made})", effects.join(", "))),
                }
            }
            Rvalue::Intrinsic {
                intrinsic,
                args,
                ty,
            } => self.intrinsic(f, *intrinsic, args, *ty),
            Rvalue::Call { func, args } => {
                let args: Vec<String> = args.iter().map(|a| self.operand(f, a).text).collect();
                Expr::atom(format!("{}({})", self.func_name(*func), args.join(", ")))
            }
            // An address is a `void*`: called as a pointer to a function of its type.
            Rvalue::CallIndirect { callee, args } => {
                let Ty::Fn { params, ret } = self.mir.types.get(self.operand_ty(f, callee)).clone()
                else {
                    unreachable!("only functions are called")
                };
                let params: Vec<String> = params.iter().map(|&p| self.ty(p)).collect();
                let pointer = format!("{} (*)({})", self.ty(ret), params.join(", "));
                let callee = self.operand(f, callee).operand();
                let args: Vec<String> = args.iter().map(|a| self.operand(f, a).text).collect();
                Expr::atom(format!("(({pointer}){callee})({})", args.join(", ")))
            }
        }
    }

    /// `!` of a `bool`; of an integer, `~`. C flips a small integer's bits as an `int`'s, so
    /// it's cut back to its own type.
    fn not(&self, f: &mut FnState, op: &Operand) -> Expr {
        let ty = self.operand_ty(f, op);
        let value = self.operand(f, op).operand();
        match self.mir.types.get(ty) {
            Ty::Int(_) if self.is_narrow(ty) => {
                Expr::compound(format!("({})~{value}", self.ty(ty)))
            }
            Ty::Int(_) => Expr::compound(format!("~{value}")),
            _ => Expr::compound(format!("!{value}")),
        }
    }

    fn cast(&self, f: &mut FnState, operand: &Operand, ty: TypeId) -> Expr {
        let from = self.operand_ty(f, operand);
        let value = self.operand(f, operand);
        let viewed = |t: TypeId| match self.mir.types.get(t) {
            Ty::Ref { to, .. } => Some(self.mir.types.get(*to)),
            _ => None,
        };
        match (viewed(from), viewed(ty)) {
            // A reference to an array becomes where its elements start, and how many.
            (Some(Ty::Array { len, .. }), Some(Ty::Slice(_))) => Expr::atom(format!(
                "({}){{{}->items, {}}}",
                self.ty(ty),
                value.operand(),
                self.mir.types.len(*len)
            )),
            // `[mut T]` to `[T]`: the same struct.
            (Some(Ty::Slice(_)), Some(Ty::Slice(_))) => value,
            // Text as its bytes: the same pointer and count, in a slice's struct.
            (Some(Ty::Str), Some(Ty::Slice(_))) => {
                let text = value.operand();
                Expr::atom(format!("({}){{{text}.ptr, {text}.len}}", self.ty(ty)))
            }
            _ => match (self.mir.types.get(from), self.mir.types.get(ty)) {
                (Ty::Float(_), Ty::Int(int)) => self.float_to_int(&value.operand(), *int),
                _ => Expr::compound(format!("({}){}", self.ty(ty), value.operand())),
            },
        }
    }

    fn binary(&self, f: &mut FnState, op: BinOp, lhs: &Operand, rhs: &Operand) -> Expr {
        let pointer = op == BinOp::Add && self.operand_ty(f, lhs) == self.mir.types.pointer;
        let (l, r) = (self.operand(f, lhs), self.operand(f, rhs));
        // `ptr + n` offsets by `n` bytes.
        if pointer {
            let (l, r) = (l.operand(), r.operand());
            return Expr::compound(format!("(void*)((uint8_t*){l} + {r})"));
        }
        if matches!(op, BinOp::ShiftLeft | BinOp::ShiftRight) {
            return self.shift(f, op, lhs, rhs, l, r);
        }
        let (l, r) = (l.operand_of(op, true), r.operand_of(op, false));
        let symbol = match op {
            BinOp::And => "&&",
            BinOp::Or => "||",
            op => op.symbol(),
        };
        Expr {
            text: format!("{l} {symbol} {r}"),
            prec: Prec::of(op),
        }
    }

    /// Each field by its designator: `(struct Shape){ .tag = 1, .as.Rect.w = 2 }`.
    fn variant(&self, f: &mut FnState, ty: TypeId, variant: usize, fields: &[Operand]) -> Expr {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            unreachable!("variants are of enums")
        };
        let s = self.mir.struct_def(*def, args);
        let mut text = format!("({}){{ .tag = {variant}", self.ty(ty));
        for (&index, op) in s.variant_fields(variant).iter().zip(fields) {
            let member = &self.names.fields[def][index];
            text.push_str(&format!(", .{member} = {}", self.operand(f, op).text));
        }
        text.push_str(" }");
        Expr::atom(text)
    }

    fn intrinsic(
        &self,
        f: &mut FnState,
        intrinsic: Intrinsic,
        args: &[Operand],
        ty: TypeId,
    ) -> Expr {
        let p = self.operand(f, &args[0]).operand();
        let element = match self.mir.types.get(self.operand_ty(f, &args[0])) {
            Ty::Ptr(element) => self.ty(*element),
            _ => unreachable!("intrinsics work on typed pointers"),
        };
        let mut arg = |i: usize| self.operand(f, &args[i]).operand();
        match intrinsic {
            Intrinsic::Resize => {
                let count = arg(1);
                Expr::compound(format!(
                    "({}){REALLOC}({p}, {count} * sizeof({element}))",
                    self.ty(ty)
                ))
            }
            Intrinsic::Free => Expr::atom(format!("({FREE}({p}), 0)")),
            Intrinsic::Take => Expr::atom(format!("{p}[{}]", arg(1))),
            Intrinsic::Put => {
                let (i, value) = (arg(1), arg(2));
                Expr::atom(format!("({p}[{i}] = {value}, 0)"))
            }
            Intrinsic::Offset => Expr::compound(format!("{p} + {}", arg(1))),
            Intrinsic::View => {
                let len = arg(1);
                Expr::atom(format!("({}){{{p}, {len}}}", self.ty(ty)))
            }
            Intrinsic::IsNull => Expr::compound(format!("{p} == 0")),
        }
    }

    pub(crate) fn operand_ty(&self, f: &FnState, op: &Operand) -> TypeId {
        self.mir.operand_ty(&f.body.locals, op)
    }

    fn project(&self, ty: TypeId, projection: Projection) -> TypeId {
        (self.mir.project(ty, projection)).expect("validated MIR has well-typed places")
    }

    /// Whether `place` is a `()` reached through a field, an element or a reference. A `()`
    /// takes no room, so C has no member for it: read, it's `0`; written, nothing.
    pub(crate) fn is_absent(&self, f: &FnState, place: &Place) -> bool {
        if place.projection.is_empty() {
            return false;
        }
        let ty = (place.projection.iter())
            .fold(f.body.local(place.local).ty, |ty, &p| self.project(ty, p));
        self.is_unit(ty)
    }

    /// The fields of struct type `ty` C has: all but its `()`s. `None` for anything else.
    pub(crate) fn present_fields(&self, ty: TypeId) -> Option<Vec<bool>> {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            return None;
        };
        let s = self.mir.struct_def(*def, args);
        if s.variants.is_some() || s.handle {
            return None;
        }
        Some(s.fields.iter().map(|&(_, t)| !self.is_unit(t)).collect())
    }

    pub(crate) fn operand(&self, f: &mut FnState, op: &Operand) -> Expr {
        match op {
            // A temporary computed by the previous statement is folded in here.
            Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
                match f.fold(place.local) {
                    Some(value) => value,
                    None => Expr::atom(self.place(f, place)),
                }
            }
            Operand::Copy(place) | Operand::Move(place) if self.is_absent(f, place) => {
                Expr::atom("0".to_string())
            }
            // Moves are plain copies in C; the borrow checker already made sure they're sound.
            Operand::Copy(place) | Operand::Move(place) => Expr::atom(self.place(f, place)),
            Operand::Const(c) => Expr::atom(self.constant(c)),
        }
    }

    fn constant(&self, c: &Const) -> String {
        match &c.kind {
            // C gives a literal a type wide enough for its value, and converts it to
            // whatever it's combined with, so no cast is needed.
            ConstKind::Int(n) => self.int_literal(*n, c.ty),
            ConstKind::Float(x) => match self.mir.types.get(c.ty) {
                Ty::Float(FloatTy::F32) => format!("{x:?}f"),
                _ => format!("{x:?}"),
            },
            ConstKind::Bool(b) => (if *b { "true" } else { "false" }).into(),
            // A `cstr` is the literal; a `str` is where it starts, and how many bytes it has.
            ConstKind::Str(s) if c.ty == self.mir.types.cstr => {
                format!("(uint8_t*){}", c_string(s))
            }
            // A `string`: its block is part of the program, counted as never freed.
            ConstKind::Str(s) if matches!(self.mir.types.get(c.ty), Ty::Struct(..)) => {
                let block = self.text_block(s);
                format!("({}){{(void*)&{block}}}", self.ty(c.ty))
            }
            ConstKind::Str(s) => format!(
                "({}){{(uint8_t*){}, {}}}",
                self.ty(c.ty),
                c_string(s),
                s.len()
            ),
            ConstKind::Unit => "0".into(),
            ConstKind::Func(id) => format!("(void*)&{}", self.func_name(*id)),
            ConstKind::Pending(_) => unreachable!("som_comptime works out every constant"),
            // A compound literal zeroes anything: `(void*){0}`, `(Color){0}`.
            ConstKind::Zeroed => format!("({}){{0}}", self.ty(c.ty)),
            ConstKind::Aggregate(parts) => self.aggregate(c.ty, parts),
        }
    }

    /// A struct, enum, array, optional or range made while compiling, as a compound literal.
    fn aggregate(&self, ty: TypeId, parts: &[Const]) -> String {
        let present = self.present_fields(ty);
        let text: Vec<String> = (parts.iter().enumerate())
            .filter(|(i, _)| present.as_ref().is_none_or(|p| p[*i]))
            .map(|(_, p)| self.constant(p))
            .collect();
        let name = self.ty(ty);
        match self.mir.types.get(ty) {
            // `(struct Shape){ .tag = 1, .as.Rect.w = 2 }`: the tag's variant's fields.
            Ty::Struct(def, args) if self.mir.struct_def(*def, args).variants.is_some() => {
                let s = self.mir.struct_def(*def, args);
                let ConstKind::Int(variant) = parts[0].kind else {
                    unreachable!("an enum's tag is a number")
                };
                let mut out = format!("({name}){{ .tag = {variant}");
                for &index in s.variant_fields(variant as usize) {
                    let member = &self.names.fields[def][index];
                    out.push_str(&format!(", .{member} = {}", text[index]));
                }
                out + " }"
            }
            _ if text.is_empty() => format!("({name}){{0}}"),
            // An array's elements are in its `items`.
            Ty::Array { .. } => format!("({name}){{{{{}}}}}", text.join(", ")),
            _ => format!("({name}){{{}}}", text.join(", ")),
        }
    }

    /// A place as a C lvalue: `x`, `p.x`, `self->x`, `(*r)`.
    pub(crate) fn place(&self, f: &FnState, place: &Place) -> String {
        f.mention(place.local);
        let mut s = f.locals[place.local.id].clone();
        let mut ty = f.body.local(place.local).ty;
        let mut projections = place.projection.iter().peekable();
        while let Some(&projection) = projections.next() {
            // `(*p).x` is `p->x`, and `(*p)[i]` is `p->items[i]`.
            if projection == Projection::Deref
                && let Some(&&next @ (Projection::Field(_) | Projection::Index(_))) =
                    projections.peek()
            {
                projections.next();
                let viewed = self.project(ty, Projection::Deref);
                s = match (self.mir.types.get(viewed), next) {
                    // `(*s)[i]` of a slice is `s.ptr[i]`.
                    (Ty::Slice(_) | Ty::Str, Projection::Index(index)) => {
                        format!("{s}.ptr[{}]", self.index(f, index))
                    }
                    _ => format!("{s}->{}", self.member(f, viewed, next)),
                };
                ty = self.project(viewed, next);
                continue;
            }
            s = match projection {
                Projection::Field(_) | Projection::Index(_) => {
                    format!("{s}.{}", self.member(f, ty, projection))
                }
                Projection::Deref => format!("(*{s})"),
            };
            ty = self.project(ty, projection);
        }
        s
    }

    /// What follows the `.` or `->` for a field, or an array's element: `x`, `items[i]`.
    fn member(&self, f: &FnState, ty: TypeId, projection: Projection) -> String {
        match projection {
            Projection::Field(i) => self.field_name(ty, i).to_string(),
            Projection::Index(index) => format!("items[{}]", self.index(f, index)),
            Projection::Deref => unreachable!("a dereference isn't a member"),
        }
    }

    /// An array index: the local holding it, or the temporary folded in.
    fn index(&self, f: &FnState, index: Local) -> String {
        match f.fold(index) {
            Some(expr) => expr.text,
            None => {
                f.mention(index);
                f.locals[index.id].clone()
            }
        }
    }

    fn field_name(&self, ty: TypeId, index: usize) -> &str {
        match self.mir.types.get(ty) {
            Ty::Struct(def, _) => &self.names.fields[def][index],
            Ty::Optional(_) if index == 0 => "some",
            Ty::Optional(_) => "value",
            Ty::Range(_) if index == 0 => "start",
            Ty::Range(_) => "end",
            Ty::Ref { .. } if index == 0 => "ptr",
            Ty::Ref { .. } => "len",
            _ => unreachable!("fields belong to structs and optionals"),
        }
    }
}

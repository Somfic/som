use super::{Machine, Unkeepable};
use crate::memory::{Byte, POINTER, layout};
use crate::numbers::{bounds, round, shift, traps, wrap};
use crate::*;
use som_base::Span;

impl Machine<'_, '_> {
    /// What a `Failure` value says failed: `@unreachable`, or `@fail`'s message.
    pub(crate) fn failure(&self, failure: &Value, ty: TypeId, span: &Span) -> Result<Fault, Stop> {
        let (Value::Agg(parts), Ty::Struct(def, args)) = (failure, self.mir().types.get(ty)) else {
            return unsupported("this failure", span);
        };
        let fields = &self.mir().struct_def(*def, args).fields;
        let field =
            |name: &str| (fields.iter().position(|(n, _)| n == name)).and_then(|i| parts.get(i));
        if let Some(Value::Bool(true)) = field(som_lang::FailureField::Unreachable.name()) {
            return Ok(Fault::Unreachable);
        }
        match field(som_lang::FailureField::Message.name()) {
            Some(message) => Ok(Fault::Failed(self.string(message, span)?)),
            None => unsupported("this failure", span),
        }
    }

    pub(crate) fn string(&self, value: &Value, span: &Span) -> Result<String, Stop> {
        let Value::Agg(parts) = value else {
            return unsupported("this text", span);
        };
        let Some(Value::Ref(view)) = parts.first() else {
            return unsupported("this text", span);
        };
        let Root::Heap { alloc, offset } = view.root else {
            return unsupported("this text", span);
        };
        // The block: its count, its length, then its bytes.
        let block = self.data(view, 2 * POINTER, span)?;
        let mut word = [0u8; 16];
        word[..POINTER].copy_from_slice(&block[POINTER..]);
        let len = u128::from_le_bytes(word) as usize;
        let bytes = self.data(&Loc::heap(alloc, offset + 2 * POINTER), len, span)?;
        String::from_utf8(bytes).or_else(|_| unsupported("text that isn't UTF-8", span))
    }

    /// What intrinsic `intrinsic` gives, given its arguments: a pointer to `element`s, then
    /// the rest.
    pub(crate) fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        element: TypeId,
        args: Vec<Value>,
        span: &Span,
    ) -> Result<Value, Stop> {
        let p = pointer(&args[0], span)?;
        let size = match layout(self.mir(), element) {
            Some(layout) => layout.size,
            None => return unsupported("memory holding a value of this type", span),
        };
        // The `i`th element on from where `p` points.
        let at = |m: &Self, i: usize| match &p {
            Some(loc) => m.step(loc, i, element, span),
            None => out_of_bounds(span),
        };
        Ok(match intrinsic {
            Intrinsic::Resize => {
                let bytes = count(&args[1], span)? * size;
                match &p {
                    None => Value::Ref(self.allocate(vec![Byte::Uninit; bytes])),
                    Some(Loc {
                        root: Root::Heap { alloc, offset: 0 },
                        ..
                    }) => {
                        match self.heap.get_mut(*alloc) {
                            Some(Some(memory)) => memory.resize(bytes, Byte::Uninit),
                            _ => return out_of_bounds(span),
                        }
                        args[0].clone()
                    }
                    Some(_) => return unsupported("resizing memory it didn't allocate", span),
                }
            }
            Intrinsic::Free => {
                match &p {
                    None => {}
                    Some(Loc {
                        root: Root::Heap { alloc, offset: 0 },
                        ..
                    }) => match self.heap.get_mut(*alloc) {
                        Some(slot @ Some(_)) => *slot = None,
                        _ => return out_of_bounds(span),
                    },
                    Some(_) => return unsupported("freeing memory it didn't allocate", span),
                }
                Value::Unit
            }
            Intrinsic::Take => {
                let loc = at(self, count(&args[1], span)?)?;
                self.read(&loc, element, span)?
            }
            Intrinsic::Put => {
                let loc = at(self, count(&args[1], span)?)?;
                self.write(&loc, element, args[2].clone(), span)?;
                Value::Unit
            }
            Intrinsic::Offset => Value::Ref(at(self, count(&args[1], span)?)?),
            Intrinsic::View => Value::Slice {
                start: p.clone(),
                len: count(&args[1], span)?,
            },
            Intrinsic::IsNull => Value::Bool(p.is_none()),
        })
    }

    pub(crate) fn operand(&mut self, op: &Operand, span: &Span) -> Result<Value, Stop> {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                let (loc, ty) = self.loc(place, span)?;
                self.read(&loc, ty, span)
            }
            Operand::Const(c) => self.constant(c, span),
        }
    }

    pub(crate) fn constant(&mut self, c: &Const, span: &Span) -> Result<Value, Stop> {
        Ok(match &c.kind {
            ConstKind::Int(n) => Value::Int(*n),
            ConstKind::Float(x) => Value::Float(*x),
            ConstKind::Bool(b) => Value::Bool(*b),
            ConstKind::Str(s) => self.text(s, c.ty, span)?,
            ConstKind::Unit => Value::Unit,
            ConstKind::Func(f) => Value::Func(*f),
            ConstKind::Zeroed => self.zero(c.ty, span)?,
            ConstKind::Aggregate(parts) => Value::Agg(
                (parts.iter())
                    .map(|part| self.constant(part, span))
                    .collect::<Result<_, _>>()?,
            ),
            ConstKind::Pending(init) => {
                let kind = self.state.init(*init).map_err(|stop| match stop {
                    // Needing itself: reported where this one needs it.
                    Stop::Report(fault @ Fault::Cycle(_), _) => Stop::Report(fault, span.clone()),
                    stop => stop,
                })?;
                self.constant(&Const { kind, ty: c.ty }, span)?
            }
        })
    }

    /// A text literal of type `ty`, in memory as the program would have it: a `cstr`'s bytes
    /// and a zero byte; a `str`'s bytes; or a `string`'s block, with the count that says it's
    /// part of the program, its length, its bytes, and a zero byte.
    pub(crate) fn text(&mut self, text: &str, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let types = &self.mir().types;
        let (cstr, string) = (ty == types.cstr, ty == types.text);
        let str_ref =
            matches!(types.get(ty), Ty::Ref { to, .. } if matches!(types.get(*to), Ty::Str));
        let string = string && matches!(types.get(ty), Ty::Struct(..));
        let bytes: Vec<Byte> = text.bytes().map(Byte::Data).collect();
        let zero = [Byte::Data(0)];
        if cstr {
            return Ok(Value::Ref(self.allocate([&bytes[..], &zero].concat())));
        }
        if string {
            let word = |n: usize| (n as u128).to_le_bytes()[..POINTER].to_vec();
            let header: Vec<Byte> = [word(usize::MAX), word(text.len())]
                .concat()
                .into_iter()
                .map(Byte::Data)
                .collect();
            let view = self.allocate([&header[..], &bytes, &zero].concat());
            return Ok(Value::Agg(vec![Value::Ref(view)]));
        }
        if str_ref {
            let start = self.allocate(bytes);
            return Ok(Value::Slice {
                start: Some(start),
                len: text.len(),
            });
        }
        unsupported("text of this type", span)
    }

    pub(crate) fn zero(&self, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let mir = self.mir();
        Ok(match mir.types.get(ty) {
            Ty::Unit => Value::Unit,
            Ty::Bool => Value::Bool(false),
            Ty::Char | Ty::Int(_) => Value::Int(0),
            Ty::Float(_) => Value::Float(0.0),
            Ty::Ref { to, .. } if matches!(mir.types.get(*to), Ty::Slice(_) | Ty::Str) => {
                Value::Slice {
                    start: None,
                    len: 0,
                }
            }
            Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. } | Ty::Fn { .. } => Value::Null,
            Ty::Struct(def, args) if mir.struct_def(*def, args).handle => Value::Null,
            Ty::Struct(..) | Ty::Optional(_) | Ty::Array { .. } | Ty::Range(_) => {
                let Some(layout) = layout(mir, ty) else {
                    return unsupported("a value of this type", span);
                };
                Value::Agg(
                    (layout.parts.iter())
                        .map(|&(ty, _)| self.zero(ty, span))
                        .collect::<Result<_, _>>()?,
                )
            }
            _ => return unsupported("a value of this type", span),
        })
    }

    pub(crate) fn array_len(&self, len: TypeId, span: &Span) -> Result<usize, Stop> {
        match self.mir().types.get(len) {
            Ty::Value(som_mir::Value::Int(n)) => Ok(*n as usize),
            _ => unsupported("an array of this length", span),
        }
    }

    pub(crate) fn operand_ty(&self, op: &Operand) -> TypeId {
        let frame = self.frames.last().expect("a frame");
        let body = self.mir().funcs[frame.func].body.as_ref().expect("a body");
        self.mir().operand_ty(&body.locals, op)
    }

    pub(crate) fn rvalue(&mut self, rvalue: &Rvalue, span: &Span) -> Result<Value, Stop> {
        let operands = |m: &mut Self, ops: &[Operand]| -> Result<Vec<Value>, Stop> {
            ops.iter().map(|op| m.operand(op, span)).collect()
        };
        Ok(match rvalue {
            Rvalue::Use(op) => self.operand(op, span)?,
            // `&*r` is `r` again: for a slice, its start and its length.
            Rvalue::Ref { place, .. } if place.projection.last() == Some(&Projection::Deref) => {
                let mut inner = place.clone();
                inner.projection.pop();
                let (loc, ty) = self.loc(&inner, span)?;
                self.read(&loc, ty, span)?
            }
            Rvalue::Ref { place, .. } => Value::Ref(self.loc(place, span)?.0),
            Rvalue::Binary { op, lhs, rhs } => {
                let ty = self.operand_ty(lhs);
                let (a, b) = (self.operand(lhs, span)?, self.operand(rhs, span)?);
                self.binary(*op, a, b, ty, span)?
            }
            Rvalue::Not(op) => {
                let ty = self.operand_ty(op);
                match (self.operand(op, span)?, self.mir().types.get(ty)) {
                    (Value::Bool(b), _) => Value::Bool(!b),
                    (Value::Int(n), Ty::Int(int)) => Value::Int(wrap(!n, *int)),
                    _ => return unsupported("this `!`", span),
                }
            }
            Rvalue::Some(op) => Value::Agg(vec![Value::Bool(true), self.operand(op, span)?]),
            Rvalue::Array { items, .. } => Value::Agg(operands(self, items)?),
            Rvalue::Repeat { ty, operand } => {
                let value = self.operand(operand, span)?;
                let len = match self.mir().types.get(*ty) {
                    Ty::Array { len, .. } => self.array_len(*len, span)?,
                    _ => return unsupported("an array of this length", span),
                };
                Value::Agg(vec![value; len])
            }
            Rvalue::Range { start, end, .. } => {
                Value::Agg(vec![self.operand(start, span)?, self.operand(end, span)?])
            }
            Rvalue::Cast { operand, ty } => {
                let from = self.operand_ty(operand);
                let value = self.operand(operand, span)?;
                self.cast(value, from, *ty, span)?
            }
            Rvalue::Struct {
                variant: None,
                fields,
                ..
            } => Value::Agg(operands(self, fields)?),
            Rvalue::Struct {
                ty,
                variant: Some(variant),
                fields,
            } => {
                let Value::Agg(mut parts) = self.zero(*ty, span)? else {
                    return unsupported("this enum", span);
                };
                let Ty::Struct(def, args) = self.mir().types.get(*ty) else {
                    return unsupported("this enum", span);
                };
                let indexes = (self.mir().struct_def(*def, args))
                    .variant_fields(*variant)
                    .to_vec();
                // Its tag, first, says which variant it is.
                parts[0] = Value::Int(*variant as i128);
                for (index, value) in indexes.into_iter().zip(operands(self, fields)?) {
                    parts[index] = value;
                }
                Value::Agg(parts)
            }
            Rvalue::MakeSlice { start, len, .. } => {
                let start = pointer(&self.operand(start, span)?, span)?;
                let len = count(&self.operand(len, span)?, span)?;
                Value::Slice { start, len }
            }
            Rvalue::Intrinsic {
                intrinsic, args, ..
            } => {
                let element = match self.mir().types.get(self.operand_ty(&args[0])) {
                    Ty::Ptr(element) => *element,
                    _ => return unsupported("this pointer", span),
                };
                let args = operands(self, args)?;
                self.intrinsic(*intrinsic, element, args, span)?
            }
            Rvalue::Call { .. } => unreachable!("calls are statements of their own"),
        })
    }

    pub(crate) fn binary(
        &self,
        op: BinOp,
        a: Value,
        b: Value,
        ty: TypeId,
        span: &Span,
    ) -> Result<Value, Stop> {
        let ty = self.mir().types.get(ty);
        Ok(match (a, b, ty) {
            // `ptr + n`: `n` bytes on.
            (Value::Ref(mut loc), Value::Int(n), Ty::Pointer) if op == BinOp::Add => {
                match &mut loc.root {
                    Root::Heap { offset, .. } | Root::Foreign { address: offset } => {
                        *offset = offset.wrapping_add(n as usize)
                    }
                    Root::Local { .. } => return unsupported("bytes on from a local", span),
                }
                Value::Ref(loc)
            }
            (Value::Int(a), Value::Int(b), Ty::Int(int)) => match op {
                BinOp::Add => Value::Int(wrap(a.wrapping_add(b), *int)),
                BinOp::Subtract => Value::Int(wrap(a.wrapping_sub(b), *int)),
                BinOp::Multiply => Value::Int(wrap(a.wrapping_mul(b), *int)),
                BinOp::Divide | BinOp::Modulo if b == 0 || traps(a, b, *int) => {
                    let overflow = b != 0;
                    return Err(Stop::Report(Fault::DividedByZero(overflow), span.clone()));
                }
                // Both truncate towards zero.
                BinOp::Divide => Value::Int(wrap(a / b, *int)),
                BinOp::Modulo => Value::Int(wrap(a % b, *int)),
                BinOp::BitAnd => Value::Int(wrap(a & b, *int)),
                BinOp::BitOr => Value::Int(wrap(a | b, *int)),
                BinOp::BitXor => Value::Int(wrap(a ^ b, *int)),
                BinOp::ShiftLeft | BinOp::ShiftRight => Value::Int(shift(op, a, b, *int)),
                BinOp::LessThan => Value::Bool(a < b),
                BinOp::GreaterThan => Value::Bool(a > b),
                BinOp::LessThanOrEqual => Value::Bool(a <= b),
                BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                BinOp::And | BinOp::Or => return unsupported("this operator", span),
            },
            // A `char`'s compared as the number it is.
            (Value::Int(a), Value::Int(b), _) => match op {
                BinOp::LessThan => Value::Bool(a < b),
                BinOp::GreaterThan => Value::Bool(a > b),
                BinOp::LessThanOrEqual => Value::Bool(a <= b),
                BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                _ => return unsupported("this operator", span),
            },
            (Value::Float(a), Value::Float(b), Ty::Float(float)) => {
                let round = |x: f64| match float {
                    FloatTy::F32 => (x as f32) as f64,
                    FloatTy::F64 => x,
                };
                let (a, b) = (round(a), round(b));
                match op {
                    BinOp::Add => Value::Float(round(a + b)),
                    BinOp::Subtract => Value::Float(round(a - b)),
                    BinOp::Multiply => Value::Float(round(a * b)),
                    BinOp::Divide => Value::Float(round(a / b)),
                    BinOp::LessThan => Value::Bool(a < b),
                    BinOp::GreaterThan => Value::Bool(a > b),
                    BinOp::LessThanOrEqual => Value::Bool(a <= b),
                    BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                    BinOp::Equals => Value::Bool(a == b),
                    BinOp::NotEquals => Value::Bool(a != b),
                    _ => return unsupported("this operator on floats", span),
                }
            }
            // Pointers are equal when they point to the same place.
            (a @ (Value::Ref(_) | Value::Null), b @ (Value::Ref(_) | Value::Null), _)
                if matches!(op, BinOp::Equals | BinOp::NotEquals) =>
            {
                let same = match (&a, &b) {
                    // An address C gave into a buffer it was given is that allocation's.
                    (Value::Ref(a), Value::Ref(b)) => self.unpinned(a) == self.unpinned(b),
                    (Value::Null, Value::Null) => true,
                    _ => false,
                };
                Value::Bool(same == (op == BinOp::Equals))
            }
            (Value::Bool(a), Value::Bool(b), _) => match op {
                BinOp::And => Value::Bool(a && b),
                BinOp::Or => Value::Bool(a || b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                _ => return unsupported("this operator on bools", span),
            },
            _ => return unsupported("this operator", span),
        })
    }

    pub(crate) fn cast(
        &self,
        value: Value,
        from: TypeId,
        to: TypeId,
        span: &Span,
    ) -> Result<Value, Stop> {
        let types = &self.mir().types;
        let pointee = |t: TypeId| match types.get(t) {
            Ty::Ref { to, .. } => Some(types.get(*to)),
            _ => None,
        };
        // A reference to an array, as a slice of its elements.
        if let (Some(Ty::Array { len, .. }), Some(Ty::Slice(_))) = (pointee(from), pointee(to)) {
            let len = self.array_len(*len, span)?;
            return Ok(match value {
                Value::Ref(mut loc) => {
                    // In a local, its first element; in memory, it starts where it is.
                    if let Root::Local { .. } = loc.root {
                        loc.path.push(0);
                    }
                    Value::Slice {
                        start: Some(loc),
                        len,
                    }
                }
                _ => return unsupported("this conversion", span),
            });
        }
        // `[mut T]` as `[T]`, and pointers as other pointers: where they point is the same.
        let points = |t: &Ty| matches!(t, Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. });
        if points(types.get(from)) && points(types.get(to)) {
            return Ok(match value {
                Value::Uninit => Value::Null,
                value => value,
            });
        }
        Ok(match (value, types.get(from), types.get(to)) {
            (Value::Int(n), _, Ty::Int(int)) => Value::Int(wrap(n, *int)),
            (Value::Int(n), _, Ty::Float(float)) => Value::Float(round(n as f64, *float)),
            (Value::Int(n), _, Ty::Char) => Value::Int(n),
            (Value::Bool(b), _, Ty::Int(_)) => Value::Int(b as i128),
            // To the integer nearest zero, saturating; NaN is 0.
            (Value::Float(x), _, Ty::Int(int)) => {
                let (min, max) = bounds(*int);
                Value::Int(if x.is_nan() {
                    0
                } else if x <= min as f64 {
                    min
                } else if x >= max as f64 {
                    max
                } else {
                    x.trunc() as i128
                })
            }
            (Value::Float(x), _, Ty::Float(float)) => Value::Float(round(x, *float)),
            (value, a, b) if a == b => value,
            _ => return unsupported("this conversion", span),
        })
    }

    /// `value`, of type `ty`, as a constant in the program, or what it is that can't be one.
    /// Text built while compiling becomes a literal.
    pub(crate) fn bake(&self, value: &Value, ty: TypeId) -> Result<ConstKind, Unkeepable> {
        let mir = self.mir();
        let types = &mir.types;
        let span = &mir.funcs[self.frames.first().map_or(Id::new(0), |f| f.func)].span;
        let memory = || {
            (
                "a reference to memory that only exists while compiling",
                Vec::new(),
            )
        };
        // Text: its bytes, kept as a literal.
        let literal = |bytes: Result<Vec<u8>, Stop>| match bytes.ok().map(String::from_utf8) {
            Some(Ok(text)) => Ok(ConstKind::Str(text)),
            Some(Err(_)) => Err(("text that isn't UTF-8", Vec::new())),
            None => Err(memory()),
        };
        if ty == types.text && matches!(types.get(ty), Ty::Struct(..)) {
            return match self.string(value, span) {
                Ok(text) => Ok(ConstKind::Str(text)),
                Err(_) => Err(memory()),
            };
        }
        match (value, types.get(ty)) {
            (Value::Ref(loc), Ty::CStr) => return literal(self.c_text(loc, span)),
            (Value::Slice { start, len }, Ty::Ref { to, .. })
                if matches!(types.get(*to), Ty::Str) =>
            {
                return literal(match start {
                    Some(start) => self.data(start, *len, span),
                    None => Ok(Vec::new()),
                });
            }
            _ => {}
        }
        // Its parts, with their types, and how each is named.
        let parts = |parts: Vec<(&Value, TypeId, String)>| -> Result<ConstKind, Unkeepable> {
            let mut baked = Vec::new();
            for (value, ty, name) in parts {
                let kind = self.bake(value, ty).map_err(|(what, mut at)| {
                    at.insert(0, name);
                    (what, at)
                })?;
                baked.push(Const { kind, ty });
            }
            Ok(ConstKind::Aggregate(baked))
        };
        const UNSET: &Value = &Value::Uninit;
        fn nth(values: &[Value], i: usize) -> &Value {
            values.get(i).unwrap_or(UNSET)
        }
        Ok(match value {
            Value::Int(n) => ConstKind::Int(*n),
            Value::Float(x) if x.is_finite() => ConstKind::Float(*x),
            Value::Float(_) => return Err(("infinite, or not a number", Vec::new())),
            Value::Bool(b) => ConstKind::Bool(*b),
            Value::Unit => ConstKind::Unit,
            Value::Func(f) => ConstKind::Func(*f),
            // Never set: a field of a variant it isn't, or `none`'s value.
            Value::Uninit | Value::Null => ConstKind::Zeroed,
            Value::Agg(values) => match types.get(ty) {
                Ty::Struct(def, args) if !mir.struct_def(*def, args).handle => {
                    let s = mir.struct_def(*def, args);
                    parts(
                        (s.fields.iter().enumerate())
                            .map(|(i, (name, ty))| (nth(values, i), *ty, format!(".{name}")))
                            .collect(),
                    )?
                }
                Ty::Optional(inner) => parts(vec![
                    (nth(values, 0), types.bool, String::new()),
                    (nth(values, 1), *inner, String::new()),
                ])?,
                Ty::Array { element, .. } => parts(
                    (values.iter().enumerate())
                        .map(|(i, v)| (v, *element, format!("[{i}]")))
                        .collect(),
                )?,
                Ty::Range(int) => parts(vec![
                    (nth(values, 0), *int, ".start".into()),
                    (nth(values, 1), *int, ".end".into()),
                ])?,
                _ => return Err(("a kind of value that can't be kept yet", Vec::new())),
            },
            Value::Ref(_) | Value::Slice { .. } => return Err(memory()),
        })
    }
}

//! Checks MIR is well formed and well typed. A failure is a bug in the pass that produced
//! the MIR, not in the program being compiled.

use crate::*;

impl Mir {
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for func in &self.funcs {
            if let Some(body) = &func.body {
                let mut v = Validator {
                    mir: self,
                    func,
                    body,
                    problems: &mut problems,
                };
                v.check();
            }
        }
        problems
    }
}

struct Validator<'a> {
    mir: &'a Mir,
    func: &'a Function,
    body: &'a Body,
    problems: &'a mut Vec<String>,
}

impl Validator<'_> {
    fn problem(&mut self, message: String) {
        self.problems.push(format!("{}: {message}", self.func.name));
    }

    fn check(&mut self) {
        if !self.check_signature() {
            return;
        }
        for (i, block) in self.body.blocks.iter().enumerate() {
            for stmt in &block.stmts {
                self.check_stmt(i, &stmt.kind);
            }
            self.check_terminator(i, &block.terminator.kind);
        }
    }

    /// False if the locals can't even hold the return value and parameters.
    fn check_signature(&mut self) -> bool {
        if self.body.locals.len() <= self.body.arg_count {
            self.problem("fewer locals than return value and parameters".into());
            return false;
        }
        if self.body.local(RETURN).ty != self.func.ret {
            self.problem("_0 doesn't have the return type".into());
        }
        let args: Vec<TypeId> = self.body.args().map(|l| self.body.local(l).ty).collect();
        if args != self.func.params {
            self.problem("parameter locals don't match the signature".into());
        }
        if self.body.blocks.is_empty() {
            self.problem("no blocks".into());
        }
        true
    }

    fn check_stmt(&mut self, block: usize, stmt: &StatementKind) {
        match stmt {
            StatementKind::Assign(place, rvalue) => {
                let (Some(place_ty), Some(value_ty)) =
                    (self.place_ty(place), self.rvalue_ty(rvalue))
                else {
                    return;
                };
                if !self.same(place_ty, value_ty) {
                    let (p, v) = (self.mir.display_ty(place_ty), self.mir.display_ty(value_ty));
                    self.problem(format!("bb{block}: assigns {v} to {place} of type {p}"));
                }
            }
            StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                self.local_ty(*local);
            }
            StatementKind::Drop { place, flag, .. } => {
                self.place_ty(place);
                if let Some(flag) = flag {
                    self.local_ty(*flag);
                }
            }
        }
    }

    fn check_terminator(&mut self, block: usize, terminator: &TerminatorKind) {
        match terminator {
            TerminatorKind::Goto(target) => self.target(*target),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                if let Some(ty) = self.operand_ty(condition)
                    && ty != self.mir.types.bool
                {
                    let ty = self.mir.display_ty(ty);
                    self.problem(format!("bb{block}: branches on a {ty}"));
                }
                self.target(*then);
                self.target(*otherwise);
            }
            TerminatorKind::Return | TerminatorKind::Unreachable => {}
        }
    }

    fn target(&mut self, block: BlockId) {
        if self.body.blocks.get(block).is_none() {
            self.problem(format!("jumps to missing {block}"));
        }
    }

    /// An error type from an earlier pass matches anything.
    fn same(&self, a: TypeId, b: TypeId) -> bool {
        a == b || a == self.mir.types.error || b == self.mir.types.error
    }

    fn local_ty(&mut self, local: Local) -> Option<TypeId> {
        match self.body.locals.get(local) {
            Some(decl) => Some(decl.ty),
            None => {
                self.problem(format!("uses missing local {local}"));
                None
            }
        }
    }

    /// Unlike `Mir::place_ty`, reports what's wrong instead of panicking.
    fn place_ty(&mut self, place: &Place) -> Option<TypeId> {
        let mut ty = self.local_ty(place.local)?;
        for &projection in &place.projection {
            if let Projection::Index(index) = projection
                && let Some(index_ty) = self.local_ty(index)
                && !matches!(self.mir.types.get(index_ty), Ty::Int(IntTy::USize))
            {
                self.problem(format!("{place}: index {index} isn't a usize"));
            }
            ty = match self.mir.project(ty, projection) {
                Some(ty) => ty,
                None => {
                    let ty = self.mir.display_ty(ty);
                    self.problem(format!("{place}: can't apply {projection:?} to {ty}"));
                    return None;
                }
            };
        }
        Some(ty)
    }

    fn operand_ty(&mut self, operand: &Operand) -> Option<TypeId> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_ty(place),
            Operand::Const(c) => Some(c.ty),
        }
    }

    fn rvalue_ty(&mut self, rvalue: &Rvalue) -> Option<TypeId> {
        match rvalue {
            Rvalue::Use(op) => self.operand_ty(op),
            Rvalue::Ref { mutable, place } => {
                let to = self.place_ty(place)?;
                let found = self.mir.types.lookup(&Ty::Ref {
                    mutable: *mutable,
                    to,
                });
                if found.is_none() {
                    self.problem(format!("&{place}: reference type was never interned"));
                }
                found
            }
            Rvalue::Binary { op, lhs, rhs } => self.binary_ty(*op, lhs, rhs),
            Rvalue::Not(op) => {
                let ty = self.operand_ty(op)?;
                Some(match self.mir.types.get(ty) {
                    Ty::Int(_) => ty,
                    _ => self.mir.types.bool,
                })
            }
            Rvalue::Array { ty, items } => {
                let Ty::Array { element, len } = *self.mir.types.get(*ty) else {
                    self.problem("array literal of a type that isn't an array".into());
                    return None;
                };
                let expected = vec![element; self.mir.types.len(len) as usize];
                self.check_operands("array", &expected, items);
                Some(*ty)
            }
            Rvalue::Repeat { ty, operand } => {
                let Ty::Array { element, .. } = *self.mir.types.get(*ty) else {
                    self.problem("array literal of a type that isn't an array".into());
                    return None;
                };
                self.check_operands("array", &[element], std::slice::from_ref(operand));
                Some(*ty)
            }
            Rvalue::Range { ty, start, end } => {
                let Ty::Range(int) = *self.mir.types.get(*ty) else {
                    self.problem("range of a type that isn't a range".into());
                    return None;
                };
                self.check_operands("range", &[int, int], &[start.clone(), end.clone()]);
                Some(*ty)
            }
            Rvalue::MakeSlice { ty, start, len } => self.make_slice_ty(*ty, start, len),
            Rvalue::Cast { operand, ty } => self.cast_ty(operand, *ty),
            Rvalue::Some(op) => {
                let inner = self.operand_ty(op)?;
                let found = self.mir.types.lookup(&Ty::Optional(inner));
                if found.is_none() {
                    self.problem("optional type was never interned".into());
                }
                found
            }
            Rvalue::Struct {
                ty,
                variant,
                fields,
            } => self.struct_ty(*ty, *variant, fields),
            Rvalue::Intrinsic { args, ty, .. } => {
                for arg in args {
                    self.operand_ty(arg);
                }
                Some(*ty)
            }
            Rvalue::Call { func, args } => self.call_ty(*func, args),
            Rvalue::CallIndirect { callee, args } => {
                let ty = self.operand_ty(callee)?;
                let Ty::Fn { params, ret } = self.mir.types.get(ty).clone() else {
                    self.problem("calls what isn't a function".to_string());
                    return None;
                };
                self.check_operands("indirect call", &params, args);
                Some(ret)
            }
        }
    }

    fn binary_ty(&mut self, op: BinOp, lhs: &Operand, rhs: &Operand) -> Option<TypeId> {
        let (lhs, rhs) = (self.operand_ty(lhs)?, self.operand_ty(rhs)?);
        let types = &self.mir.types;
        let pointer_offset = op == BinOp::Add && lhs == types.pointer;
        // A shift's amount is any integer.
        let shift = matches!(op, BinOp::ShiftLeft | BinOp::ShiftRight)
            && matches!(types.get(lhs), Ty::Int(_))
            && matches!(types.get(rhs), Ty::Int(_));
        if !pointer_offset && !shift && !self.same(lhs, rhs) {
            self.problem(format!(
                "{op:?} on {} and {}",
                self.mir.display_ty(lhs),
                self.mir.display_ty(rhs)
            ));
        }
        Some(if op.is_arithmetic() {
            lhs
        } else {
            self.mir.types.bool
        })
    }

    fn make_slice_ty(&mut self, ty: TypeId, start: &Operand, len: &Operand) -> Option<TypeId> {
        let viewed = match self.mir.types.get(ty) {
            Ty::Ref { to, .. } => self.mir.types.get(*to).clone(),
            _ => Ty::Error,
        };
        let element = match viewed {
            Ty::Slice(element) => element,
            Ty::Str => self.mir.types.u8,
            _ => {
                self.problem("slice made of a type that isn't a slice".into());
                return None;
            }
        };
        if let Some(start_ty) = self.operand_ty(start)
            && !matches!(self.mir.types.get(start_ty), Ty::Ref { to, .. } if *to == element)
        {
            self.problem("slice made from something other than a reference to an element".into());
        }
        if let Some(len_ty) = self.operand_ty(len)
            && !matches!(self.mir.types.get(len_ty), Ty::Int(IntTy::USize))
        {
            self.problem("slice made with a length that isn't a usize".into());
        }
        Some(ty)
    }

    fn cast_ty(&mut self, operand: &Operand, ty: TypeId) -> Option<TypeId> {
        let from = self.operand_ty(operand)?;
        let types = &self.mir.types;
        let number = |t: TypeId| matches!(types.get(t), Ty::Int(_) | Ty::Float(_));
        let viewed = |t: TypeId| match types.get(t) {
            Ty::Ref { to, .. } => Some(types.get(*to)),
            _ => None,
        };
        let slicing = matches!(viewed(ty), Some(Ty::Slice(_)))
            && matches!(
                viewed(from),
                Some(Ty::Array { .. } | Ty::Slice(_) | Ty::Str)
            );
        let pointer = |t: TypeId| matches!(types.get(t), Ty::Pointer | Ty::Ptr(_));
        let reference = |t: TypeId| matches!(types.get(t), Ty::Ref { .. });
        // C's text is a pointer to its bytes: `bytes as cstr`, `text as *u8`.
        let cstr = |t: TypeId| matches!(types.get(t), Ty::CStr);
        let pointers = ((pointer(from) || reference(from)) && pointer(ty))
            || (cstr(from) && pointer(ty))
            || (pointer(from) && cstr(ty));
        if !(number(from) && number(ty)) && !slicing && !pointers {
            let (f, t) = (self.mir.display_ty(from), self.mir.display_ty(ty));
            self.problem(format!("cast from {f} to {t}: those don't convert"));
        }
        Some(ty)
    }

    fn struct_ty(
        &mut self,
        ty: TypeId,
        variant: Option<usize>,
        fields: &[Operand],
    ) -> Option<TypeId> {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            self.problem("struct literal of a type that isn't a struct".into());
            return None;
        };
        if !self.mir.has_struct(*def, args) {
            let name = self.mir.display_ty(ty);
            self.problem(format!("struct literal of {name}, which has no struct"));
            return None;
        }
        let s = self.mir.struct_def(*def, args);
        let expected: Vec<TypeId> = match variant {
            Some(v) if s.variants.as_ref().is_some_and(|vs| v < vs.len()) => {
                s.variant_fields(v).iter().map(|&i| s.fields[i].1).collect()
            }
            Some(_) => {
                self.problem("variant literal of a variant its type doesn't have".into());
                return None;
            }
            None => s.fields.iter().map(|(_, t)| *t).collect(),
        };
        self.check_operands("struct", &expected, fields);
        Some(ty)
    }

    fn call_ty(&mut self, func: FuncId, args: &[Operand]) -> Option<TypeId> {
        let Some(callee) = self.mir.funcs.get(func) else {
            self.problem(format!("calls missing function {}", func.id));
            return None;
        };
        // A variadic extern's extra arguments can be anything.
        let fixed = if callee.variadic {
            &args[..callee.params.len().min(args.len())]
        } else {
            args
        };
        if callee.variadic && args.len() < callee.params.len() {
            self.problem(format!("call to {} has too few arguments", callee.name));
        }
        self.check_operands(&format!("call to {}", callee.name), &callee.params, fixed);
        Some(callee.ret)
    }

    fn check_operands(&mut self, what: &str, expected: &[TypeId], given: &[Operand]) {
        if expected.len() != given.len() {
            self.problem(format!(
                "{what}: expected {} operands, got {}",
                expected.len(),
                given.len()
            ));
            return;
        }
        for (&expected, op) in expected.iter().zip(given) {
            if let Some(ty) = self.operand_ty(op)
                && !self.same(expected, ty)
            {
                let (e, t) = (self.mir.display_ty(expected), self.mir.display_ty(ty));
                self.problem(format!("{what}: expected {e}, got {t}"));
            }
        }
    }
}

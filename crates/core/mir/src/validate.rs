//! Checks that MIR is well formed: every id is in range and every assignment,
//! call and branch is well typed. A failure here is a bug in the pass that
//! produced the MIR, not in the program being compiled.

use crate::*;

impl Mir {
    /// Returns a description of every problem found, or nothing if the MIR is well formed.
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
        if self.body.locals.len() <= self.body.arg_count {
            self.problem("fewer locals than return value and parameters".into());
            return;
        }
        if self.body.local(Local::RETURN).ty != self.func.ret {
            self.problem("_0 doesn't have the return type".into());
        }
        let args: Vec<TypeId> = self.body.args().map(|l| self.body.local(l).ty).collect();
        if args != self.func.params {
            self.problem("parameter locals don't match the signature".into());
        }
        if self.body.blocks.is_empty() {
            self.problem("no blocks".into());
        }

        for (i, block) in self.body.blocks.iter().enumerate() {
            for stmt in &block.stmts {
                match &stmt.kind {
                    StatementKind::Assign(place, rvalue) => {
                        let (Some(place_ty), Some(value_ty)) =
                            (self.place_ty(place), self.rvalue_ty(rvalue))
                        else {
                            continue;
                        };
                        if !self.same(place_ty, value_ty) {
                            let (p, v) =
                                (self.mir.display_ty(place_ty), self.mir.display_ty(value_ty));
                            self.problem(format!("bb{i}: assigns {v} to {place} of type {p}"));
                        }
                    }
                    StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                        self.local_ty(*local);
                    }
                }
            }
            match &block.terminator.kind {
                TerminatorKind::Goto(target) => self.target(*target),
                TerminatorKind::Branch {
                    condition,
                    then,
                    otherwise,
                } => {
                    if let Some(ty) = self.operand_ty(condition)
                        && ty != self.mir.types.bool
                    {
                        self.problem(format!("bb{i}: branches on a {}", self.mir.display_ty(ty)));
                    }
                    self.target(*then);
                    self.target(*otherwise);
                }
                TerminatorKind::Return | TerminatorKind::Unreachable => {}
            }
        }
    }

    fn target(&mut self, block: BlockId) {
        if block.0 as usize >= self.body.blocks.len() {
            self.problem(format!("jumps to missing {block}"));
        }
    }

    /// Types are equal, or one of them is an error type from an earlier pass.
    fn same(&self, a: TypeId, b: TypeId) -> bool {
        a == b || a == self.mir.types.error || b == self.mir.types.error
    }

    fn local_ty(&mut self, local: Local) -> Option<TypeId> {
        match self.body.locals.get(local.0 as usize) {
            Some(decl) => Some(decl.ty),
            None => {
                self.problem(format!("uses missing local {local}"));
                None
            }
        }
    }

    fn place_ty(&mut self, place: &Place) -> Option<TypeId> {
        let mut ty = self.local_ty(place.local)?;
        for projection in &place.projection {
            ty = match (projection, self.mir.types.get(ty)) {
                (Projection::Deref, Ty::Ref { to, .. }) => *to,
                (Projection::Field(i), Ty::Struct(def)) => {
                    match self.mir.struct_def(*def).fields.get(*i) {
                        Some((_, field_ty)) => *field_ty,
                        None => {
                            self.problem(format!("{place}: no field {i}"));
                            return None;
                        }
                    }
                }
                _ => {
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
        let types = &self.mir.types;
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
            Rvalue::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (self.operand_ty(lhs)?, self.operand_ty(rhs)?);
                let arithmetic = matches!(
                    op,
                    BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
                );
                let pointer_offset = *op == BinOp::Add && lhs == types.pointer;
                if !pointer_offset && !self.same(lhs, rhs) {
                    self.problem(format!(
                        "{op:?} on {} and {}",
                        self.mir.display_ty(lhs),
                        self.mir.display_ty(rhs)
                    ));
                }
                Some(if arithmetic { lhs } else { self.mir.types.bool })
            }
            Rvalue::Not(op) => {
                self.operand_ty(op);
                Some(types.bool)
            }
            Rvalue::Struct { def, fields } => {
                let expected: Vec<TypeId> = self
                    .mir
                    .struct_def(*def)
                    .fields
                    .iter()
                    .map(|(_, t)| *t)
                    .collect();
                self.check_operands("struct", &expected, fields);
                self.mir.types.lookup(&Ty::Struct(*def))
            }
            Rvalue::Call { func, args } => {
                let Some(callee) = self.mir.funcs.get(func.0 as usize) else {
                    self.problem(format!("calls missing function {}", func.0));
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
        }
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

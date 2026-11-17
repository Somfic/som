use crate::Emitter;
use crate::fold::{
    Expr, FnState, Pending, Usage, address_taken, is_zero, operand_local, statement_reads,
};
use crate::names::local_names;
use crate::runtime::UNREACHABLE;
use som_mir::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// `_0 = …` written as the block's last line, which a `return` can take over: its line, and
/// the value.
type Returned = Option<(usize, String)>;

impl<'a> Emitter<'a> {
    pub(crate) fn function(&mut self, id: FuncId, func: &Function, body: &'a Body) {
        let functions: HashSet<String> = self.names.funcs.values().cloned().collect();
        let mut f = FnState {
            body,
            locals: local_names(self.mir, body, &functions),
            usage: Usage::of(self.mir, body),
            pending: RefCell::new(HashMap::new()),
            folded: RefCell::default(),
            address_taken: address_taken(body),
            mentioned: RefCell::new(HashMap::new()),
            first: RefCell::default(),
            line: Cell::new(0),
        };

        // Only blocks something jumps to need a label. Every block ends in a jump or a
        // return, so control never falls from one block into the next.
        let targets: HashSet<BlockId> = body
            .blocks
            .iter()
            .flat_map(|b| b.terminator.kind.successors())
            .collect();
        let mut code = Vec::new();
        for (id, block) in body.blocks.iter_with_ids() {
            if targets.contains(&id) {
                code.push(format!("{id}:"));
            }
            self.block(&mut f, block, &mut code);
        }

        let signature = self.signature(id, func, Some(&f.locals));
        self.line(format!("// {}", func.name));
        self.line(format!("{signature} {{"));
        self.declare_locals(&f, &mut code);
        for line in code {
            self.line(line);
        }
        self.line("}");
        self.line("");
    }

    /// A local is declared where it's first assigned, if that's the first time it's
    /// mentioned: everything after is in its scope, as a function is one flat scope, and C
    /// allows jumping past a declaration. Before C23 a label can't come right before one,
    /// and anything else is declared at the top.
    fn declare_locals(&mut self, f: &FnState, code: &mut [String]) {
        let body = f.body;
        let mentioned = f.mentioned.borrow();
        let first = f.first.borrow();
        for (id, local) in body
            .locals
            .iter_with_ids()
            .skip(body.arg_count + 1)
            .chain([(RETURN, &body.locals[RETURN])])
        {
            if !mentioned.get(&id).is_some_and(|&n| n > 0) {
                continue;
            }
            let (ty, name) = (self.ty(local.ty), &f.locals[id.id]);
            let assign = format!("    {name} = ");
            if let Some(&line) = first.get(&id)
                && code.get(line).is_some_and(|l| l.starts_with(&assign))
                && (line == 0 || !code[line - 1].ends_with(':'))
            {
                code[line] = format!("    {ty} {}", &code[line][4..]);
            } else {
                self.line(format!("    {ty} {name};"));
            }
        }
    }

    fn block(&self, f: &mut FnState, block: &Block, code: &mut Vec<String>) {
        let mut returned: Returned = None;
        for stmt in &block.stmts {
            match &stmt.kind {
                StatementKind::Drop { place, glue, flag } => {
                    self.drop(f, place, *glue, *flag, code);
                    returned = None;
                }
                StatementKind::Assign(dest, rvalue) => {
                    self.assign(f, dest, rvalue, code, &mut returned)
                }
                _ => {}
            }
        }

        let condition = match &block.terminator.kind {
            TerminatorKind::Branch { condition, .. } => operand_local(condition),
            _ => None,
        };
        self.flush_except(f, &condition.into_iter().collect::<Vec<_>>(), code);
        f.line.set(code.len());
        self.terminator(f, &block.terminator.kind, returned, code);
    }

    /// `som_drop_T(&x);`, or with a flag, only if it's set.
    fn drop(
        &self,
        f: &mut FnState,
        place: &Place,
        glue: FuncId,
        flag: Option<Local>,
        code: &mut Vec<String>,
    ) {
        self.flush_except(f, &[], code);
        f.line.set(code.len());
        let call = format!("{}(&{})", self.func_name(glue), self.place(f, place));
        code.push(match flag {
            Some(flag) => {
                f.mention(flag);
                format!("    if ({}) {call};", f.locals[flag.id])
            }
            None => format!("    {call};"),
        });
    }

    fn assign(
        &self,
        f: &mut FnState,
        dest: &Place,
        rvalue: &Rvalue,
        code: &mut Vec<String>,
        returned: &mut Returned,
    ) {
        self.flush_before(f, dest, rvalue, code);
        *f.folded.borrow_mut() = Default::default();
        f.line.set(code.len());

        if let Rvalue::Repeat { ty, operand } = rvalue
            && !is_zero(operand)
        {
            self.repeat(f, dest, *ty, operand, code);
            *returned = None;
            return;
        }

        let value = self.rvalue(f, rvalue);
        let local = dest.local;
        // A call to an extern returning `void` has no value.
        let void = matches!(rvalue, Rvalue::Call { func, .. }
            if self.ret_ty(self.mir.func(*func)) == "void");
        let is_temp = local != RETURN && f.body.local(local).name.is_none();
        if is_temp && dest.projection.is_empty() {
            let narrow = self.is_narrow(f.body.local(local).ty);
            let (assigns, reads) = (f.usage.assigns[local.id], f.usage.reads[local.id]);
            if assigns == 1
                && reads == 1
                && !void
                && f.usage.read_as[local.id].allows(rvalue, narrow)
            {
                let pending = self.pending(f, rvalue, value);
                f.pending.borrow_mut().insert(local, pending);
                return;
            }
            // Nothing reads it: a call is kept for what it does, anything else dropped.
            if reads == 0 {
                let effect = match rvalue {
                    Rvalue::Call { .. } => true,
                    Rvalue::Intrinsic { intrinsic, .. } => intrinsic.has_effect(),
                    _ => false,
                };
                if effect {
                    code.push(format!("    {};", value.text));
                }
                *returned = None;
                return;
            }
        }

        let place = self.place(f, dest);
        if void {
            code.push(format!("    {}; {place} = 0;", value.text));
            *returned = None;
            return;
        }
        code.push(format!("    {place} = {};", value.text));
        *returned =
            (local == RETURN && dest.projection.is_empty()).then(|| (code.len() - 1, value.text));
    }

    /// `[value; N]`: C has no expression for that, so a loop fills it in.
    fn repeat(
        &self,
        f: &mut FnState,
        dest: &Place,
        ty: TypeId,
        operand: &Operand,
        code: &mut Vec<String>,
    ) {
        let Ty::Array { len, .. } = *self.mir.types.get(ty) else {
            unreachable!("repeats make arrays")
        };
        let len = self.mir.types.len(len);
        let value = self.operand(f, operand).text;
        let place = self.place(f, dest);
        code.push(format!(
            "    for (uintptr_t som_i = 0; som_i < {len}; som_i++) {place}.items[som_i] = {value};"
        ));
    }

    fn terminator(
        &self,
        f: &mut FnState,
        terminator: &TerminatorKind,
        returned: Returned,
        code: &mut Vec<String>,
    ) {
        match terminator {
            TerminatorKind::Goto(target) => code.push(format!("    goto {target};")),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.operand(f, condition).text;
                code.push(format!(
                    "    if ({condition}) goto {then}; else goto {otherwise};"
                ));
            }
            TerminatorKind::Return => match returned {
                // `_0 = x; return _0;` is `return x;`, if nothing was written in between.
                Some((line, value)) if line == code.len() - 1 => {
                    code[line] = format!("    return {value};");
                    *f.mentioned.borrow_mut().entry(RETURN).or_default() -= 1;
                }
                _ => {
                    f.line.set(code.len());
                    f.mention(RETURN);
                    code.push(format!("    return {};", f.locals[0]));
                }
            },
            TerminatorKind::Unreachable => code.push(format!("    {UNREACHABLE}();")),
        }
    }

    /// The temporary assigned `rvalue`, written as `expr`, waiting to be folded: what it
    /// depends on, including the temporaries already folded into it.
    fn pending(&self, f: &FnState, rvalue: &Rvalue, expr: Expr) -> Pending {
        let (mut reads, mut memory, mut effect) = f.folded.take();
        for place in rvalue.places() {
            reads.extend(place.locals());
            memory |= place.projection.contains(&Projection::Deref);
        }
        memory |= reads.iter().any(|l| f.address_taken.contains(l));
        match rvalue {
            Rvalue::Call { .. } => effect = true,
            Rvalue::Intrinsic { intrinsic, .. } => {
                memory = true;
                effect |= intrinsic.has_effect();
            }
            _ => {}
        }
        Pending {
            expr,
            reads,
            memory,
            effect,
        }
    }

    /// Before `dest = rvalue`, writes out the pending temporaries it doesn't read and would
    /// change the value of: those that read a local it assigns, or memory it may write. A
    /// temporary with an effect is written out unless read right away, so effects stay in
    /// order.
    fn flush_before(&self, f: &FnState, dest: &Place, rvalue: &Rvalue, code: &mut Vec<String>) {
        let read = statement_reads(dest, rvalue);
        let pending = f.pending.borrow();
        let effect = matches!(rvalue, Rvalue::Call { .. })
            || matches!(rvalue, Rvalue::Intrinsic { intrinsic, .. } if intrinsic.has_effect())
            || read
                .iter()
                .any(|l| pending.get(l).is_some_and(|p| p.effect));
        let writes_memory = effect || !dest.projection.is_empty();
        let keep: Vec<Local> = (pending.iter())
            .filter(|(local, p)| {
                read.contains(local)
                    || !(p.effect || p.reads.contains(&dest.local) || p.memory && writes_memory)
            })
            .map(|(&local, _)| local)
            .collect();
        drop(pending);
        self.flush_except(f, &keep, code);
    }

    fn flush_except(&self, f: &FnState, keep: &[Local], code: &mut Vec<String>) {
        let mut flush: Vec<Local> = (f.pending.borrow().keys())
            .filter(|l| !keep.contains(l))
            .copied()
            .collect();
        flush.sort();
        for local in flush {
            let value = f.pending.borrow_mut().remove(&local).expect("pending");
            f.line.set(code.len());
            f.mention(local);
            code.push(format!("    {} = {};", f.locals[local.id], value.expr.text));
        }
    }
}

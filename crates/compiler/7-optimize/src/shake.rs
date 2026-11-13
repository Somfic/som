//! Tree shaking: removes the functions and structs a program never uses.
//!
//! Starting from the entry point, a function is used if a used function calls it
//! or takes it as a value, and a struct is used if a used function's locals or
//! signature mention it, or a used struct contains or points to it. Everything
//! else is dropped, so the output has no declarations for the hundreds of functions a
//! header import brings in, only for the few the program calls.
//!
//! Functions live in an arena, so removing some leaves the others' ids, and every
//! call to them, as they were.

use som_mir::*;
use std::collections::HashSet;

/// Removes everything `mir` can't reach from its entry point. A program without
/// one (a library) is left as it is.
pub fn shake(mir: &mut Mir) {
    let Some((entry, _)) = mir.funcs.iter_with_ids().find(|(_, f)| f.is_entry) else {
        return;
    };
    let funcs = used_funcs(mir, entry);
    let structs = used_structs(mir, &funcs);
    mir.funcs.retain(|id, _| funcs.contains(&id));
    mir.retain_structs(|s| structs.contains(&s.def));
}

/// The functions reachable from `entry`.
fn used_funcs(mir: &Mir, entry: FuncId) -> HashSet<FuncId> {
    let mut used = HashSet::from([entry]);
    let mut todo = vec![entry];
    while let Some(id) = todo.pop() {
        let Some(body) = &mir.func(id).body else {
            continue;
        };
        for callee in referenced(body) {
            if used.insert(callee) {
                todo.push(callee);
            }
        }
    }
    used
}

/// The functions `body` calls or takes as values.
fn referenced(body: &Body) -> Vec<FuncId> {
    let mut found = Vec::new();
    for block in &body.blocks {
        for stmt in &block.stmts {
            let StatementKind::Assign(_, rvalue) = &stmt.kind else {
                continue;
            };
            let operands: Vec<&Operand> = match rvalue {
                Rvalue::Use(op) | Rvalue::Not(op) | Rvalue::Some(op) => vec![op],
                Rvalue::Ref { .. } => Vec::new(),
                Rvalue::Binary { lhs, rhs, .. } => vec![lhs, rhs],
                Rvalue::Struct { fields, .. } => fields.iter().collect(),
                Rvalue::Call { func, args } => {
                    found.push(*func);
                    args.iter().collect()
                }
            };
            for op in operands {
                if let Operand::Const(Const {
                    kind: ConstKind::Func(id),
                    ..
                }) = op
                {
                    found.push(*id);
                }
            }
        }
    }
    found
}

/// The structs the used functions mention, and the structs those contain or point to.
fn used_structs(mir: &Mir, funcs: &HashSet<FuncId>) -> HashSet<DefId> {
    let mut types = Vec::new();
    for &id in funcs {
        let func = mir.func(id);
        types.extend(func.params.iter().copied());
        types.push(func.ret);
        if let Some(body) = &func.body {
            // Struct literals are assigned to locals, so their types are here too.
            types.extend(body.locals.iter().map(|l| l.ty));
        }
    }
    let mut used = HashSet::new();
    let mut seen = HashSet::new();
    while let Some(ty) = types.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match mir.types.get(ty) {
            Ty::Ref { to, .. } | Ty::Optional(to) => types.push(*to),
            Ty::Fn { params, ret } => {
                types.extend(params.iter().copied());
                types.push(*ret);
            }
            Ty::Struct(def) if used.insert(*def) => {
                types.extend(mir.struct_def(*def).fields.iter().map(|&(_, ty)| ty));
            }
            _ => {}
        }
    }
    used
}

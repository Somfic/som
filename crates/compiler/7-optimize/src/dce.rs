//! Dead code elimination: removes assignments whose values are never used, and the
//! locals they assigned.
//!
//! A local is live if something with an effect reads it, directly or through other live
//! locals. Statements with an effect are calls, writes through a reference, and anything
//! that could trap; the roots are those, `_0` (read by `return`) and branch conditions.
//! Every assignment to a local that isn't live is dropped, then every local nothing
//! mentions any more.
//!
//! Liveness is worked out for the whole body at once, not per program point, so a
//! local read anywhere keeps all its assignments. In return it catches locals that only
//! feed themselves, like a counter nothing reads: `i = i + 1`.
//!
//! Removing locals leaves the others' ids as they were. Parameters are kept even when
//! unused, since they're part of the function's signature.

use som_mir::*;

pub fn dce(mir: &mut Mir) {
    for func in mir.funcs.iter_mut() {
        if let Some(body) = &mut func.body {
            dce_body(body);
        }
    }
}

fn dce_body(body: &mut Body) {
    let live = live_locals(body);
    for block in body.blocks.iter_mut() {
        block.stmts.retain(|stmt| match &stmt.kind {
            StatementKind::Assign(dest, rvalue) => live[dest.local.id] || has_effect(dest, rvalue),
            StatementKind::StorageLive(l) | StatementKind::StorageDead(l) => live[l.id],
        });
    }

    // A kept call can still assign a local nothing reads, so look at what's left.
    let mut mentioned = live;
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        if let StatementKind::Assign(dest, _) = &stmt.kind {
            mentioned[dest.local.id] = true;
        }
    }
    for local in body.args().chain([RETURN]) {
        mentioned[local.id] = true;
    }
    body.locals.retain(|id, _| mentioned[id.id]);
}

/// Which locals something with an effect depends on, indexed by id.
fn live_locals(body: &Body) -> Vec<bool> {
    let mut live = vec![false; body.locals.slots()];
    live[RETURN.id] = true;
    for local in &body.outs {
        live[local.id] = true;
    }
    for block in body.blocks.iter() {
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            mark_operand(&mut live, condition);
        }
    }
    // Each round, a newly live local makes the assignments to it live, and so the
    // locals those read.
    loop {
        let mut changed = false;
        for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
            let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                continue;
            };
            if !live[dest.local.id] && !has_effect(dest, rvalue) {
                continue;
            }
            let before = live.iter().filter(|&&l| l).count();
            // Writing through a reference reads the reference.
            if dest.projection.contains(&Projection::Deref) {
                live[dest.local.id] = true;
            }
            match rvalue {
                Rvalue::Use(op) | Rvalue::Not(op) => mark_operand(&mut live, op),
                Rvalue::Ref { place, .. } => live[place.local.id] = true,
                Rvalue::Binary { lhs, rhs, .. } => {
                    mark_operand(&mut live, lhs);
                    mark_operand(&mut live, rhs);
                }
                Rvalue::Struct { fields: ops, .. } | Rvalue::Call { args: ops, .. } => {
                    ops.iter().for_each(|op| mark_operand(&mut live, op))
                }
            }
            changed |= live.iter().filter(|&&l| l).count() != before;
        }
        if !changed {
            return live;
        }
    }
}

fn mark_operand(live: &mut [bool], op: &Operand) {
    if let Operand::Copy(place) | Operand::Move(place) = op {
        live[place.local.id] = true;
    }
}

/// Whether the assignment does anything besides setting `dest`, so it has to stay
/// even if nothing reads `dest`.
fn has_effect(dest: &Place, rvalue: &Rvalue) -> bool {
    if dest.projection.contains(&Projection::Deref) {
        return true;
    }
    match rvalue {
        Rvalue::Call { .. } => true,
        // Dividing by zero, or `MIN / -1`, traps; leave that for run time, as folding does.
        // Only a constant divisor shows it can't.
        Rvalue::Binary {
            op: BinOp::Divide | BinOp::Modulo,
            rhs,
            ..
        } => !matches!(
            rhs,
            Operand::Const(Const { kind: ConstKind::Int(n), .. }) if *n != 0 && *n != -1
        ) && !matches!(
            rhs,
            Operand::Const(Const { kind: ConstKind::Float(_), .. })
        ),
        Rvalue::Use(_)
        | Rvalue::Not(_)
        | Rvalue::Ref { .. }
        | Rvalue::Binary { .. }
        | Rvalue::Struct { .. } => false,
    }
}

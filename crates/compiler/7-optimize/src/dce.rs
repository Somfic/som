//! Dead code elimination: removes assignments whose values are never used, and the
//! locals they assigned.
//!
//! Two analyses take turns until neither finds anything more:
//!
//! - **Unused locals.** A local is used if something with an effect reads it, directly
//!   or through other used locals. Statements with an effect are calls, writes through a
//!   reference, and anything that could trap; the roots are those, `_0` (read by
//!   `return`) and branch conditions. Every assignment to an unused local is dropped.
//!   This is worked out for the whole body at once, so it catches locals that only feed
//!   themselves, like a counter nothing reads: `i = i + 1`.
//! - **Dead stores.** A backward liveness analysis finds where a local is about to be
//!   overwritten, or never read again, so the value assigned before that is dropped:
//!   the first assignment in `a = 1; a = 2; return a`. Locals that are ever borrowed
//!   are left alone, since they can be read through the reference.
//!
//! Then every local nothing mentions any more is removed. That leaves the others' ids
//! as they were. Parameters are kept even when unused, since they're part of the
//! function's signature.

use som_mir::*;

pub fn dce(mir: &mut Mir) {
    for func in mir.funcs.iter_mut() {
        if let Some(body) = &mut func.body {
            dce_body(body);
        }
    }
}

fn dce_body(body: &mut Body) {
    while remove_unused(body) | remove_dead_stores(body) {}
    remove_unmentioned_locals(body);
}

// --- Unused locals ---

/// Drops the assignments to locals nothing with an effect depends on, and their
/// `StorageLive` and `StorageDead`. Returns whether anything changed.
fn remove_unused(body: &mut Body) -> bool {
    let used = used_locals(body);
    let mut changed = false;
    for block in body.blocks.iter_mut() {
        let before = block.stmts.len();
        block.stmts.retain(|stmt| match &stmt.kind {
            StatementKind::Assign(dest, rvalue) => used[dest.local.id] || has_effect(dest, rvalue),
            StatementKind::StorageLive(l) | StatementKind::StorageDead(l) => used[l.id],
        });
        changed |= block.stmts.len() != before;
    }
    changed
}

/// Which locals something with an effect depends on, indexed by id.
fn used_locals(body: &Body) -> Vec<bool> {
    let mut used = vec![false; body.locals.slots()];
    used[RETURN.id] = true;
    for local in &body.outs {
        used[local.id] = true;
    }
    for block in body.blocks.iter() {
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            read_operand(condition, |l| used[l.id] = true);
        }
    }
    // Each round, a newly used local makes the assignments to it used, and so the
    // locals those read.
    loop {
        let mut changed = false;
        for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
            let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                continue;
            };
            if used[dest.local.id] || has_effect(dest, rvalue) {
                reads(dest, rvalue, |l| {
                    changed |= !used[l.id];
                    used[l.id] = true;
                });
            }
        }
        if !changed {
            return used;
        }
    }
}

// --- Dead stores ---

/// Drops assignments whose value is overwritten, or never read, before anything reads
/// it. Returns whether anything changed.
fn remove_dead_stores(body: &mut Body) -> bool {
    let n = body.locals.slots();
    // Writes to a borrowed local can be read through the borrow, which this doesn't follow.
    let mut kept = vec![false; n];
    kept[RETURN.id] = true;
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        if let StatementKind::Assign(_, Rvalue::Ref { place, .. }) = &stmt.kind {
            kept[place.local.id] = true;
        }
    }

    // Which locals are live on entry to each block: read before they're overwritten.
    let mut live_in = vec![vec![false; n]; body.blocks.slots()];
    loop {
        let mut changed = false;
        for (id, block) in body.blocks.iter_with_ids() {
            let mut live = live_out(block, &live_in, n);
            for stmt in block.stmts.iter().rev() {
                if let StatementKind::Assign(dest, rvalue) = &stmt.kind
                    && !is_dead_store(dest, rvalue, &live, &kept)
                {
                    transfer(dest, rvalue, &mut live);
                }
            }
            if live != live_in[id.id] {
                live_in[id.id] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut changed = false;
    let ids: Vec<BlockId> = body.blocks.iter_with_ids().map(|(id, _)| id).collect();
    for id in ids {
        let mut live = live_out(&body.blocks[id], &live_in, n);
        let block = &mut body.blocks[id];
        // Walk backwards, so each statement is judged by what's live after it.
        let mut keep = vec![true; block.stmts.len()];
        for (i, stmt) in block.stmts.iter().enumerate().rev() {
            if let StatementKind::Assign(dest, rvalue) = &stmt.kind {
                if is_dead_store(dest, rvalue, &live, &kept) {
                    keep[i] = false;
                    changed = true;
                } else {
                    transfer(dest, rvalue, &mut live);
                }
            }
        }
        let mut keep = keep.into_iter();
        block.stmts.retain(|_| keep.next().unwrap_or(true));
    }
    changed
}

/// The locals live at the end of `block`: those its terminator reads, and those live on
/// entry to a block it can jump to.
fn live_out(block: &Block, live_in: &[Vec<bool>], n: usize) -> Vec<bool> {
    let mut live = vec![false; n];
    match &block.terminator.kind {
        TerminatorKind::Return => live[RETURN.id] = true,
        TerminatorKind::Branch { condition, .. } => read_operand(condition, |l| live[l.id] = true),
        TerminatorKind::Goto(_) | TerminatorKind::Unreachable => {}
    }
    for next in block.terminator.kind.successors() {
        for (l, &on_entry) in live.iter_mut().zip(&live_in[next.id]) {
            *l |= on_entry;
        }
    }
    live
}

fn is_dead_store(dest: &Place, rvalue: &Rvalue, live: &[bool], kept: &[bool]) -> bool {
    !live[dest.local.id] && !kept[dest.local.id] && !has_effect(dest, rvalue)
}

/// Updates `live` from after a kept assignment to before it.
fn transfer(dest: &Place, rvalue: &Rvalue, live: &mut [bool]) {
    // Assigning the whole local overwrites what it held; a field leaves the rest.
    if dest.projection.is_empty() {
        live[dest.local.id] = false;
    }
    reads(dest, rvalue, |l| live[l.id] = true);
}

// --- Locals ---

/// Removes the locals no statement mentions any more, other than `_0` and parameters.
fn remove_unmentioned_locals(body: &mut Body) {
    let mut mentioned = vec![false; body.locals.slots()];
    for local in body.args().chain([RETURN]).chain(body.outs.iter().copied()) {
        mentioned[local.id] = true;
    }
    for block in body.blocks.iter() {
        for stmt in &block.stmts {
            match &stmt.kind {
                StatementKind::Assign(dest, rvalue) => {
                    mentioned[dest.local.id] = true;
                    reads(dest, rvalue, |l| mentioned[l.id] = true);
                }
                StatementKind::StorageLive(l) | StatementKind::StorageDead(l) => {
                    mentioned[l.id] = true;
                }
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            read_operand(condition, |l| mentioned[l.id] = true);
        }
    }
    body.locals.retain(|id, _| mentioned[id.id]);
}

// --- Assignments ---

/// Calls `read` with each local the assignment reads.
fn reads(dest: &Place, rvalue: &Rvalue, mut read: impl FnMut(Local)) {
    // Writing through a reference reads the reference.
    if dest.projection.contains(&Projection::Deref) {
        read(dest.local);
    }
    match rvalue {
        Rvalue::Use(op) | Rvalue::Not(op) | Rvalue::Some(op) => read_operand(op, read),
        Rvalue::Ref { place, .. } => read(place.local),
        Rvalue::Binary { lhs, rhs, .. } => {
            read_operand(lhs, &mut read);
            read_operand(rhs, read);
        }
        Rvalue::Struct { fields: ops, .. } | Rvalue::Call { args: ops, .. } => {
            ops.iter().for_each(|op| read_operand(op, &mut read))
        }
    }
}

fn read_operand(op: &Operand, mut read: impl FnMut(Local)) {
    if let Operand::Copy(place) | Operand::Move(place) = op {
        read(place.local);
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
        } => {
            !matches!(
                rhs,
                Operand::Const(Const { kind: ConstKind::Int(n), .. }) if *n != 0 && *n != -1
            ) && !matches!(
                rhs,
                Operand::Const(Const {
                    kind: ConstKind::Float(_),
                    ..
                })
            )
        }
        Rvalue::Use(_)
        | Rvalue::Not(_)
        | Rvalue::Some(_)
        | Rvalue::Ref { .. }
        | Rvalue::Binary { .. }
        | Rvalue::Struct { .. } => false,
    }
}

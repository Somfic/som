//! Constant folding: works out at compile time what doesn't depend on the program's input.
//!
//! Each function's body is simplified until nothing changes:
//!
//! - `1 + 2` becomes `3`, `!true` becomes `false`.
//! - A local assigned a constant exactly once, and never borrowed, is replaced by that
//!   constant wherever it's read. The borrow checker has already made sure nothing reads
//!   a local before it's assigned, so every read sees that one value.
//! - A branch on a constant becomes a `goto`, and blocks nothing can reach any more are
//!   removed.
//!
//! The assignments left behind to locals nothing reads any more are removed by
//! [`dce`](crate::dce).
//!
//! A folded value has to be exactly what the program would have computed at run time.
//! Integers wrap to their type's width (the C backend compiles with `-fwrapv` for
//! this). Anything that traps (dividing by zero, `MIN / -1`) or depends on the target
//! (`isize` and `usize` past 32 bits) is left for run time.

use som_mir::*;
use std::collections::HashSet;

pub fn fold(mir: &mut Mir) {
    for func in mir.funcs.iter_mut() {
        if let Some(body) = &mut func.body {
            fold_body(&mir.types, body);
        }
    }
}

fn fold_body(types: &TypeTable, body: &mut Body) {
    loop {
        let mut changed = fold_rvalues(types, body);
        changed |= propagate(body);
        changed |= fold_branches(body);
        changed |= remove_unreachable(body);
        if !changed {
            break;
        }
    }
}

// --- Expressions ---

/// Replaces operations on constants with their results.
fn fold_rvalues(types: &TypeTable, body: &mut Body) -> bool {
    let mut changed = false;
    for block in body.blocks.iter_mut() {
        for stmt in &mut block.stmts {
            let StatementKind::Assign(_, rvalue) = &mut stmt.kind else {
                continue;
            };
            let folded = match rvalue {
                Rvalue::Binary {
                    op,
                    lhs: Operand::Const(lhs),
                    rhs: Operand::Const(rhs),
                } => binary(types, *op, lhs, rhs),
                Rvalue::Not(Operand::Const(Const {
                    kind: ConstKind::Bool(b),
                    ty,
                })) => Some(Const {
                    kind: ConstKind::Bool(!*b),
                    ty: *ty,
                }),
                _ => None,
            };
            if let Some(c) = folded {
                *rvalue = Rvalue::Use(Operand::Const(c));
                changed = true;
            }
        }
    }
    changed
}

/// `lhs op rhs`, or `None` if it can't be worked out now. Both operands have the same
/// type; the type checker made sure of that.
fn binary(types: &TypeTable, op: BinOp, lhs: &Const, rhs: &Const) -> Option<Const> {
    let ty = lhs.ty;
    let arithmetic = |kind| Some(Const { kind, ty });
    let compare = |b| {
        Some(Const {
            kind: ConstKind::Bool(b),
            ty: types.bool,
        })
    };
    match (&lhs.kind, &rhs.kind, types.get(ty)) {
        (ConstKind::Int(a), ConstKind::Int(b), Ty::Int(int)) => {
            let (a, b) = (value(*a, *int)?, value(*b, *int)?);
            match op {
                BinOp::Add => arithmetic(ConstKind::Int(wrap(a + b, *int)?)),
                BinOp::Subtract => arithmetic(ConstKind::Int(wrap(a - b, *int)?)),
                BinOp::Multiply => arithmetic(ConstKind::Int(wrap(a * b, *int)?)),
                // Dividing by zero, and `MIN / -1`, trap at run time.
                BinOp::Divide | BinOp::Modulo if b == 0 || traps(a, b, *int) => None,
                // Both truncate towards zero.
                BinOp::Divide => arithmetic(ConstKind::Int(wrap(a / b, *int)?)),
                BinOp::Modulo => arithmetic(ConstKind::Int(wrap(a % b, *int)?)),
                BinOp::LessThan => compare(a < b),
                BinOp::GreaterThan => compare(a > b),
                BinOp::LessThanOrEqual => compare(a <= b),
                BinOp::GreaterThanOrEqual => compare(a >= b),
                BinOp::Equals => compare(a == b),
                BinOp::NotEquals => compare(a != b),
                BinOp::And | BinOp::Or => None,
            }
        }
        (ConstKind::Float(a), ConstKind::Float(b), Ty::Float(float)) => {
            let (a, b) = (*a, *b);
            let result = |x: f64| {
                // `f32` arithmetic rounds to `f32` at every step.
                let x = match float {
                    FloatTy::F32 => (x as f32) as f64,
                    FloatTy::F64 => x,
                };
                // The C backend can't write infinity or NaN as a literal.
                x.is_finite().then(|| Const {
                    kind: ConstKind::Float(x),
                    ty,
                })
            };
            match (op, float) {
                (BinOp::Add, FloatTy::F32) => result((a as f32 + b as f32) as f64),
                (BinOp::Subtract, FloatTy::F32) => result((a as f32 - b as f32) as f64),
                (BinOp::Multiply, FloatTy::F32) => result((a as f32 * b as f32) as f64),
                (BinOp::Divide, FloatTy::F32) => result((a as f32 / b as f32) as f64),
                (BinOp::Add, _) => result(a + b),
                (BinOp::Subtract, _) => result(a - b),
                (BinOp::Multiply, _) => result(a * b),
                (BinOp::Divide, _) => result(a / b),
                (BinOp::LessThan, _) => compare(a < b),
                (BinOp::GreaterThan, _) => compare(a > b),
                (BinOp::LessThanOrEqual, _) => compare(a <= b),
                (BinOp::GreaterThanOrEqual, _) => compare(a >= b),
                (BinOp::Equals, _) => compare(a == b),
                (BinOp::NotEquals, _) => compare(a != b),
                // The C backend has no `%` on floats; leave it for C to report.
                (BinOp::Modulo | BinOp::And | BinOp::Or, _) => None,
            }
        }
        (ConstKind::Bool(a), ConstKind::Bool(b), _) => match op {
            BinOp::And => compare(*a && *b),
            BinOp::Or => compare(*a || *b),
            BinOp::Equals => compare(a == b),
            BinOp::NotEquals => compare(a != b),
            _ => None,
        },
        _ => None,
    }
}

/// The value of the literal `n` as an `int`: `-1` as a `u8` is 255.
fn value(n: i64, int: IntTy) -> Option<i128> {
    wrap(n as i128, int).map(i128::from)
}

/// Whether `a / b` is `MIN / -1` in a signed type, which traps on most CPUs. C widens
/// `i8` and `i16` first, so they don't there, but other backends may not.
fn traps(a: i128, b: i128, int: IntTy) -> bool {
    let min = match int {
        IntTy::I8 => i8::MIN.into(),
        IntTy::I16 => i16::MIN.into(),
        IntTy::I32 | IntTy::ISize => i32::MIN.into(),
        IntTy::I64 => i64::MIN.into(),
        _ => return false,
    };
    a == min && b == -1
}

/// `n` as a value of type `int`, wrapped to its width, or `None` if `ConstKind::Int`
/// can't hold it.
fn wrap(n: i128, int: IntTy) -> Option<i64> {
    let (bits, signed) = match int {
        IntTy::I8 => (8, true),
        IntTy::I16 => (16, true),
        IntTy::I32 => (32, true),
        IntTy::I64 => (64, true),
        IntTy::U8 => (8, false),
        IntTy::U16 => (16, false),
        IntTy::U32 => (32, false),
        IntTy::U64 => (64, false),
        // Their width depends on the target: only fold what fits in the narrowest.
        IntTy::ISize => return i32::try_from(n).ok().map(i64::from),
        IntTy::USize => return u32::try_from(n).ok().map(i64::from),
        IntTy::I128 | IntTy::U128 => return None,
    };
    let modulus = 1i128 << bits;
    let min = if signed { -modulus / 2 } else { 0 };
    let wrapped = (n - min).rem_euclid(modulus) + min;
    // The C backend can't write `i64::MIN`: `-9223372036854775808` is `-` applied to a
    // number too big for `i64`.
    i64::try_from(wrapped).ok().filter(|&n| n != i64::MIN)
}

// --- Locals ---

/// Replaces reads of locals that only ever hold one constant with that constant.
fn propagate(body: &mut Body) -> bool {
    let mut assigned: Vec<Vec<&Rvalue>> = vec![Vec::new(); body.locals.slots()];
    let mut pinned: HashSet<Local> = body.args().chain([RETURN]).collect();
    pinned.extend(body.outs.iter().copied());
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
            continue;
        };
        if dest.projection.is_empty() {
            assigned[dest.local.id].push(rvalue);
        } else {
            pinned.insert(dest.local);
        }
        if let Rvalue::Ref { place, .. } = rvalue {
            pinned.insert(place.local);
        }
    }
    let constants: Vec<Option<Const>> = assigned
        .iter()
        .enumerate()
        .map(|(i, rvalues)| match rvalues.as_slice() {
            [Rvalue::Use(Operand::Const(c))]
                if !pinned.contains(&Id::new(i))
                    && matches!(
                        c.kind,
                        ConstKind::Int(_) | ConstKind::Float(_) | ConstKind::Bool(_)
                    ) =>
            {
                Some(c.clone())
            }
            _ => None,
        })
        .collect();

    let mut changed = false;
    let mut replace = |op: &mut Operand| {
        if let Operand::Copy(place) | Operand::Move(place) = op
            && place.projection.is_empty()
            && let Some(c) = &constants[place.local.id]
        {
            *op = Operand::Const(c.clone());
            changed = true;
        }
    };
    for block in body.blocks.iter_mut() {
        for stmt in &mut block.stmts {
            if let StatementKind::Assign(_, rvalue) = &mut stmt.kind {
                operands_mut(rvalue).into_iter().for_each(&mut replace);
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &mut block.terminator.kind {
            replace(condition);
        }
    }
    changed
}

// --- Control flow ---

/// Turns branches on constants into jumps.
fn fold_branches(body: &mut Body) -> bool {
    let mut changed = false;
    for block in body.blocks.iter_mut() {
        if let TerminatorKind::Branch {
            condition:
                Operand::Const(Const {
                    kind: ConstKind::Bool(b),
                    ..
                }),
            then,
            otherwise,
        } = block.terminator.kind
        {
            block.terminator.kind = TerminatorKind::Goto(if b { then } else { otherwise });
            changed = true;
        }
    }
    changed
}

/// Removes the blocks control can't reach from the entry block, `bb0`.
fn remove_unreachable(body: &mut Body) -> bool {
    let entry = Id::new(0);
    let mut reachable = HashSet::from([entry]);
    let mut todo = vec![entry];
    while let Some(block) = todo.pop() {
        for next in body.block(block).terminator.kind.successors() {
            if reachable.insert(next) {
                todo.push(next);
            }
        }
    }
    let before = body.blocks.len();
    body.blocks.retain(|id, _| reachable.contains(&id));
    body.blocks.len() != before
}

// --- Operands ---

fn operands_mut(rvalue: &mut Rvalue) -> Vec<&mut Operand> {
    match rvalue {
        Rvalue::Use(op) | Rvalue::Not(op) => vec![op],
        Rvalue::Ref { .. } => Vec::new(),
        Rvalue::Binary { lhs, rhs, .. } => vec![lhs, rhs],
        Rvalue::Struct { fields: ops, .. } | Rvalue::Call { args: ops, .. } => {
            ops.iter_mut().collect()
        }
    }
}

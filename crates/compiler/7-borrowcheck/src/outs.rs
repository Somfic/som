//! `out` parameters are assigned before they're read, and before the function returns.
//!
//! A forward dataflow analysis tracks which `out` parameters are definitely assigned
//! at every point. None are on entry; assigning through one (`*p = …`, written
//! `remainder = …` in som) assigns it. Branches merge by intersection, so a parameter
//! only counts as assigned where every path to that point assigned it. Assigning a
//! single field doesn't assign the whole value.

use crate::BorrowError;
use som_base::Span;
use som_mir::*;
use std::collections::{BTreeSet, HashSet};

pub(crate) fn check(body: &Body, errors: &mut Vec<BorrowError>) {
    if body.outs.is_empty() {
        return;
    }
    let mut checker = Checker {
        body,
        report: false,
        errors: Vec::new(),
        reported: HashSet::new(),
    };
    let entry = dataflow::forward(
        body,
        BTreeSet::new(),
        |b, assigned| checker.block(b.id, assigned),
        |existing, assigned| {
            let before = existing.len();
            existing.retain(|l| assigned.contains(l));
            existing.len() != before
        },
    );

    // The states are final; walk every reachable block once more, reporting errors.
    checker.report = true;
    for (b, assigned) in entry.into_iter().enumerate() {
        if let Some(mut assigned) = assigned {
            checker.block(b, &mut assigned);
        }
    }
    errors.extend(checker.errors);
}

struct Checker<'a> {
    body: &'a Body,
    /// Whether to report errors. Off while iterating to a fixpoint, on for the final pass.
    report: bool,
    errors: Vec<BorrowError>,
    reported: HashSet<(Local, usize)>,
}

impl Checker<'_> {
    fn block(&mut self, b: usize, assigned: &mut BTreeSet<Local>) {
        let block = &self.body.blocks[Id::new(b)];
        for stmt in &block.stmts {
            let StatementKind::Assign(place, rvalue) = &stmt.kind else {
                continue;
            };
            match rvalue {
                Rvalue::Ref { place, .. } => self.read(assigned, place, &stmt.span),
                _ => (rvalue.operands().into_iter())
                    .for_each(|op| self.operand(assigned, op, &stmt.span)),
            }
            if self.is_out(place.local) && place.projection == [Projection::Deref] {
                assigned.insert(place.local);
            } else if place.projection.len() > 1 {
                // `remainder.x = 1` needs `remainder` to already hold a value.
                let whole = Place::local(place.local).project(Projection::Deref);
                self.read(assigned, &whole, &stmt.span);
            }
        }
        match &block.terminator.kind {
            TerminatorKind::Branch { condition, .. } => {
                self.operand(assigned, condition, &block.terminator.span)
            }
            TerminatorKind::Return => {
                for &out in &self.body.outs {
                    if !assigned.contains(&out) {
                        let decl = self.body.local(out);
                        self.error(
                            out,
                            0,
                            BorrowError::OutNotAssigned {
                                name: decl.name.clone().unwrap_or_default(),
                                span: decl.span.clone(),
                            },
                        );
                    }
                }
            }
            TerminatorKind::Goto(_) | TerminatorKind::Unreachable => {}
        }
    }

    fn operand(&mut self, assigned: &BTreeSet<Local>, op: &Operand, span: &Span) {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            self.read(assigned, place, span);
        }
    }

    fn read(&mut self, assigned: &BTreeSet<Local>, place: &Place, span: &Span) {
        let through = place.projection.first() == Some(&Projection::Deref);
        if self.is_out(place.local) && through && !assigned.contains(&place.local) {
            let name = self
                .body
                .local(place.local)
                .name
                .clone()
                .unwrap_or_default();
            self.error(
                place.local,
                span.start_offset + 1,
                BorrowError::OutReadBeforeAssigned {
                    name,
                    span: span.clone(),
                },
            );
        }
    }

    fn is_out(&self, local: Local) -> bool {
        self.body.outs.contains(&local)
    }

    /// Reports `error` once per parameter and `key`.
    fn error(&mut self, local: Local, key: usize, error: BorrowError) {
        if self.report && self.reported.insert((local, key)) {
            self.errors.push(error);
        }
    }
}

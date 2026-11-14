//! Functions that call themselves on every path: `fn f() -> i32 { f() }` can never return, and
//! runs until the stack runs out, or forever once the call becomes a jump.

use std::collections::HashSet;

use som_base::Span;
use som_mir::*;

use crate::BorrowError;

/// Reports function `id` if every path through it calls itself before it can return.
pub(crate) fn check(id: FuncId, func: &Function, body: &Body, errors: &mut Vec<BorrowError>) {
    let Some((entry, _)) = body.blocks.iter_with_ids().next() else {
        return;
    };
    // Every block reachable without calling itself first: if a `return` is one, it can return.
    let mut seen = HashSet::from([entry]);
    let mut work = vec![entry];
    let mut first_call: Option<Span> = None;
    while let Some(block) = work.pop() {
        let block = &body.blocks[block];
        let call = block.stmts.iter().find(|stmt| {
            matches!(&stmt.kind, StatementKind::Assign(_, Rvalue::Call { func, .. }) if *func == id)
        });
        if let Some(call) = call {
            first_call.get_or_insert_with(|| call.span.clone());
            continue;
        }
        match block.terminator.kind {
            TerminatorKind::Return => return,
            ref kind => {
                for next in kind.successors() {
                    if seen.insert(next) {
                        work.push(next);
                    }
                }
            }
        }
    }
    if let Some(span) = first_call {
        // `gen`, not the copy for `i32`, `gen<i32>`.
        let name = func.name.split('<').next().unwrap_or(&func.name).to_string();
        errors.push(BorrowError::EndlessRecursion { name, span });
    }
}

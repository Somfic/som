//! Calls the program makes when it runs that could be made while compiling: `fib(20)`, all
//! of whose arguments are known, of a function that does work (a loop, or recursion) and
//! nothing outside the program (no C but what the interpreter does itself). Each is tried:
//! one that works out within the step limit is suggested, `@fib(20)`, with its value.

use crate::State;
use som_base::Span;
use som_mir::*;
use std::collections::HashMap;

/// A call that could be worked out while compiling.
#[derive(Debug)]
pub struct CouldBeConst {
    /// The call as written: `fib(20)`.
    pub call: String,
    /// What it gives, where that's a number or a bool: `6765`.
    pub value: Option<String>,
    pub span: Span,
}

/// The C functions the interpreter does itself: calling them changes nothing outside.
const NATIVE: &[&str] = &["memcpy", "memmove", "memset", "memcmp", "strlen", "snprintf"];

/// Every call in `mir` that could be `@` one, each said once.
pub fn could_be_const(mir: &Mir) -> Vec<CouldBeConst> {
    let mut facts = Facts {
        mir,
        pure: HashMap::new(),
        works: HashMap::new(),
    };
    let mut found: Vec<CouldBeConst> = Vec::new();
    let mut tried: HashMap<String, bool> = HashMap::new();
    for (id, function) in mir.funcs.iter_with_ids() {
        let Some(body) = &function.body else {
            continue;
        };
        // Not the standard library's, what's already worked out while compiling, or an
        // instance of a generic function, whose arguments can be known in only some of them.
        if function.name.starts_with("som::")
            || function.name.starts_with("@const")
            || function.name.contains('<')
            || Some(id) == mir.fail
        {
            continue;
        }
        for block in body.blocks.iter() {
            for statement in &block.stmts {
                let StatementKind::Assign(_, Rvalue::Call { func, args }) = &statement.kind else {
                    continue;
                };
                let callee = &mir.funcs[*func];
                let known = args.iter().all(|a| {
                    matches!(a, Operand::Const(c) if !matches!(c.kind, ConstKind::Pending(_) | ConstKind::Func(_)))
                });
                if !known
                    || callee.body.is_none()
                    || matches!(mir.types.get(callee.ret), Ty::Unit)
                    || !facts.pure(*func)
                    || !facts.works(*func)
                    || found.iter().any(|f| f.span == statement.span)
                {
                    continue;
                }
                // Where the call is in its statement (`fib(TEN)` of `let a = fib(TEN);`).
                let short = callee.name.rsplit("::").next().unwrap_or(&callee.name);
                let Some(span) = call_span(&statement.span, short) else {
                    continue;
                };
                if found.iter().any(|f| f.span == span) {
                    continue;
                }
                // Worked out while compiling: what it gives, if it does within the limit.
                let Some(value) = State::try_call(mir, *func, args) else {
                    continue;
                };
                let call = span.get_text().to_string();
                if *tried.entry(call.clone()).or_insert(true) {
                    found.push(CouldBeConst {
                        value: match value {
                            ConstKind::Int(n) => Some(n.to_string()),
                            ConstKind::Float(x) => Some(x.to_string()),
                            ConstKind::Bool(b) => Some(b.to_string()),
                            _ => None,
                        },
                        call,
                        span,
                    });
                }
            }
        }
    }
    found
}

/// What's known of each function, worked out as needed.
struct Facts<'m> {
    mir: &'m Mir,
    /// Whether it does nothing outside the program: calls no C but what the interpreter does.
    pure: HashMap<FuncId, bool>,
    /// Whether it does work: a loop, or recursion, in it or what it calls.
    works: HashMap<FuncId, bool>,
}

impl Facts<'_> {
    fn pure(&mut self, func: FuncId) -> bool {
        if let Some(&known) = self.pure.get(&func) {
            return known;
        }
        let function = &self.mir.funcs[func];
        let Some(body) = &function.body else {
            let name = function.link_name.as_deref().unwrap_or(&function.name);
            return NATIVE.contains(&name);
        };
        // Recursion, while working it out: pure as far as it's gone.
        self.pure.insert(func, true);
        let mut pure = true;
        for callee in callees(body) {
            if !self.pure(callee) {
                pure = false;
                break;
            }
        }
        self.pure.insert(func, pure);
        pure
    }

    fn works(&mut self, func: FuncId) -> bool {
        if let Some(&known) = self.works.get(&func) {
            return known;
        }
        let Some(body) = &self.mir.funcs[func].body else {
            return false;
        };
        // Reaching it again, while working it out, is recursion: work.
        self.works.insert(func, true);
        // A jump back, to a block before it or itself: a loop.
        let looping = (body.blocks.iter_with_ids()).any(|(id, block)| {
            (block.terminator.kind.successors().iter()).any(|t| t.id <= id.id)
        });
        let works = looping || callees(body).into_iter().any(|callee| self.works(callee));
        self.works.insert(func, works);
        works
    }
}

/// The functions `body` calls.
fn callees(body: &Body) -> Vec<FuncId> {
    let mut found = Vec::new();
    for block in body.blocks.iter() {
        for statement in &block.stmts {
            match &statement.kind {
                StatementKind::Assign(_, Rvalue::Call { func, .. }) => found.push(*func),
                StatementKind::Drop { glue, .. } => found.push(*glue),
                _ => {}
            }
        }
    }
    found
}

/// Where the call of `name` is in `statement`: from the path naming it to its `)`. `None` if
/// it isn't written there once, by that name.
fn call_span(statement: &Span, name: &str) -> Option<Span> {
    let text = statement.get_text();
    let pattern = format!("{name}(");
    let mut matches = text.match_indices(&pattern).filter(|&(at, _)| {
        // A whole name, not the end of a longer one.
        text[..at].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
            || text[..at].ends_with("::")
    });
    let (at, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    // Back over the path in front of it: `math::fib(`.
    let mut start = at;
    while start > 0 {
        let before = &text[..start];
        if before.ends_with("::") {
            start -= 2;
        } else if before.chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            start -= before.chars().next_back().map_or(1, char::len_utf8);
        } else {
            break;
        }
    }
    // To the `)` that closes its arguments.
    let mut depth = 0usize;
    let mut end = None;
    for (i, c) in text[at + name.len()..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(at + name.len() + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let begin = statement.start_offset + start;
    Some(Span::from_range(begin..statement.start_offset + end?, statement.source.clone()))
}

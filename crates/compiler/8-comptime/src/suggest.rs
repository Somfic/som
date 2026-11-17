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
    /// What to write instead: `@fib(20)`, a local's value written in place of it (`@fib(n)`
    /// can't name `n`, a local).
    pub fix: String,
    pub span: Span,
}

/// The C functions the interpreter does itself: calling them changes nothing outside.
const NATIVE: &[&str] = &["memcpy", "memmove", "memset", "memcmp", "strlen", "snprintf"];

/// The most steps a call is tried for: more, and it's no call to make while compiling (and the
/// editor tries them on every check).
const TRY_LIMIT: u64 = 1_000_000;

/// The fewest steps a call takes for it to be worth working out while compiling:
/// `factorial(5)` is gone in a moment when the program runs.
const MIN_STEPS: u64 = 1_000;

/// Every call in `mir` that could be `@` one, each said once.
pub fn could_be_const(mir: &Mir) -> Vec<CouldBeConst> {
    let mut facts = Facts {
        mir,
        pure: HashMap::new(),
        works: HashMap::new(),
    };
    let mut found: Vec<CouldBeConst> = Vec::new();
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
        let known = known_locals(body);
        for block in body.blocks.iter() {
            for statement in &block.stmts {
                let StatementKind::Assign(_, Rvalue::Call { func, args }) = &statement.kind else {
                    continue;
                };
                let callee = &mir.funcs[*func];
                // Each argument a constant: written as one, or a local only ever set to one
                // (`let n = 1000; count_primes(n)`).
                let written_args = args;
                let Some(args) = (args.iter())
                    .map(|a| constant(a, &known).map(Operand::Const))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                if callee.body.is_none()
                    || matches!(mir.types.get(callee.ret), Ty::Unit)
                    || !facts.pure(*func)
                    || !facts.works(*func)
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
                // Worked out while compiling: what it gives, if it does within the limit, and
                // only if that saves enough work when the program runs.
                let Some((value, steps)) = State::try_call(mir, *func, &args, TRY_LIMIT) else {
                    continue;
                };
                if steps < MIN_STEPS {
                    continue;
                }
                // The call with each local argument's value written in its place.
                let call = span.get_text().to_string();
                let mut fixed = call.clone();
                let mut writable = true;
                for (written, arg) in written_args.iter().zip(&args) {
                    let (Operand::Copy(place) | Operand::Move(place)) = written else {
                        continue;
                    };
                    let (Some(name), Operand::Const(c)) = (&body.locals[place.local].name, arg) else {
                        continue;
                    };
                    match literal(&c.kind) {
                        Some(text) => fixed = replace_word(&fixed, name, &text),
                        None => writable = false,
                    }
                }
                if !writable {
                    continue;
                }
                // `@f(args)` for a call of a function by its name; `@const …` for anything else.
                let plain = (fixed.split('(').next()).is_some_and(|callee| {
                    !callee.is_empty()
                        && callee.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':')
                });
                let fix = match plain {
                    true => format!("@{fixed}"),
                    false => format!("@const {fixed}"),
                };
                found.push(CouldBeConst {
                    value: literal(&value),
                    call,
                    fix,
                    span,
                });
            }
        }
    }
    found
}

/// A number or bool, written as som reads it back: `3.0` for a float, not `3`.
fn literal(kind: &ConstKind) -> Option<String> {
    match kind {
        ConstKind::Int(n) => Some(n.to_string()),
        ConstKind::Float(x) => Some(format!("{x:?}")),
        ConstKind::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `text` with each whole word `word` in it replaced by `with`.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(word) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + word.len()..].chars().next();
        out.push_str(&rest[..at]);
        // Not part of a longer name, or a field: `.n`.
        if before.is_none_or(|c| !is_word(c) && c != '.') && after.is_none_or(|c| !is_word(c)) {
            out.push_str(with);
        } else {
            out.push_str(word);
        }
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    out
}

/// The constant `operand` is: written as one, or a local in `known`.
fn constant(operand: &Operand, known: &HashMap<Local, Const>) -> Option<Const> {
    match operand {
        Operand::Const(c) if !matches!(c.kind, ConstKind::Pending(_) | ConstKind::Func(_)) => {
            Some(c.clone())
        }
        Operand::Const(_) => None,
        Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
            known.get(&place.local).cloned()
        }
        Operand::Copy(_) | Operand::Move(_) => None,
    }
}

/// The locals of `body` that are always the same constant: set once, to a constant or another
/// such local, and never changed in part or borrowed. Not its parameters, which callers set.
fn known_locals(body: &Body) -> HashMap<Local, Const> {
    let mut sets: HashMap<Local, usize> = HashMap::new();
    let mut changed: std::collections::HashSet<Local> = std::collections::HashSet::new();
    let mut set_to: HashMap<Local, &Operand> = HashMap::new();
    for block in body.blocks.iter() {
        for statement in &block.stmts {
            let StatementKind::Assign(place, rvalue) = &statement.kind else {
                continue;
            };
            if let Rvalue::Ref { place: borrowed, .. } = rvalue {
                changed.insert(borrowed.local);
            }
            if !place.projection.is_empty() {
                changed.insert(place.local);
                continue;
            }
            *sets.entry(place.local).or_default() += 1;
            match rvalue {
                Rvalue::Use(operand) => {
                    set_to.insert(place.local, operand);
                }
                _ => {
                    changed.insert(place.local);
                }
            }
        }
    }
    let mut known: HashMap<Local, Const> = HashMap::new();
    // `let m = n;` of a known `n`: until nothing more is learned.
    loop {
        let mut learned = false;
        for (&local, &operand) in &set_to {
            if known.contains_key(&local)
                || local.id <= body.arg_count
                || sets.get(&local) != Some(&1)
                || changed.contains(&local)
            {
                continue;
            }
            if let Some(c) = constant(operand, &known) {
                known.insert(local, c);
                learned = true;
            }
        }
        if !learned {
            return known;
        }
    }
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
        let works = loops(body) || callees(body).into_iter().any(|callee| self.works(callee));
        self.works.insert(func, works);
        works
    }
}

/// Whether `body` has a loop: a jump back to a block on the way to it from the start. (Not
/// just to an earlier-numbered one: blocks are numbered as they're made, and an `if`'s can
/// jump to a lower one.)
fn loops(body: &Body) -> bool {
    fn visit(body: &Body, block: BlockId, state: &mut Vec<u8>) -> bool {
        // 0: not seen, 1: on the way here, 2: done.
        state[block.id] = 1;
        for next in body.blocks[block].terminator.kind.successors() {
            let seen = state[next.id];
            match seen {
                1 => return true,
                0 if visit(body, next, state) => return true,
                _ => {}
            }
        }
        state[block.id] = 2;
        false
    }
    let count = body.blocks.iter().count();
    count > 0 && visit(body, Id::new(0), &mut vec![0; count])
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

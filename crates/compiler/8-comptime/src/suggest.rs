//! Calls the program makes when it runs that could be made while compiling: `fib(20)`, all
//! of whose arguments are known, of a function that does work (a loop, or recursion) and
//! nothing outside the program (no C but what the interpreter does itself). Each is tried:
//! one that works out within the step limit is suggested, `@fib(20)`, with its value.

use crate::State;
use crate::limits::{MIN_STEPS, TRY_LIMIT};
use crate::natives::is_native;
use som_base::Span;
use som_lang::Builtin;
use som_mir::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct CouldBeConst {
    pub call: String,
    /// What it gives, where that's a number or a bool: `6765`.
    pub value: Option<String>,
    /// What to write instead: `@fib(20)`, a local's value written in place of it (`@fib(n)`
    /// can't name `n`, a local).
    pub fix: String,
    /// The locals it's given that are always the same, each with that value and where it's
    /// set: `n`, `100`, at `let n = 100;`.
    pub known: Vec<(String, String, Span)>,
    pub span: Span,
}

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
        if crate::is_std(&function.name)
            || function.name.starts_with(Builtin::Const.name())
            || function.name.contains('<')
            || Some(id) == mir.fail
        {
            continue;
        }
        let known = known_locals(body);
        for statement in body.blocks.iter().flat_map(|block| &block.stmts) {
            let StatementKind::Assign(_, Rvalue::Call { func, args }) = &statement.kind else {
                continue;
            };
            if let Some(suggestion) = facts.suggest(body, &known, statement, *func, args, &found) {
                found.push(suggestion);
            }
        }
    }
    found
}

impl Facts<'_> {
    /// The suggestion for `statement`, a call of `func` with `written_args`, if there is one.
    fn suggest(
        &mut self,
        body: &Body,
        known: &HashMap<Local, Const>,
        statement: &Statement,
        func: FuncId,
        written_args: &[Operand],
        found: &[CouldBeConst],
    ) -> Option<CouldBeConst> {
        let mir = self.mir;
        let callee = &mir.funcs[func];
        // Each argument a constant: written as one, or a local only ever set to one
        // (`let n = 1000; count_primes(n)`).
        let args = (written_args.iter())
            .map(|a| constant(a, known).map(Operand::Const))
            .collect::<Option<Vec<_>>>()?;
        if callee.body.is_none()
            || matches!(mir.types.get(callee.ret), Ty::Unit)
            || !self.pure(func)
            || !self.works(func)
        {
            return None;
        }
        // Where the call is in its statement (`fib(TEN)` of `let a = fib(TEN);`).
        let short = callee.name.rsplit("::").next().unwrap_or(&callee.name);
        let span = call_span(&statement.span, short)?;
        if found.iter().any(|f| f.span == span) {
            return None;
        }
        // Only if that saves enough work when the program runs.
        let (value, steps) = State::try_call(mir, func, &args, TRY_LIMIT)?;
        if steps < MIN_STEPS {
            return None;
        }
        let call = span.get_text().to_string();
        let (fixed, known_here) = with_values_written(&call, body, written_args, &args)?;
        // `@f(args)` for a call of a function by its name; `@const …` for anything else.
        let plain = (fixed.split('(').next()).is_some_and(|callee| {
            !callee.is_empty()
                && callee
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        });
        let fix = match plain {
            true => format!("@{fixed}"),
            false => format!("{} {fixed}", Builtin::Const.name()),
        };
        Some(CouldBeConst {
            value: literal(&value),
            call,
            fix,
            known: known_here,
            span,
        })
    }
}

/// `call` with each local argument's value written in its place, and those locals: each
/// with its value and where it's set. `None` if a value can't be written.
fn with_values_written(
    call: &str,
    body: &Body,
    written_args: &[Operand],
    args: &[Operand],
) -> Option<(String, Vec<(String, String, Span)>)> {
    let mut fixed = call.to_string();
    let mut writable = true;
    let mut known_here = Vec::new();
    for (written, arg) in written_args.iter().zip(args) {
        let Some(place) = written.place() else {
            continue;
        };
        let (Some(name), Operand::Const(c)) = (&body.locals[place.local].name, arg) else {
            continue;
        };
        match literal(&c.kind) {
            Some(text) => {
                fixed = replace_word(&fixed, name, &text);
                let set_at = body.locals[place.local].span.clone();
                if !known_here.iter().any(|(n, _, _)| n == name) {
                    known_here.push((name.clone(), text, set_at));
                }
            }
            None => writable = false,
        }
    }
    writable.then_some((fixed, known_here))
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
            if let Rvalue::Ref {
                place: borrowed, ..
            } = rvalue
            {
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
            return is_native(name);
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
    #[derive(Clone, Copy, PartialEq)]
    enum Visit {
        NotSeen,
        OnTheWay,
        Done,
    }
    fn visit(body: &Body, block: BlockId, state: &mut Vec<Visit>) -> bool {
        state[block.id] = Visit::OnTheWay;
        for next in body.blocks[block].terminator.kind.successors() {
            let seen = state[next.id];
            match seen {
                Visit::OnTheWay => return true,
                Visit::NotSeen if visit(body, next, state) => return true,
                _ => {}
            }
        }
        state[block.id] = Visit::Done;
        false
    }
    let count = body.blocks.iter().count();
    count > 0 && visit(body, Id::new(0), &mut vec![Visit::NotSeen; count])
}

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
        text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
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
        } else if before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
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
    Some(Span::from_range(
        begin..statement.start_offset + end?,
        statement.source.clone(),
    ))
}

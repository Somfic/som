//! Checks what has to be known where it's needed, once every type is: that an index is
//! inside what it indexes, and that each `@where` of a function holds where it's called.
//! Nothing is checked as the program runs, so what can't be known here isn't allowed.
//!
//! Each function body is gone through in the order it runs, learning what holds as it
//! goes (see [`facts`]): an `if`'s condition inside it, and its opposite after it when it
//! returns; a `while`'s condition; a `for` loop's range; a `let`'s value; the function's
//! own `@where`s. What's learned about a place is forgotten when it may change: assigned,
//! lent as `&mut`, or given to a method taking `&mut self`.

pub mod error;
pub mod facts;

use som_ast::{Ast, BinOp, Expr, Func, Id, Stmt};
use som_resolve::{DefId, DefKind, Names};
use som_typeck::Typed;
use som_typeck::{IntTy, Ty, TypeId};

pub use error::ProveError;
use facts::{Compare, Cond, Facts, Term, Value};
use std::collections::HashMap;
use som_base::message::code;
use som_base::{Fix, Message, Span, msg};

/// Checks every function in `ast`.
pub fn prove(ast: &Ast, names: &Names, typed: &Typed) -> Vec<ProveError> {
    let mut prover = Prover {
        ast,
        names,
        typed,
        facts: Facts::default(),
        errors: Vec::new(),
        within: Vec::new(),
        bound: HashMap::new(),
    };
    for (id, func) in ast.funcs.iter() {
        // A trait's method has the trait's conditions, all a caller through the trait knows.
        if let (Some(clause), Some(declared)) =
            (func.requires.first(), prover.trait_declaration(id))
        {
            let trait_ = (ast.traits.iter())
                .find(|(_, t)| t.methods.contains(&declared))
                .map(|(_, t)| t.name.name.to_string())
                .unwrap_or_default();
            prover.errors.push(ProveError::WhereOnImpl {
                method: func.name.name.to_string(),
                trait_,
                span: ast.exprs.span(clause.condition).clone(),
            });
        }
        // A `@where` that can't be followed could never be known to hold.
        for clause in &func.requires {
            if prover.cond(clause.condition, true, &as_is).has_unknown() {
                prover.errors.push(ProveError::UnfollowedWhere {
                    span: ast.exprs.span(clause.condition).clone(),
                });
            }
        }
    }
    for (id, func) in ast.funcs.iter() {
        // A member only some instances have is proven for each: what's in it depends on them.
        if typed.is_per_instance(id) && !func.when.is_empty() {
            continue;
        }
        if let Some(body) = func.body {
            prover.facts = Facts::default();
            prover.assume_requires(id);
            prover.expr(body);
        }
    }
    // A constant's value, worked out while compiling: its calls' `@where`s too, `at(5)` of
    // `@where i < 3` an error here rather than whatever running it gives.
    for (_, constant) in ast.consts.iter() {
        if typed.is_checked_expr(constant.value) {
            prover.facts = Facts::default();
            prover.expr(constant.value);
        }
    }
    prover.errors
}

/// Proves one instance of function `func`, checked for that instance (`typed`), its generic
/// value parameters standing for `values` (`N` for 3): its body, where `@if N >= 3` is
/// known. Only what's wrong inside `regions` (its `@if`s on them) is said: the rest of it was
/// proven once, for every instance.
pub fn prove_instance(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    func: Id<som_ast::Func>,
    values: &[(DefId, i128)],
    regions: &[Span],
) -> Vec<ProveError> {
    let mut prover = Prover {
        ast,
        names,
        typed,
        facts: Facts::default(),
        errors: Vec::new(),
        within: Vec::new(),
        bound: values.iter().copied().collect(),
    };
    if let Some(body) = ast.funcs[func].body {
        prover.assume_requires(func);
        prover.expr(body);
    }
    let inside = |error: &ProveError| {
        let diagnostic = error.to_diagnostic();
        let Some(label) = diagnostic.labels.iter().find(|l| l.is_primary) else {
            return false;
        };
        let at = &label.span;
        regions.iter().any(|region| {
            region.source.identifier() == at.source.identifier()
                && region.start_offset <= at.start_offset
                && at.start_offset < region.start_offset + region.length
        })
    };
    prover.errors.into_iter().filter(|e| inside(e)).collect()
}

/// What facts are about: a variable or a field or property of one, `xs.len`; or an
/// array's length that isn't written out, `N` of `[T; N]`.
#[derive(Clone, PartialEq, Debug)]
pub enum Place {
    Path { root: DefId, path: Vec<Box<str>> },
    Len(DefId),
}

impl Place {
    /// Whether this is `other`, or inside it: `xs.len` is inside `xs`.
    fn within(&self, other: &Place) -> bool {
        match (self, other) {
            (
                Place::Path { root, path },
                Place::Path {
                    root: other_root,
                    path: other_path,
                },
            ) => root == other_root && path.starts_with(other_path),
            _ => false,
        }
    }
}

/// How a condition's parameters are read: at a call, as the arguments given for them.
type Subst<'s> = &'s dyn Fn(DefId) -> Option<Value<Place>>;

fn as_is(_: DefId) -> Option<Value<Place>> {
    None
}

struct Prover<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    facts: Facts<Place>,
    errors: Vec<ProveError>,
    /// The statements being gone through, innermost last: where a fix puts a check.
    within: Vec<Within>,
    /// Proving one instance: the numbers its generic value parameters stand for.
    bound: HashMap<DefId, i128>,
}

/// A statement being gone through, with the rest of its block after it, and whether the
/// block ends in a value.
struct Within {
    stmt: Id<Stmt>,
    rest: Vec<Id<Stmt>>,
    tail: bool,
}

/// A `@where` not known to hold at a call: the condition, how it reads there, and what
/// it means.
struct Unmet {
    condition: Id<Expr>,
    text: String,
    reason: Option<String>,
    /// It's known not to hold.
    never: bool,
}

impl Prover<'_> {
    // --- Going through bodies ---

    /// Statements, in the scope of whoever goes through them: what an early `return` in
    /// them teaches lasts as long as that scope.
    fn stmts(&mut self, stmts: &[Id<Stmt>], tail: bool) {
        for (i, &stmt) in stmts.iter().enumerate() {
            self.within.push(Within {
                stmt,
                rest: stmts[i + 1..].to_vec(),
                tail,
            });
            self.stmt(stmt);
            self.within.pop();
            // `if i >= xs.len { return; }`: the rest only runs where it's false.
            if let Stmt::If {
                condition,
                then,
                otherwise,
            } = &self.ast.stmts[stmt]
            {
                let then_leaves = self.leaves(then);
                let otherwise_leaves = otherwise.as_ref().is_some_and(|o| self.leaves(o));
                if then_leaves != otherwise_leaves {
                    let cond = self.cond(*condition, otherwise_leaves, &as_is);
                    self.facts.assume(&cond);
                }
            }
        }
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            // A constant's value isn't proven here, wherever it's written.
            Stmt::Use(_) | Stmt::Const(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Let { value, .. } => {
                self.expr(*value);
                // `let last = xs.len - 1`: `last` is that, until it changes.
                if let Some(def) = self.names.local(id)
                    && self.is_integer(*value)
                    && let Some((value, needs)) = self.value_of(*value, &as_is)
                {
                    let local = Value::place(Place::Path {
                        root: def,
                        path: Vec::new(),
                    });
                    if self.is_unsigned_ty(self.typed.local(def)) {
                        self.facts.unsigned(local.term_place().clone());
                    }
                    if needs.iter().all(|n| self.facts.holds(n)) {
                        let equal = Cond::compare(&local, Compare::Equal, &value);
                        self.facts.assume(&equal);
                    }
                }
                // `let p = Point { x: 1, y: 2 }`: `p.x` is 1, until `p` changes.
                if let Some(def) = self.names.local(id) {
                    self.learn_fields(def, Vec::new(), *value);
                    // `let s: &[i32] = [1, 2, 3]`: `s.len` is 3.
                    if let Expr::Array(values) = &self.ast.exprs[*value]
                        && matches!(self.typed.ty(self.typed.local(def)), Ty::Ref { to, .. }
                            if matches!(self.typed.ty(*to), Ty::Slice(_)))
                    {
                        let len = Place::Path {
                            root: def,
                            path: vec!["len".into()],
                        };
                        self.facts.unsigned(len.clone());
                        let count = Value::number(values.len() as i128);
                        self.facts
                            .assume(&Cond::compare(&Value::place(len), Compare::Equal, &count));
                    }
                }
            }
            // `@for f in T` over a generic type, proving one instance: its copy.
            Stmt::Expr(_) if let Some(arm) = self.typed.chosen_arm(id) => self.expr(arm),
            Stmt::Expr(expr) => self.expr(*expr),
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.expr(*value);
                }
            }
            // An `@if` checked for each instance of its function: proven there, not here.
            Stmt::If { .. } if self.typed.is_region(id) => {}
            // `@if`: only the branch it picks is there, and nothing's checked of the condition
            // as the program runs.
            Stmt::If {
                then, otherwise, ..
            } if let Some(holds) = self.typed.settled_if(id) => {
                let branch = if holds { Some(then) } else { otherwise.as_ref() };
                if let Some(branch) = branch {
                    self.stmts(branch, false);
                }
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                self.expr(*condition);
                self.branch(*condition, true, |p| p.stmts(then, false));
                if let Some(otherwise) = otherwise {
                    self.branch(*condition, false, |p| p.stmts(otherwise, false));
                }
            }
            Stmt::Loop { body } => {
                self.forget_changed_in(body);
                let mark = self.facts.mark();
                self.stmts(body, false);
                self.facts.forget_since(mark);
            }
            Stmt::While { condition, body } => {
                self.forget_changed_in(body);
                self.expr(*condition);
                self.branch(*condition, true, |p| p.stmts(body, false));
                // `while i < n { … }` ends with `i >= n`, unless a `break` leaves it sooner.
                if !self.breaks(body) {
                    let cond = self.cond(*condition, false, &as_is);
                    self.facts.assume(&cond);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.forget_changed_in(body);
                self.expr(*iterable);
                let mark = self.facts.mark();
                self.assume_range(id, *iterable);
                self.stmts(body, false);
                self.facts.forget_since(mark);
            }
        }
    }

    /// Runs `f` where `condition` is `truth`.
    fn branch(&mut self, condition: Id<Expr>, truth: bool, f: impl FnOnce(&mut Self)) {
        let mark = self.facts.mark();
        let cond = self.cond(condition, truth, &as_is);
        self.facts.assume(&cond);
        f(self);
        self.facts.forget_since(mark);
    }

    /// What the integer fields of a struct written out, `value`, put at `path` in local
    /// `root` are: `p.x` is 1 for `let p = Point { x: 1, y: 2 }`. Fields of fields too.
    fn learn_fields(&mut self, root: DefId, path: Vec<Box<str>>, value: Id<Expr>) {
        let Expr::Construct { fields, .. } = &self.ast.exprs[value] else {
            return;
        };
        for (name, field) in fields {
            let mut inner = path.clone();
            inner.push(name.name.clone());
            if self.is_integer(*field)
                && let Some((known, needs)) = self.value_of(*field, &as_is)
                && needs.iter().all(|n| self.facts.holds(n))
            {
                let place = Value::place(Place::Path {
                    root,
                    path: inner.clone(),
                });
                if self.is_unsigned(*field) {
                    self.facts.unsigned(place.term_place().clone());
                }
                self.facts
                    .assume(&Cond::compare(&place, Compare::Equal, &known));
            }
            self.learn_fields(root, inner, *field);
        }
    }

    /// `for i in a..b`: `a <= i` and `i < b`, each time round. Not of a `mut` variable,
    /// which the body can change.
    fn assume_range(&mut self, stmt: Id<Stmt>, iterable: Id<Expr>) {
        let Stmt::For { mutable: false, .. } = self.ast.stmts[stmt] else {
            return;
        };
        let Expr::Range { start, end } = self.ast.exprs[iterable] else {
            return;
        };
        let Some(def) = self.names.local(stmt) else {
            return;
        };
        let var = Value::place(Place::Path {
            root: def,
            path: Vec::new(),
        });
        if self.is_unsigned_ty(self.typed.local(def)) {
            self.facts.unsigned(var.term_place().clone());
        }
        let start = match start {
            Some(start) => self.value_of(start, &as_is),
            None => Some((Value::number(0), Vec::new())),
        };
        if let Some((start, needs)) = start
            && needs.iter().all(|n| self.facts.holds(n))
        {
            self.facts
                .assume(&Cond::compare(&start, Compare::LessOrEqual, &var));
        }
        if let Some(end) = end
            && let Some((end, needs)) = self.value_of(end, &as_is)
            && needs.iter().all(|n| self.facts.holds(n))
        {
            self.facts.assume(&Cond::compare(&var, Compare::Less, &end));
        }
    }

    fn expr(&mut self, id: Id<Expr>) {
        match &self.ast.exprs[id] {
            Expr::Fail(message) => self.expr(*message),
            // A constant's value isn't proven here, wherever it's written.
            Expr::InPlace(_)
            | Expr::Hole
            | Expr::Type(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Var(_)
            | Expr::Unreachable
            | Expr::Out(_) => {}
            Expr::Array(values) => values.iter().for_each(|&v| self.expr(v)),
            Expr::Interpolated(parts) => {
                for &part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.expr(value);
                    }
                }
            }
            Expr::Cast { value, .. } => self.expr(*value),
            Expr::Range { start, end } => {
                start.iter().chain(end).for_each(|&e| self.expr(e));
            }
            Expr::ArrayRepeat { value, .. } => self.expr(*value),
            Expr::Binary { op, lhs, rhs } => {
                self.expr(*lhs);
                // `i < xs.len and xs[i] == 0`: the right side only runs where the left is
                // true, or for `||`, false.
                match op {
                    BinOp::And => self.branch(*lhs, true, |p| p.expr(*rhs)),
                    BinOp::Or => self.branch(*lhs, false, |p| p.expr(*rhs)),
                    _ => self.expr(*rhs),
                }
                // `a / b` and `a % b` of integers: `b` has to be known not to be 0.
                if matches!(op, BinOp::Divide | BinOp::Modulo) && self.is_integer(*lhs) {
                    self.check_divisor(*rhs);
                }
            }
            Expr::Not(inner) | Expr::Neg(inner) | Expr::BitNot(inner) | Expr::Deref(inner) => {
                self.expr(*inner)
            }
            Expr::Borrow { mutable, expr } => {
                self.expr(*expr);
                if *mutable {
                    self.changed(*expr);
                }
            }
            Expr::Assign { target, value } => {
                self.expr(*value);
                self.target(*target);
                self.changed(*target);
                // `i = n`: `i` is `n` from here, if `n` isn't `i` itself.
                if let Some(def) = self.local_var(*target)
                    && self.is_integer(*value)
                    && !self.mentions(*value, def)
                    && let Some((value, needs)) = self.value_of(*value, &as_is)
                    && needs.iter().all(|n| self.facts.holds(n))
                {
                    let var = Value::place(Place::Path {
                        root: def,
                        path: Vec::new(),
                    });
                    self.facts
                        .assume(&Cond::compare(&var, Compare::Equal, &value));
                }
            }
            Expr::Call { args, .. } => {
                args.iter().for_each(|&a| self.expr(a));
                if let Some(func) = self.callee(id) {
                    self.check_call(id, func, 0, None, args);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.expr(*receiver);
                args.iter().for_each(|&a| self.expr(a));
                if let Some(func) = self.method(id) {
                    self.check_call(id, func, 1, Some(*receiver), args);
                    if self.takes_mut_self(func) {
                        self.changed(*receiver);
                    }
                }
            }
            Expr::Field { object, .. } => {
                self.expr(*object);
                // A property's `@where`.
                if let Some(func) = self.method(id) {
                    self.check_call(id, func, 1, Some(*object), &[]);
                }
            }
            Expr::Index { object, index } => {
                self.expr(*object);
                self.expr(*index);
                self.check_index(id, *object, *index, true);
            }
            Expr::Construct { fields, .. } => fields.iter().for_each(|&(_, v)| self.expr(v)),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.expr(*condition);
                self.branch(*condition, true, |p| p.expr(*then));
                self.branch(*condition, false, |p| p.expr(*otherwise));
            }
            Expr::Fallback { optional, fallback } => {
                self.expr(*optional);
                self.expr(*fallback);
            }
            Expr::Match { .. } if let Some(copy) = self.typed.chosen_match(id) => self.expr(copy),
            Expr::Match { scrutinee, arms } => {
                self.expr(*scrutinee);
                for arm in arms {
                    let mark = self.facts.mark();
                    self.expr(arm.body);
                    self.facts.forget_since(mark);
                }
            }
            Expr::Block { stmts, value } => {
                let mark = self.facts.mark();
                self.stmts(stmts, value.is_some());
                if let Some(value) = value {
                    self.expr(*value);
                }
                self.facts.forget_since(mark);
            }
        }
    }

    /// An assignment's target: what's evaluated to find where it goes, `i` in `xs[i] = v`.
    fn target(&mut self, target: Id<Expr>) {
        match &self.ast.exprs[target] {
            Expr::Var(_) => {}
            Expr::Field { object, .. } | Expr::Deref(object) => self.target(*object),
            Expr::Index { object, index } => {
                self.target(*object);
                self.expr(*index);
                self.check_index(target, *object, *index, false);
            }
            _ => self.expr(target),
        }
    }

    // --- Indexes and calls ---

    /// `object[index]`: an array's, inside its length; any other's, its `at`'s `@where`.
    /// `reading`: not assigned to, so `get` would do instead.
    fn check_index(&mut self, id: Id<Expr>, object: Id<Expr>, index: Id<Expr>, reading: bool) {
        if let Some(func) = self.method(id) {
            if !self.unproven(id, func, 1, Some(object), &[index]).is_empty() {

                self.out_of_bounds(id, object, index, reading);
            }
            return;
        }
        let Some(len) = self.array_len(object) else {
            return;
        };
        // A number written out is checked against a length written out by the type checker;
        // against `N`, here.
        if matches!(self.ast.exprs[index], Expr::Int(_)) && len.is_number() {
            return;
        }
        // Inside is from 0 up to the length: a signed index can be below 0, and `as usize`
        // would take it far past the end.
        let at = self.value_of(index, &as_is);
        let known = at.is_some_and(|(at, needs)| {
            needs.iter().all(|n| self.facts.holds(n))
                && self.facts.holds(&Cond::compare(&at, Compare::GreaterOrEqual, &Value::number(0)))
                && self.facts.holds(&Cond::compare(&at, Compare::Less, &len))
        });
        // `y * 10 + x`, of an array of a known length: where it's known to be from what's
        // known of `x` and `y`.
        let known = known
            || (len.is_number()
                && self
                    .range_of(index)
                    .is_some_and(|(least, most)| least >= 0 && most < len.offset));
        if !known {
            self.out_of_bounds(id, object, index, reading);
        }
    }

    /// `object[index]` not known to be inside it.
    fn out_of_bounds(&mut self, id: Id<Expr>, object: Id<Expr>, index: Id<Expr>, get: bool) {
        let object_text = self.ast.exprs.span(object).get_text();
        let index_text = self.ast.exprs.span(index).get_text();
        let len_text = format!("{object_text}.len");
        let known = self.known_about(object, index, &index_text, &len_text);
        // Known to be outside, not just not known to be inside.
        let never = self.index_outside(object, index);
        let written = matches!(self.ast.exprs[index], Expr::Int(_));
        // A signed index known not to be below 0 only needs the length checked.
        let signed = !written && !self.is_unsigned(index) && !self.at_least_zero(index);
        let check = match (written, signed) {
            (true, _) => format!("{len_text} > {index_text}"),
            (false, true) => format!("{index_text} >= 0 && {index_text} < {len_text}"),
            (false, false) => format!("{index_text} < {len_text}"),
        };
        let title = match written {
            true => msg![
                "check the length first: ",
                code(format!("if {check} {{ … }}"))
            ],
            false => msg!["check it first: ", code(format!("if {check} {{ … }}"))],
        };
        let fix = self.check_fix(&check, title);
        let get_fix = get.then(|| {
            let get = format!("{object_text}.get({index_text})");
            Fix::replace(
                msg!["use ", code(get.clone()), ", which gives an optional"],
                self.ast.exprs.span(id).clone(),
                get,
            )
        });
        self.errors.push(ProveError::MaybeOutOfBounds {
            never,
            index: index_text.to_string(),
            check,
            written,
            signed,
            len: len_text,
            known,
            span: self.ast.exprs.span(index).clone(),
            fix,
            get_fix,
        });
    }

    /// A fix putting the statement being gone through inside `if check { … }`: with the
    /// rest of its block, for a `let`, whose variable that uses. Not for a `return`, after
    /// which the function would need another, nor a `let` in a block ending in a value.
    fn check_fix(&self, check: &str, title: Message) -> Option<Fix> {
        let within = self.within.last()?;
        let last = match self.ast.stmts[within.stmt] {
            Stmt::Expr(_) => within.stmt,
            Stmt::Let { .. } if !within.tail => *within.rest.last().unwrap_or(&within.stmt),
            _ => return None,
        };
        let first = self.ast.stmts.span(within.stmt);
        let end = self.ast.stmts.span(last);
        let mut end_offset = end.start_offset + end.length;
        // With the last one's `;`, which goes inside.
        if first.source.content()[end_offset..].starts_with(';') {
            end_offset += 1;
        }
        let span = Span::from_range(first.start_offset..end_offset, first.source.clone());
        let indent = " ".repeat(span.start.col.saturating_sub(1));
        let text = span.get_text();
        let body: Vec<String> = text
            .lines()
            .enumerate()
            .map(|(i, line)| match i {
                0 => format!("{indent}    {line}"),
                _ if line.is_empty() => String::new(),
                _ => format!("    {line}"),
            })
            .collect();
        let wrapped = format!("if {check} {{\n{}\n{indent}}}", body.join("\n"));
        Some(Fix::replace(title, span, wrapped))
    }

    /// Whether index `index` of `object` is known to be at or past its end.
    fn index_outside(&mut self, object: Id<Expr>, index: Id<Expr>) -> bool {
        let len = match self.array_len(object) {
            Some(len) => len,
            None => match self.value_of(object, &as_is) {
                Some((
                    Value {
                        term: Term::Place(Place::Path { root, mut path }),
                        minus: Term::Zero,
                        offset: 0,
                    },
                    _,
                )) => {
                    path.push("len".into());
                    Value::place(Place::Path { root, path })
                }
                _ => return false,
            },
        };
        let Some((at, _)) = self.value_of(index, &as_is) else {
            return false;
        };
        self.facts
            .refuted(&Cond::compare(&at, Compare::Less, &len))
    }

    /// What is known about index `index` of `object`, for when it isn't enough: how it
    /// compares with the length, or what each is known to be.
    fn known_about(
        &mut self,
        object: Id<Expr>,
        index: Id<Expr>,
        index_text: &str,
        len_text: &str,
    ) -> Message {
        let len = match self.array_len(object) {
            Some(len) => Some(len),
            None => self
                .value_of(object, &as_is)
                .and_then(|(base, _)| match base {
                    Value {
                        term: Term::Place(Place::Path { root, mut path }),
                        minus: Term::Zero,
                        offset: 0,
                    } => {
                        path.push("len".into());
                        Some(Value::place(Place::Path { root, path }))
                    }
                    _ => None,
                }),
        };
        let at = self.value_of(index, &as_is).map(|(at, _)| at);
        let range = |p: &Self, v: &Value<Place>| {
            // Unsigned values are at least 0 anyway: that's no news.
            let least = (p.facts.bound(&v.minus, &v.term))
                .map(|d| v.offset - d)
                .filter(|&n| n > 0);
            let most = p.facts.bound(&v.term, &v.minus).map(|d| d + v.offset);
            (least, most)
        };
        let describe =
            |subject: &str, (least, most): (Option<i128>, Option<i128>)| match (least, most) {
                (Some(l), Some(m)) if l == m => {
                    msg![code(subject.to_string()), " is ", l.to_string()]
                }
                (Some(l), Some(m)) => msg![
                    code(subject.to_string()),
                    " is only known to be from ",
                    code(l.to_string()),
                    " to ",
                    code(m.to_string())
                ],
                (Some(l), None) => msg![
                    code(subject.to_string()),
                    " is only known to be at least ",
                    code(l.to_string())
                ],
                (None, Some(m)) => msg![
                    code(subject.to_string()),
                    " is only known to be at most ",
                    code(m.to_string())
                ],
                (None, None) => msg![
                    "nothing is known about ",
                    code(subject.to_string()),
                    " here"
                ],
            };
        let Some(len) = len else {
            return msg![
                "nothing is known about ",
                code(len_text.to_string()),
                " here"
            ];
        };
        let len_known = describe(len_text, range(self, &len));
        let Some(at) = at else {
            return len_known;
        };
        if at.is_number() {
            return len_known;
        }
        // `i <= xs.len`: how the index compares with the length, when it's no number.
        let at_range = range(self, &at);
        if at_range.1.is_none()
            && at.minus == Term::Zero
            && let Some(d) = self.facts.bound(&at.term, &len.term)
        {
            let above = d + at.offset - len.offset;
            let limit = match above {
                0 => len_text.to_string(),
                n if n > 0 => format!("{len_text} + {n}"),
                n => format!("{len_text} - {}", -n),
            };
            return msg![
                code(index_text.to_string()),
                " is only known to be at most ",
                code(limit)
            ];
        }
        let at_known = describe(index_text, at_range);
        msg![at_known, ", and ", len_known]
    }

    /// The divisor of an integer `/` or `%`: known not to be 0, as dividing by it has no
    /// answer and nothing is checked as the program runs.
    fn check_divisor(&mut self, divisor: Id<Expr>) {
        if self.nonzero(divisor) {
            return;
        }
        let never = match self.value_of(divisor, &as_is) {
            Some((value, needs)) => self.facts.refuted(&Cond::All(
                needs
                    .into_iter()
                    .chain([Cond::compare(&value, Compare::NotEqual, &Value::number(0))])
                    .collect(),
            )),
            None => false,
        };
        let text = self.ast.exprs.span(divisor).get_text().to_string();
        let simple = text.chars().all(|c| c.is_alphanumeric() || "_.".contains(c));
        let check = match simple {
            true => format!("{text} != 0"),
            false => format!("({text}) != 0"),
        };
        // `12 / 0`: no check would do.
        let written = matches!(self.ast.exprs[divisor], Expr::Int(_));
        let title = msg!["check it first: ", code(format!("if {check} {{ … }}"))];
        let fix = (!written).then(|| self.check_fix(&check, title)).flatten();
        self.errors.push(ProveError::MaybeZeroDivisor {
            written,
            divisor: text,
            check,
            never,
            span: self.ast.exprs.span(divisor).clone(),
            fix,
        });
    }

    /// Whether `expr` is known not to be 0. Of `a if c else b`, each where it's chosen: `a`
    /// where `c` holds, `b` where it doesn't.
    fn nonzero(&mut self, expr: Id<Expr>) -> bool {
        if let Expr::Conditional {
            condition,
            then,
            otherwise,
        } = self.ast.exprs[expr]
        {
            let mut both = true;
            for (branch, truth) in [(then, true), (otherwise, false)] {
                let mark = self.facts.mark();
                let cond = self.cond(condition, truth, &as_is);
                self.facts.assume(&cond);
                both &= self.nonzero(branch);
                self.facts.forget_since(mark);
            }
            return both;
        }
        // `(n % 4) + 1`: from what's known of what it's made of.
        if self
            .range_of(expr)
            .is_some_and(|(least, most)| least > 0 || most < 0)
        {
            return true;
        }
        let Some((value, needs)) = self.value_of(expr, &as_is) else {
            return false;
        };
        let cond = Cond::All(
            needs
                .into_iter()
                .chain([Cond::compare(&value, Compare::NotEqual, &Value::number(0))])
                .collect(),
        );
        self.facts.holds(&cond)
    }

    /// Whether `expr` is known not to be below 0.
    fn at_least_zero(&mut self, expr: Id<Expr>) -> bool {
        if self.range_of(expr).is_some_and(|(least, _)| least >= 0) {
            return true;
        }
        self.value_of(expr, &as_is).is_some_and(|(value, needs)| {
            needs.iter().all(|n| self.facts.holds(n))
                && self.facts.holds(&Cond::compare(
                    &value,
                    Compare::GreaterOrEqual,
                    &Value::number(0),
                ))
        })
    }

    /// The least and most integer `expr` can be: from what's known of each place in it
    /// (or as far as its type goes), through `+`, `-`, `*`, `/`, `%` and negation. What's compared
    /// with each other isn't followed here (that's [`Facts`]), only how far each can go:
    /// `y * 10 + x` of `x` from 0 to 9 and `y` from 0 to 19 is from 0 to 199. `None` where
    /// a step could wrap around, as its type has no room for it.
    fn range_of(&mut self, expr: Id<Expr>) -> Option<(i128, i128)> {
        if !self.is_integer(expr) {
            return None;
        }
        let (least, most) = match self.ast.exprs[expr] {
            Expr::Neg(inner) => {
                let (least, most) = self.range_of(inner)?;
                (most.checked_neg()?, least.checked_neg()?)
            }
            Expr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
                ) =>
            {
                let (a, b) = (self.range_of(lhs)?, self.range_of(rhs)?);
                // Of `*` and `/`, the extremes are among the ends' combinations.
                let corners = |f: fn(i128, i128) -> Option<i128>| {
                    let all = [f(a.0, b.0)?, f(a.0, b.1)?, f(a.1, b.0)?, f(a.1, b.1)?];
                    Some((*all.iter().min()?, *all.iter().max()?))
                };
                match op {
                    BinOp::Add => (a.0.checked_add(b.0)?, a.1.checked_add(b.1)?),
                    BinOp::Subtract => (a.0.checked_sub(b.1)?, a.1.checked_sub(b.0)?),
                    BinOp::Multiply => corners(i128::checked_mul)?,
                    // Rounding towards 0, as C does; never by something that can be 0.
                    BinOp::Divide if b.0 > 0 || b.1 < 0 => corners(i128::checked_div)?,
                    // What's left over is below the divisor, and of something not below 0,
                    // not below 0 either.
                    BinOp::Modulo if a.0 >= 0 && b.0 > 0 => (0, a.1.min(b.1 - 1)),
                    _ => return None,
                }
            }
            _ => {
                let (value, needs) = self.value_of(expr, &as_is)?;
                if !needs.iter().all(|n| self.facts.holds(n)) {
                    return None;
                }
                match value.is_number() {
                    true => (value.offset, value.offset),
                    false => {
                        // `m - t <= d` is `t - m >= -d`; `t - m <= d` is `t - m <= d`. Where
                        // one isn't known, it's as far as the type goes.
                        let (term, minus) = (&value.term, &value.minus);
                        let (min, max) = self.int_limits(expr)?;
                        let least = match self.facts.bound(minus, term) {
                            Some(below) => value.offset.checked_sub(below)?,
                            None => min,
                        };
                        let most = match self.facts.bound(term, minus) {
                            Some(above) => value.offset.checked_add(above)?,
                            None => max,
                        };
                        (least, most)
                    }
                }
            }
        };
        let (min, max) = self.int_limits(expr)?;
        (min <= least && most <= max).then_some((least, most))
    }

    /// The least and most a value of `expr`'s integer type can be.
    fn int_limits(&self, expr: Id<Expr>) -> Option<(i128, i128)> {
        let ty = self.through_ref(self.expr_ty(expr)?);
        let Ty::Int(int) = *self.typed.ty(ty) else {
            return None;
        };
        let bits = match int {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::ISize | IntTy::USize => 64,
            IntTy::I128 | IntTy::U128 => 128,
        };
        Some(match (int.is_signed(), bits) {
            (true, 128) => (i128::MIN, i128::MAX),
            (true, n) => (-(1 << (n - 1)), (1 << (n - 1)) - 1),
            (false, 128) => (0, i128::MAX),
            (false, n) => (0, (1i128 << n) - 1),
        })
    }

    /// The integer a constant's value is: `W * H` of `@const W = 60;` and `@const H = 20;`
    /// is 1200. `None` where it isn't one, or a step wraps around.
    fn const_int(&self, expr: Id<Expr>) -> Option<i128> {
        let n = match self.ast.exprs[expr] {
            Expr::Int(n) => n,
            Expr::Neg(inner) => self.const_int(inner)?.checked_neg()?,
            Expr::BitNot(inner) => !self.const_int(inner)?,
            Expr::Cast { value, .. } => self.const_int(value)?,
            Expr::Var(ref path) if let Some((_, n)) = path.int_limit() => n,
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                let DefKind::Const(c) = self.names.def(def).kind else {
                    return None;
                };
                // Worked out by running it, before types were checked.
                match self.typed.const_value(c) {
                    Some(som_typeck::ConstValue::Int(n)) => n,
                    Some(_) => return None,
                    None => self.const_int(self.ast.consts[c].value)?,
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let (a, b) = (self.const_int(lhs)?, self.const_int(rhs)?);
                match op {
                    BinOp::Add => a.checked_add(b)?,
                    BinOp::Subtract => a.checked_sub(b)?,
                    BinOp::Multiply => a.checked_mul(b)?,
                    BinOp::Divide => a.checked_div(b)?,
                    BinOp::Modulo => a.checked_rem(b)?,
                    BinOp::BitAnd => a & b,
                    BinOp::BitOr => a | b,
                    BinOp::BitXor => a ^ b,
                    BinOp::ShiftLeft => a.checked_shl(u32::try_from(b).ok()?)?,
                    BinOp::ShiftRight => a.checked_shr(u32::try_from(b).ok()?)?,
                    _ => return None,
                }
            }
            _ => return None,
        };
        // Within its type, or it wrapped around on the way.
        let (min, max) = self.int_limits(expr)?;
        (min <= n && n <= max).then_some(n)
    }

    /// What a call of a function without parameters always gives, if its body is only a
    /// number: `fn width() -> i32 = 60;`, or `{ return 60; }`.
    fn constant_call(&self, expr: Id<Expr>) -> Option<i128> {
        let Expr::Call { ref args, .. } = self.ast.exprs[expr] else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let func = &self.ast.funcs[self.callee(expr)?];
        if !func.params.is_empty() || !func.generics.is_empty() {
            return None;
        }
        let mut body = func.body?;
        loop {
            body = match self.ast.exprs[body] {
                Expr::Int(n) => return Some(n),
                Expr::Neg(inner) => match self.ast.exprs[inner] {
                    Expr::Int(n) => return Some(-n),
                    _ => return None,
                },
                Expr::Block { ref stmts, value } => match (&stmts[..], value) {
                    ([], Some(value)) => value,
                    ([only], None) => match self.ast.stmts[*only] {
                        Stmt::Return(Some(value)) => value,
                        _ => return None,
                    },
                    _ => return None,
                },
                _ => return None,
            };
        }
    }

    /// A call `call` of `func`: each of its `@where`s has to be known to hold.
    fn check_call(
        &mut self,
        call: Id<Expr>,
        func: Id<Func>,
        first: usize,
        receiver: Option<Id<Expr>>,
        args: &[Id<Expr>],
    ) {
        for unmet in self.unproven(call, func, first, receiver, args) {
            let title = msg![
                "check it first: ",
                code(format!("if {} {{ … }}", unmet.text))
            ];
            let fix = self.check_fix(&unmet.text, title);
            self.errors.push(ProveError::Unproven {
                func: self.ast.funcs[func].name.name.to_string(),
                condition: unmet.text,
                reason: unmet.reason,
                never: unmet.never,
                span: self.ast.exprs.span(call).clone(),
                declared: self.ast.exprs.span(unmet.condition).clone(),
                fix,
            });
        }
    }

    /// The `@where`s of `func` not known to hold at a call of it, with `receiver` and `args`
    /// for its parameters from the `first`th; each with how it reads there.
    fn unproven(
        &mut self,
        call: Id<Expr>,
        func: Id<Func>,
        first: usize,
        receiver: Option<Id<Expr>>,
        args: &[Id<Expr>],
    ) -> Vec<Unmet> {
        let mut given: Vec<Option<Id<Expr>>> = vec![None; first];
        if first == 1 {
            given[0] = receiver;
        }
        given.extend(args.iter().map(|&a| Some(a)));
        let mut values = Vec::new();
        let mut needs = Vec::new();
        for arg in &given {
            match arg.and_then(|a| self.value_of(a, &as_is)) {
                Some((value, n)) => {
                    values.push(Some(value));
                    needs.extend(n);
                }
                None => values.push(None),
            }
        }
        // `N` of `first<N>`, at this call: 3, or the caller's own `M`.
        let generics: Vec<(DefId, Value<Place>)> = (self.names.type_params(func).iter())
            .zip(self.typed.type_args(call))
            .filter_map(|(&param, &arg)| Some((param, self.generic_value(arg)?)))
            .collect();
        let mut failed = Vec::new();
        for (clause, declared) in self.requires_of(func) {
            let condition = clause.condition;
            let params = self.names.params(declared).to_vec();
            // A parameter whose argument isn't followed stands for what's never known.
            let subst =
                |def: DefId| -> Option<Value<Place>> {
                    if let Some((_, value)) = generics.iter().find(|(p, _)| *p == def) {
                        return Some(value.clone());
                    }
                    let index = params.iter().position(|&d| d == def)?;
                    Some(values.get(index).cloned().flatten().unwrap_or(Value::place(
                        Place::Path {
                            root: def,
                            path: vec!["?".into()],
                        },
                    )))
                };
            let cond = self.cond(condition, true, &subst);
            let all = Cond::All(needs.iter().cloned().chain([cond]).collect());
            if !self.facts.holds(&all) {
                let never = self.facts.refuted(&all);
                let written: Vec<(DefId, String)> = (params.iter().copied())
                    .zip(&given)
                    .filter_map(|(def, arg)| {
                        Some((def, self.ast.exprs.span((*arg)?).get_text().to_string()))
                    })
                    // A caller's own `M` for `N`: how it reads there.
                    .chain(generics.iter().filter_map(|(param, value)| match &value.term {
                        Term::Place(Place::Len(own)) if value.offset == 0 => {
                            Some((*param, self.names.def(*own).name.to_string()))
                        }
                        _ => None,
                    }))
                    .collect();
                failed.push(Unmet {
                    condition,
                    text: self.condition_text(condition, &written),
                    reason: clause.reason.as_deref().map(String::from),
                    never,
                });
            }
        }
        failed
    }

    /// What a generic value argument is to the prover: a number, or a value parameter of the
    /// function it's in.
    fn generic_value(&mut self, arg: TypeId) -> Option<Value<Place>> {
        match *self.typed.ty(arg) {
            Ty::Value(som_typeck::Value::Int(n)) => Some(Value::number(n as i128)),
            Ty::Param(def) => Some(match self.bound.get(&def) {
                Some(&n) => Value::number(n),
                None => {
                    self.facts.unsigned(Place::Len(def));
                    Value::place(Place::Len(def))
                }
            }),
            _ => None,
        }
    }

    /// Function `func`'s `@where`s, each with the function whose parameters it names: its
    /// own, or for a trait's method in an implementation, the trait's.
    fn requires_of(&self, func: Id<Func>) -> Vec<(som_ast::Where, Id<Func>)> {
        let declared = self.trait_declaration(func).unwrap_or(func);
        (self.ast.funcs[declared].requires.iter())
            .map(|c| (c.clone(), declared))
            .collect()
    }

    /// For a method implementing a trait's: the trait's, from it or a trait it builds on.
    fn trait_declaration(&self, func: Id<Func>) -> Option<Id<Func>> {
        let block = self.names.impl_of(func)?;
        let trait_ = self.names.impls[block].trait_?;
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        let name = &self.ast.funcs[func].name.name;
        let supers = self
            .names
            .supers(t)
            .iter()
            .filter_map(|&s| match self.names.def(s).kind {
                DefKind::Trait(t) => Some(t),
                _ => None,
            });
        std::iter::once(t).chain(supers).find_map(|t| {
            (self.ast.traits[t].methods.iter().copied())
                .find(|&m| self.ast.funcs[m].name.name == *name)
        })
    }

    /// Learns function `func`'s `@where`s, which its body relies on.
    fn assume_requires(&mut self, func: Id<Func>) {
        let own = self.names.params(func).to_vec();
        for (clause, declared) in self.requires_of(func) {
            let condition = clause.condition;
            let theirs = self.names.params(declared).to_vec();
            let subst = |def: DefId| {
                let index = theirs.iter().position(|&d| d == def)?;
                Some(Value::place(Place::Path {
                    root: *own.get(index)?,
                    path: Vec::new(),
                }))
            };
            let cond = self.cond(condition, true, &subst);
            self.facts.assume(&cond);
        }
    }

    /// Condition `expr` as it reads with each parameter in `written` replaced by its
    /// argument: `i < self.len` at `xs.at(k)` is `k < xs.len`.
    fn condition_text(&self, expr: Id<Expr>, written: &[(DefId, String)]) -> String {
        let text = |e: Id<Expr>| self.condition_text(e, written);
        match &self.ast.exprs[expr] {
            Expr::Int(n) => n.to_string(),
            Expr::Bool(b) => b.to_string(),
            Expr::Var(path) => {
                let arg =
                    (self.names.expr(expr)).and_then(|def| written.iter().find(|(d, _)| *d == def));
                match arg {
                    Some((_, arg))
                        if arg.chars().all(|c| c.is_alphanumeric() || "_.".contains(c)) =>
                    {
                        arg.clone()
                    }
                    Some((_, arg)) => format!("({arg})"),
                    None => path.to_string(),
                }
            }
            Expr::Field { object, field } => format!("{}.{}", text(*object), field.name),
            Expr::Deref(inner) => text(*inner),
            Expr::Not(inner) => format!("!{}", text(*inner)),
            Expr::Binary { op, lhs, rhs } => {
                format!("{} {} {}", text(*lhs), op.symbol(), text(*rhs))
            }
            _ => self.ast.exprs.span(expr).get_text().to_string(),
        }
    }

    // --- Conditions and values ---

    /// What `expr` says when it's `truth`: `i < xs.len` when true, `i >= xs.len` when false.
    fn cond(&mut self, expr: Id<Expr>, truth: bool, subst: Subst) -> Cond<Place> {
        match self.ast.exprs[expr] {
            Expr::Bool(value) => match value == truth {
                true => Cond::always(),
                false => Cond::never(),
            },
            Expr::Not(inner) => self.cond(inner, !truth, subst),
            Expr::Binary { op, lhs, rhs } => {
                let compare = match op {
                    BinOp::And | BinOp::Or => {
                        let both = vec![self.cond(lhs, truth, subst), self.cond(rhs, truth, subst)];
                        // `a and b` being false is `!a or !b`.
                        return match (op == BinOp::And) == truth {
                            true => Cond::All(both),
                            false => Cond::Any(both),
                        };
                    }
                    BinOp::LessThan => Compare::Less,
                    BinOp::LessThanOrEqual => Compare::LessOrEqual,
                    BinOp::GreaterThan => Compare::Greater,
                    BinOp::GreaterThanOrEqual => Compare::GreaterOrEqual,
                    BinOp::Equals => Compare::Equal,
                    BinOp::NotEquals => Compare::NotEqual,
                    _ => return Cond::Unknown,
                };
                if !self.is_integer(lhs) {
                    return Cond::Unknown;
                }
                let compare = if truth { compare } else { compare.opposite() };
                let (Some((x, x_needs)), Some((y, y_needs))) =
                    (self.value_of(lhs, subst), self.value_of(rhs, subst))
                else {
                    return Cond::Unknown;
                };
                let cond = Cond::compare(&x, compare, &y);
                // `xs.len - 1` is only one less than `xs.len` where it doesn't wrap.
                let needs: Vec<_> = x_needs.into_iter().chain(y_needs).collect();
                match needs.is_empty() {
                    true => cond,
                    false => Cond::All(needs.into_iter().chain([cond]).collect()),
                }
            }
            Expr::Var(_) | Expr::Field { .. } if self.is_bool(expr) => {
                match self.value_of(expr, subst) {
                    Some((
                        Value {
                            term: Term::Place(place),
                            minus: Term::Zero,
                            offset: 0,
                        },
                        _,
                    )) => Cond::Is(place, truth),
                    _ => Cond::Unknown,
                }
            }
            _ => Cond::Unknown,
        }
    }

    /// `expr` as a value followed, if it's one: a number written out; a variable, field or
    /// property of one; an array's length; or one of those plus or minus a number. With
    /// what has to hold for it not to wrap around: `xs.len - 1` of a `usize` needs
    /// `xs.len >= 1`.
    fn value_of(
        &mut self,
        expr: Id<Expr>,
        subst: Subst,
    ) -> Option<(Value<Place>, Vec<Cond<Place>>)> {
        // Worked out while compiling: `3 * W + 5`.
        if let Some(n) = self.const_int(expr) {
            return Some((Value::number(n), Vec::new()));
        }
        let value = match self.ast.exprs[expr] {
            Expr::Int(n) => Value::number(n),
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                if let Some(value) = subst(def) {
                    return Some((value, Vec::new()));
                }
                match self.names.def(def).kind {
                    DefKind::Local(_) | DefKind::Param { .. } | DefKind::Binding(_) => {
                        Value::place(Place::Path {
                            root: def,
                            path: Vec::new(),
                        })
                    }
                    // `W`, of `@const W = 60;`; or, for one whose value runs code, worked out
                    // after this, a value nothing changes: what's checked of it holds.
                    DefKind::Const(c) => match self.const_int(self.ast.consts[c].value) {
                        Some(n) => Value::number(n),
                        None => Value::place(Place::Path {
                            root: def,
                            path: Vec::new(),
                        }),
                    },
                    // `N`, proving one instance: the number it stands for there.
                    DefKind::TypeParam { value: true, .. }
                    | DefKind::StructParam { value: true, .. }
                    | DefKind::ImplParam { value: true, .. }
                        if let Some(&n) = self.bound.get(&def) =>
                    {
                        Value::number(n)
                    }
                    // `N`, of `[T; N]`.
                    DefKind::TypeParam { value: true, .. }
                    | DefKind::StructParam { value: true, .. }
                    | DefKind::ImplParam { value: true, .. } => {
                        self.facts.unsigned(Place::Len(def));
                        Value::place(Place::Len(def))
                    }
                    _ => return None,
                }
            }
            Expr::Field { object, ref field } => {
                if &*field.name == "len"
                    && let Some(len) = self.array_len(object)
                {
                    return Some((len, Vec::new()));
                }
                let (base, needs) = self.value_of(object, subst)?;
                let Value {
                    term: Term::Place(Place::Path { root, mut path }),
                    minus: Term::Zero,
                    offset: 0,
                } = base
                else {
                    return None;
                };
                path.push(field.name.clone());
                let _ = needs;
                let place = Place::Path { root, path };
                // A slice's length is never below 0, whatever integer type it's read as.
                if &*field.name == "len" {
                    let mut ty = self.typed.expr(object);
                    while let Ty::Ref { to, .. } = *self.typed.ty(ty) {
                        ty = to;
                    }
                    if matches!(self.typed.ty(ty), Ty::Slice(_) | Ty::Str) {
                        self.facts.unsigned(place.clone());
                    }
                }
                Value::place(place)
            }
            Expr::Deref(inner) => return self.value_of(inner, subst),
            // `width()` of `fn width() -> i32 = 60;`.
            Expr::Call { .. } => Value::number(self.constant_call(expr)?),
            Expr::Binary {
                op: op @ (BinOp::Add | BinOp::Subtract),
                lhs,
                rhs,
            } => {
                let (n, other) = match (&self.ast.exprs[lhs], &self.ast.exprs[rhs]) {
                    (_, &Expr::Int(n)) => (n, lhs),
                    (&Expr::Int(n), _) if op == BinOp::Add => (n, rhs),
                    // `N - 1 - i`: one term less another.
                    _ => {
                        let (a, mut needs) = self.value_of(lhs, subst)?;
                        let (b, more) = self.value_of(rhs, subst)?;
                        needs.extend(more);
                        let value = match op {
                            BinOp::Add => a.plus(&b)?,
                            _ => {
                                // `a - b` doesn't wrap where `a >= b`.
                                if self.is_unsigned(expr) {
                                    needs.push(Cond::compare(&a, Compare::GreaterOrEqual, &b));
                                }
                                a.less(&b)?
                            }
                        };
                        return Some((value, needs));
                    }
                };
                let (mut value, mut needs) = self.value_of(other, subst)?;
                if op == BinOp::Add {
                    value.offset += n;
                } else {
                    if self.is_unsigned(expr) {
                        // `v - n` doesn't wrap where `v >= n`.
                        needs.push(Cond::compare(
                            &value,
                            Compare::GreaterOrEqual,
                            &Value::number(n),
                        ));
                    }
                    value.offset -= n;
                }
                return Some((value, needs));
            }
            _ => return None,
        };
        if self.is_unsigned(expr)
            && value.minus == Term::Zero
            && let Term::Place(place) = &value.term
        {
            self.facts.unsigned(place.clone());
        }
        Some((value, Vec::new()))
    }

    /// The length of array `object`, if it is one: a number, or its length parameter.
    fn array_len(&mut self, object: Id<Expr>) -> Option<Value<Place>> {
        let mut ty = self.typed.expr(object);
        if let Ty::Ref { to, .. } = *self.typed.ty(ty) {
            ty = to;
        }
        let Ty::Array { len, .. } = *self.typed.ty(ty) else {
            return None;
        };
        match *self.typed.ty(len) {
            Ty::Value(som_typeck::Value::Int(n)) => Some(Value::number(n as i128)),
            Ty::Param(def) => {
                self.facts.unsigned(Place::Len(def));
                Some(Value::place(Place::Len(def)))
            }
            _ => None,
        }
    }

    // --- Changes ---

    /// `place` may have changed: what's known about it, and inside it, is forgotten. An
    /// element, `xs[i]`, changes on its own: `at_mut` lends it without changing `xs`.
    fn changed(&mut self, place: Id<Expr>) {
        if let Some(changed) = self.place_of(place) {
            self.facts
                .forget(|p| p.within(&changed) || changed.within(p));
        }
    }

    /// The variable, or field of one, expression `expr` is a place in: `xs` for `xs[i]`.
    fn place_of(&self, expr: Id<Expr>) -> Option<Place> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => Some(Place::Path {
                root: self.names.expr(expr)?,
                path: Vec::new(),
            }),
            Expr::Field { object, field } => match self.place_of(*object)? {
                Place::Path { root, mut path } => {
                    path.push(field.name.clone());
                    Some(Place::Path { root, path })
                }
                len => Some(len),
            },
            Expr::Deref(inner) => self.place_of(*inner),
            _ => None,
        }
    }

    /// Before a loop: forgets what its body may change, since a later time round sees it.
    fn forget_changed_in(&mut self, body: &[Id<Stmt>]) {
        if self.facts.is_empty() {
            return;
        }
        let mut changed = Vec::new();
        for &stmt in body {
            self.changes_in_stmt(stmt, &mut changed);
        }
        // A counter only ever counted up, `i = i + 1`, is still at least what it was.
        let rising: Vec<Place> = (changed.iter())
            .filter(|place| matches!(place, Place::Path { path, .. } if path.is_empty()))
            .filter(|&place| self.only_rises(body, place))
            .cloned()
            .collect();
        let touches = |place: &Place, p: &Place| p.within(place) || place.within(p);
        for place in &changed {
            if rising.contains(place) {
                let others: Vec<&Place> = changed.iter().filter(|c| *c != place).collect();
                self.facts.forget_rising(place, |p| {
                    touches(place, p) || others.iter().any(|o| touches(o, p))
                });
            } else {
                self.facts.forget(|p| touches(place, p));
            }
        }
    }

    /// Whether every change to variable `place` in `body` adds a number not below 0 to it:
    /// `i = i + 1`, `i += 2`.
    fn only_rises(&self, body: &[Id<Stmt>], place: &Place) -> bool {
        let mut rises = true;
        for &stmt in body {
            self.stmt_exprs_deep(stmt, &mut |expr| {
                let mut changed = Vec::new();
                match self.ast.exprs[expr] {
                    Expr::Assign { target, value } if self.place_of(target).as_ref() == Some(place) => {
                        let up = match self.ast.exprs[value] {
                            Expr::Binary {
                                op: BinOp::Add,
                                lhs,
                                rhs,
                            } => match (&self.ast.exprs[lhs], &self.ast.exprs[rhs]) {
                                (_, &Expr::Int(n)) => n >= 0 && self.place_of(lhs).as_ref() == Some(place),
                                (&Expr::Int(n), _) => n >= 0 && self.place_of(rhs).as_ref() == Some(place),
                                _ => false,
                            },
                            _ => false,
                        };
                        rises &= up;
                    }
                    Expr::Assign { .. } => {}
                    // Lent as `&mut`, or to a method that may change it: anything could happen.
                    _ => {
                        self.changes_here(expr, &mut changed);
                        rises &= !changed.iter().any(|c| c.within(place) || place.within(c));
                    }
                }
            });
        }
        rises
    }

    /// The places `expr` itself may change, not counting what's inside it.
    fn changes_here(&self, expr: Id<Expr>, changed: &mut Vec<Place>) {
        let target = match &self.ast.exprs[expr] {
            Expr::Assign { target, .. } => Some(*target),
            Expr::Borrow {
                mutable: true,
                expr,
            } => Some(*expr),
            Expr::MethodCall { receiver, .. }
                if self.method(expr).is_some_and(|f| self.takes_mut_self(f)) =>
            {
                Some(*receiver)
            }
            _ => None,
        };
        if let Some(place) = target.and_then(|t| self.place_of(t)) {
            changed.push(place);
        }
    }

    /// Calls `f` with every expression in `stmt`, however deep.
    fn stmt_exprs_deep(&self, stmt: Id<Stmt>, f: &mut dyn FnMut(Id<Expr>)) {
        fn walk(p: &Prover, expr: Id<Expr>, f: &mut dyn FnMut(Id<Expr>)) {
            f(expr);
            p.each_child(expr, &mut |child| walk(p, child, f));
        }
        self.stmt_exprs(stmt, &mut |e| walk(self, e, f));
    }

    fn changes_in_stmt(&self, stmt: Id<Stmt>, changed: &mut Vec<Place>) {
        match &self.ast.stmts[stmt] {
            Stmt::Let { value, .. } => self.changes_in(*value, changed),
            Stmt::Expr(e) => self.changes_in(*e, changed),
            Stmt::Return(Some(e)) => self.changes_in(*e, changed),
            Stmt::Loop { body } => body.iter().for_each(|&s| self.changes_in_stmt(s, changed)),
            Stmt::While { condition, body } => {
                self.changes_in(*condition, changed);
                body.iter().for_each(|&s| self.changes_in_stmt(s, changed));
            }
            Stmt::For { iterable, body, .. } => {
                self.changes_in(*iterable, changed);
                body.iter().for_each(|&s| self.changes_in_stmt(s, changed));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                self.changes_in(*condition, changed);
                then.iter()
                    .chain(otherwise.iter().flatten())
                    .for_each(|&s| self.changes_in_stmt(s, changed));
            }
            _ => {}
        }
    }

    /// The places `expr` may change: assigned, lent as `&mut`, or given to `&mut self`.
    fn changes_in(&self, expr: Id<Expr>, changed: &mut Vec<Place>) {
        let mut add = |e: Id<Expr>| {
            if let Some(place) = self.place_of(e) {
                changed.push(place);
            }
        };
        match &self.ast.exprs[expr] {
            Expr::Assign { target, .. } => add(*target),
            Expr::Borrow {
                mutable: true,
                expr,
            } => add(*expr),
            Expr::MethodCall { receiver, .. }
                if self.method(expr).is_some_and(|f| self.takes_mut_self(f)) =>
            {
                add(*receiver)
            }
            _ => {}
        }
        self.each_child(expr, &mut |child| self.changes_in(child, changed));
    }

    /// Calls `f` with each expression directly inside `expr`, and each statement's.
    fn each_child(&self, expr: Id<Expr>, f: &mut dyn FnMut(Id<Expr>)) {
        match &self.ast.exprs[expr] {
            Expr::Array(values) => values.iter().for_each(|&v| f(v)),
            Expr::Interpolated(parts) => {
                for &part in parts {
                    if let som_ast::Part::Value(value) = part {
                        f(value);
                    }
                }
            }
            Expr::Cast { value, .. } | Expr::ArrayRepeat { value, .. } => f(*value),
            Expr::Range { start, end } => start.iter().chain(end).for_each(|&e| f(e)),
            Expr::Binary { lhs, rhs, .. } => {
                f(*lhs);
                f(*rhs);
            }
            Expr::Not(e) | Expr::Neg(e) | Expr::BitNot(e) | Expr::Deref(e) => f(*e),
            Expr::Borrow { expr, .. } => f(*expr),
            Expr::Assign { target, value } => {
                f(*target);
                f(*value);
            }
            Expr::Call { args, .. } => args.iter().for_each(|&a| f(a)),
            Expr::MethodCall { receiver, args, .. } => {
                f(*receiver);
                args.iter().for_each(|&a| f(a));
            }
            Expr::Field { object, .. } => f(*object),
            Expr::Index { object, index } => {
                f(*object);
                f(*index);
            }
            Expr::Construct { fields, .. } => fields.iter().for_each(|&(_, v)| f(v)),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                f(*condition);
                f(*then);
                f(*otherwise);
            }
            Expr::Fallback { optional, fallback } => {
                f(*optional);
                f(*fallback);
            }
            Expr::Match { .. } if let Some(copy) = self.typed.chosen_match(expr) => f(copy),
            Expr::Match { scrutinee, arms } => {
                f(*scrutinee);
                arms.iter().for_each(|arm| f(arm.body));
            }
            Expr::Block { stmts, value } => {
                for &stmt in stmts {
                    self.stmt_exprs(stmt, f);
                }
                if let Some(value) = value {
                    f(*value);
                }
            }
            _ => {}
        }
    }

    fn stmt_exprs(&self, stmt: Id<Stmt>, f: &mut dyn FnMut(Id<Expr>)) {
        match &self.ast.stmts[stmt] {
            Stmt::Let { value, .. } | Stmt::Expr(value) | Stmt::Return(Some(value)) => f(*value),
            Stmt::Loop { body } => body.iter().for_each(|&s| self.stmt_exprs(s, f)),
            Stmt::While { condition, body }
            | Stmt::For {
                iterable: condition,
                body,
                ..
            } => {
                f(*condition);
                body.iter().for_each(|&s| self.stmt_exprs(s, f));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                f(*condition);
                then.iter()
                    .chain(otherwise.iter().flatten())
                    .for_each(|&s| self.stmt_exprs(s, f));
            }
            _ => {}
        }
    }

    /// Whether `stmts` has a `break` anywhere: one that may leave the loop they're in.
    fn breaks(&self, stmts: &[Id<Stmt>]) -> bool {
        let mut found = false;
        for &stmt in stmts {
            found |= match &self.ast.stmts[stmt] {
                Stmt::Break(_) => true,
                Stmt::Loop { body } | Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    self.breaks(body)
                }
                Stmt::If {
                    then, otherwise, ..
                } => self.breaks(then) || otherwise.as_ref().is_some_and(|o| self.breaks(o)),
                _ => false,
            };
            self.stmt_exprs(stmt, &mut |e| found |= self.expr_breaks(e));
        }
        found
    }

    /// Whether a block inside `expr` has a `break`.
    fn expr_breaks(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Block { stmts, .. } => self.breaks(stmts),
            _ => {
                let mut found = false;
                self.each_child(expr, &mut |child| found |= self.expr_breaks(child));
                found
            }
        }
    }

    /// Whether control never gets past the end of `stmts`.
    fn leaves(&self, stmts: &[Id<Stmt>]) -> bool {
        let Some(&last) = stmts.last() else {
            return false;
        };
        match &self.ast.stmts[last] {
            Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => true,
            Stmt::If {
                then, otherwise, ..
            } if let Some(holds) = self.typed.settled_if(last) => match holds {
                true => self.leaves(then),
                false => otherwise.as_ref().is_some_and(|o| self.leaves(o)),
            },
            Stmt::If {
                then,
                otherwise: Some(otherwise),
                ..
            } => self.leaves(then) && self.leaves(otherwise),
            Stmt::Expr(expr) => {
                self.typed.diverges(*expr)
                    || matches!(self.ast.exprs[*expr], Expr::Unreachable | Expr::Fail(_))
            }
            _ => false,
        }
    }

    // --- What things are ---

    fn callee(&self, call: Id<Expr>) -> Option<Id<Func>> {
        match self.names.def(self.typed.callee(call, self.names)?).kind {
            DefKind::Func(func) => Some(func),
            _ => None,
        }
    }

    fn method(&self, expr: Id<Expr>) -> Option<Id<Func>> {
        match self.names.def(self.typed.method(expr)?).kind {
            DefKind::Func(func) => Some(func),
            _ => None,
        }
    }

    /// Whether method `func` takes its `self` as `&mut`, and so may change it.
    fn takes_mut_self(&self, func: Id<Func>) -> bool {
        (self.typed.func(func).params.first())
            .is_some_and(|&p| matches!(self.typed.ty(p), Ty::Ref { mutable: true, .. }))
    }

    /// The local variable `expr` is, if it's one.
    fn local_var(&self, expr: Id<Expr>) -> Option<DefId> {
        let Expr::Var(_) = self.ast.exprs[expr] else {
            return None;
        };
        let def = self.names.expr(expr)?;
        matches!(
            self.names.def(def).kind,
            DefKind::Local(_) | DefKind::Param { .. } | DefKind::Binding(_)
        )
        .then_some(def)
    }

    /// Whether `expr` reads variable `def`.
    fn mentions(&self, expr: Id<Expr>, def: DefId) -> bool {
        if let Expr::Var(_) = self.ast.exprs[expr]
            && self.names.expr(expr) == Some(def)
        {
            return true;
        }
        let mut found = false;
        self.each_child(expr, &mut |child| found |= self.mentions(child, def));
        found
    }

    fn expr_ty(&self, expr: Id<Expr>) -> Option<TypeId> {
        Some(self.typed.expr(expr))
    }

    fn is_bool(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr)
            .is_some_and(|ty| matches!(self.typed.ty(ty), Ty::Bool))
    }

    fn is_integer(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr)
            .is_some_and(|ty| matches!(self.typed.ty(self.through_ref(ty)), Ty::Int(_)))
    }

    fn is_unsigned(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr).is_some_and(|ty| self.is_unsigned_ty(ty))
    }

    fn is_unsigned_ty(&self, ty: TypeId) -> bool {
        matches!(
            self.typed.ty(self.through_ref(ty)),
            Ty::Int(IntTy::U8 | IntTy::U16 | IntTy::U32 | IntTy::U64 | IntTy::U128 | IntTy::USize)
        )
    }

    fn through_ref(&self, ty: TypeId) -> TypeId {
        match *self.typed.ty(ty) {
            Ty::Ref { to, .. } => to,
            _ => ty,
        }
    }
}

impl Value<Place> {
    fn term_place(&self) -> &Place {
        match &self.term {
            Term::Place(place) => place,
            Term::Zero => unreachable!("made from a place"),
        }
    }
}

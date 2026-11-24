//! What each function giving a `T!` can fail with, worked out from its body once the whole
//! program is checked: the types of the values its `fail`s are given, and what the `T!`s it
//! passes on with `?` can fail with. A `fail "…"` in the `else` of a failure says what was
//! being done and fails with that failure, so it adds nothing of its own.
//!
//! A function that says what it fails with, `T!ParseError`, fails with that and nothing
//! else: its body is checked to keep to it. Then each `match` on a `T!` without an arm for any
//! failure is checked to have one for each type it can fail with.
//!
//! Where it can't be known, as through a function's address, or of a `T!` passed in, a `T!`
//! can fail with anything.

use som_ast::{Ast, Child, Expr, Func, Id, Pattern, Stmt, TypeExpr};
use som_base::Span;
use som_base::message::{Message, code};
use som_lang::LangStruct;
use som_resolve::{DefId, DefKind, Names};
use std::collections::{BTreeSet, HashMap};

use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::Typed;

/// What something can fail with: values of these structs and enums, or with `anything`,
/// values of types not known here too.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FailSet {
    pub kinds: BTreeSet<DefId>,
    pub anything: bool,
}

impl FailSet {
    fn anything() -> Self {
        FailSet {
            kinds: BTreeSet::new(),
            anything: true,
        }
    }

    fn add(&mut self, other: &FailSet) {
        self.kinds.extend(other.kinds.iter().copied());
        self.anything |= other.anything;
    }

    /// As it's written: `ParseError`, `(IoError, ParseError)`, `()` for nothing, or with
    /// `anything`, `…`.
    pub fn text(&self, names: &Names) -> String {
        let mut parts: Vec<String> = (self.kinds.iter())
            .map(|&k| names.def(k).name.to_string())
            .collect();
        if self.anything {
            parts.push("…".into());
        }
        match parts.len() {
            1 => parts.remove(0),
            _ => format!("({})", parts.join(", ")),
        }
    }
}

/// What each function giving a `T!` fails with: what it says, or what was worked out.
#[derive(Clone, Debug, Default)]
pub struct Failures {
    pub sets: HashMap<Id<Func>, FailSet>,
    /// The functions saying what they fail with.
    pub declared: HashMap<Id<Func>, FailSet>,
}

pub(crate) fn infer(ast: &Ast, names: &Names, typed: &Typed) -> (Failures, Vec<TypeError>) {
    let Some(outcome) = names.lang_struct(LangStruct::Outcome) else {
        return (Failures::default(), Vec::new());
    };
    let error = names.lang_struct(LangStruct::Error);
    let mut failures = Failures::default();
    let mut failing = Vec::new();
    for (id, func) in ast.funcs.iter() {
        let ret = typed.func(id).ret;
        if !matches!(typed.ty(ret), Ty::Struct(def, _) if *def == outcome) {
            continue;
        }
        if let Some(declared) = declared(ast, names, func) {
            failures.declared.insert(id, declared.clone());
            failures.sets.insert(id, declared);
        } else {
            failures.sets.insert(id, FailSet::default());
        }
        if func.body.is_some() {
            failing.push(id);
        }
    }

    // Each set only grows, and there are only so many types: it settles.
    loop {
        let mut changed = false;
        for &id in &failing {
            if failures.declared.contains_key(&id) {
                continue;
            }
            let mut walk = Walk::new(ast, names, typed, &failures, outcome, error, false);
            walk.body(id);
            if failures.sets.get(&id) != Some(&walk.fails) {
                failures.sets.insert(id, walk.fails);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Then what's said is kept to, and every failure matched.
    let mut errors = Vec::new();
    for (id, func) in ast.funcs.iter() {
        if func.body.is_none() {
            continue;
        }
        let mut walk = Walk::new(ast, names, typed, &failures, outcome, error, true);
        walk.declared = failures.declared.get(&id).map(|d| {
            let at = func.returns.map(|t| ast.types.span(t).clone());
            (d.clone(), func.name.name.to_string(), at)
        });
        walk.body(id);
        errors.extend(walk.errors);
    }
    (failures, errors)
}

/// What `func` says it fails with, `T!ParseError`; `None` if it leaves it to be worked out.
fn declared(ast: &Ast, names: &Names, func: &Func) -> Option<FailSet> {
    let returns = func.returns?;
    let TypeExpr::Failable {
        errors: Some(errors),
        ..
    } = &ast.types[returns]
    else {
        return None;
    };
    let mut set = FailSet::default();
    for &error in errors {
        match names.ty(error) {
            Some(def) if matches!(names.def(def).kind, DefKind::Struct(_)) => {
                set.kinds.insert(def);
            }
            _ => set.anything = true,
        }
    }
    Some(set)
}

struct Walk<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    failures: &'a Failures,
    outcome: DefId,
    error: Option<DefId>,
    /// What the function fails with, so far.
    fails: FailSet,
    /// What the `T!`s in variables, and the `Error`s, can fail with.
    locals: HashMap<DefId, FailSet>,
    /// In the `else` of a failure with no name: what it can fail with, innermost last.
    handling: Vec<FailSet>,
    /// Reporting: what's wrong is said.
    report: bool,
    /// What the function says it fails with, its name, and where it says so.
    declared: Option<(FailSet, String, Option<Span>)>,
    errors: Vec<TypeError>,
}

impl<'a> Walk<'a> {
    fn new(
        ast: &'a Ast,
        names: &'a Names,
        typed: &'a Typed,
        failures: &'a Failures,
        outcome: DefId,
        error: Option<DefId>,
        report: bool,
    ) -> Self {
        Walk {
            ast,
            names,
            typed,
            failures,
            outcome,
            error,
            fails: FailSet::default(),
            locals: HashMap::new(),
            handling: Vec::new(),
            report,
            declared: None,
            errors: Vec::new(),
        }
    }

    fn body(&mut self, func: Id<Func>) {
        if let Some(body) = self.ast.funcs[func].body {
            self.expr(body);
        }
    }

    fn is_failable(&self, expr: Id<Expr>) -> bool {
        matches!(self.typed.ty(self.typed.expr(expr)), Ty::Struct(def, _) if *def == self.outcome)
    }

    /// The function fails with `set` here, at `at`.
    fn fails_with(&mut self, set: FailSet, at: Span) {
        if self.report
            && let Some((declared, name, said)) = &self.declared
        {
            let mut beyond: Vec<String> = (set.kinds.iter())
                .filter(|k| !declared.kinds.contains(k))
                .map(|&k| self.names.def(k).name.to_string())
                .collect();
            if set.anything && !declared.anything {
                beyond.push("…".into());
            }
            if !declared.anything && !beyond.is_empty() {
                self.errors.push(TypeError::FailsUndeclared {
                    func: name.clone(),
                    kinds: beyond,
                    span: at,
                    said: said.clone(),
                });
            }
        }
        self.fails.add(&set);
    }

    /// What the `T!` `expr` gives can fail with.
    fn set_of(&self, expr: Id<Expr>) -> FailSet {
        let func_set = |def: Option<DefId>| match def.map(|d| &self.names.def(d).kind) {
            Some(DefKind::Func(f)) => self.failures.sets.get(f).cloned().unwrap_or_else(FailSet::anything),
            _ => FailSet::anything(),
        };
        match &self.ast.exprs[expr] {
            Expr::Call { .. } => func_set(self.typed.callee(expr, self.names)),
            Expr::MethodCall { .. } => func_set(self.typed.method(expr)),
            Expr::Var(_) => match self.typed.callee(expr, self.names) {
                Some(def) => self.locals.get(&def).cloned().unwrap_or_else(FailSet::anything),
                None => FailSet::anything(),
            },
            Expr::Block { value: Some(value), .. } => self.set_of(*value),
            Expr::Conditional {
                then, otherwise, ..
            } => {
                let mut set = self.set_of(*then);
                set.add(&self.set_of(*otherwise));
                set
            }
            _ => FailSet::anything(),
        }
    }

    /// What an `Error` given to `fail` can fail with: the failure it was bound to.
    fn error_set(&self, value: Id<Expr>) -> FailSet {
        match (&self.ast.exprs[value], self.typed.callee(value, self.names)) {
            (Expr::Var(_), Some(def)) => self.locals.get(&def).cloned().unwrap_or_else(FailSet::anything),
            _ => FailSet::anything(),
        }
    }

    fn is_error(&self, ty: TypeId) -> bool {
        matches!(self.typed.ty(ty), Ty::Struct(def, _) if Some(*def) == self.error)
    }

    /// The struct or enum a value given to `fail` is; text is a `string`, a struct too.
    fn kind_of(&self, ty: TypeId) -> FailSet {
        match self.typed.ty(ty) {
            Ty::Struct(def, _) => FailSet {
                kinds: BTreeSet::from([*def]),
                anything: false,
            },
            _ => FailSet::anything(),
        }
    }

    fn is_text(&self, ty: TypeId) -> bool {
        matches!(self.typed.ty(ty), Ty::Struct(def, _) if Some(*def) == self.names.lang_struct(LangStruct::String))
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        if let Stmt::Let { value, .. } = &self.ast.stmts[id] {
            let value = *value;
            self.expr(value);
            if self.is_failable(value)
                && let Some(def) = self.names.local(id)
            {
                let set = self.set_of(value);
                self.locals.entry(def).or_default().add(&set);
            }
            return;
        }
        self.ast.stmts[id].for_each_child(|child| self.child(child));
    }

    fn child(&mut self, child: Child) {
        match child {
            Child::Expr(e) => self.expr(e),
            Child::Stmt(s) => self.stmt(s),
        }
    }

    fn expr(&mut self, id: Id<Expr>) {
        let ast = self.ast;
        match &ast.exprs[id] {
            // A closure's `fail`s are its own.
            Expr::Closure { .. } => {}
            Expr::Raise(value) => {
                self.expr(*value);
                let ty = self.typed.expr(*value);
                let set = if self.is_error(ty) {
                    self.error_set(*value)
                } else {
                    match self.handling.last() {
                        // `else fail "reading the port"`: the failure, said more of.
                        Some(handled) if self.is_text(ty) => handled.clone(),
                        _ => self.kind_of(ty),
                    }
                };
                self.fails_with(set, ast.exprs.span(id).clone());
            }
            Expr::Propagate(value) => {
                self.expr(*value);
                let set = self.set_of(*value);
                self.fails_with(set, ast.exprs.span(id).clone());
            }
            Expr::Fallback { optional, fallback } if self.is_failable(*optional) => {
                self.expr(*optional);
                let handled = self.set_of(*optional);
                self.handling.push(handled);
                self.expr(*fallback);
                self.handling.pop();
            }
            Expr::Recover {
                failable,
                error,
                body,
            } => {
                self.expr(*failable);
                let handled = self.set_of(*failable);
                if let Some(def) = self.names.pattern(*error) {
                    self.locals.insert(def, handled);
                }
                self.expr(*body);
            }
            Expr::Match { scrutinee, arms } if self.is_failable(*scrutinee) => {
                self.expr(*scrutinee);
                let matched = self.set_of(*scrutinee);
                for arm in arms {
                    if let Pattern::Failed(inner) = ast.patterns[arm.pattern]
                        && let Some(def) = self.names.pattern(inner)
                        && matches!(self.names.def(def).kind, DefKind::Binding(_))
                    {
                        self.locals.insert(def, matched.clone());
                    }
                    self.expr(arm.body);
                }
                if self.report {
                    self.check_match(id, *scrutinee, &matched);
                }
            }
            other => other.for_each_child(|child| self.child(child)),
        }
    }

    /// A `match` on a `T!` that can fail with `matched`: each failure has an arm, and each
    /// `fail` arm for a type matches some failure.
    fn check_match(&mut self, id: Id<Expr>, scrutinee: Id<Expr>, matched: &FailSet) {
        let Some(arms) = self.typed.failure_arms(id) else {
            return;
        };
        let ty: Message = code(display(self.typed, self.names, self.typed.expr(scrutinee))).into();
        // An arm for a type it can't fail with.
        if !matched.anything {
            for &(def, _, pattern) in &arms.kinds {
                if !matched.kinds.contains(&def) {
                    self.errors.push(TypeError::CantFailWith {
                        kind: self.names.def(def).name.to_string(),
                        fails: matched.text(self.names),
                        span: self.ast.patterns.span(pattern).clone(),
                    });
                }
            }
        }
        if arms.catch_all {
            return;
        }
        let covered = |def: DefId| arms.kinds.iter().any(|&(d, whole, _)| d == def && whole);
        let mut missing = Vec::new();
        for &kind in matched.kinds.iter().filter(|&&k| !covered(k)) {
            match arms.missing.iter().find(|(d, _)| *d == kind) {
                Some((_, arms)) => missing.extend(arms.iter().cloned()),
                None => missing.extend(self.every_value(kind)),
            }
        }
        if matched.anything {
            missing.push("fail e".into());
        }
        if missing.is_empty() {
            return;
        }
        self.errors.push(TypeError::NonExhaustiveMatch {
            ty,
            missing,
            span: self.ast.exprs.span(scrutinee).clone(),
            fix: None,
        });
    }
}

impl Walk<'_> {
    /// Arms matching every value of struct or enum `def`: one for each variant of an enum.
    fn every_value(&self, def: DefId) -> Vec<String> {
        let name = &self.names.def(def).name;
        let DefKind::Struct(s) = self.names.def(def).kind else {
            return vec![format!("fail {name} {{ .. }}")];
        };
        let strukt = &self.ast.structs[s];
        if !strukt.is_enum() {
            return vec![format!("fail {name} {{ .. }}")];
        }
        (strukt.variants().iter())
            .map(|v| match v.kind {
                som_ast::VariantKind::Unit => format!("fail {name}::{}", v.name.name),
                som_ast::VariantKind::Tuple => format!("fail {name}::{}(..)", v.name.name),
                som_ast::VariantKind::Named => format!("fail {name}::{} {{ .. }}", v.name.name),
            })
            .collect()
    }
}

fn display(typed: &Typed, names: &Names, ty: TypeId) -> String {
    typed.table.display(ty, names)
}

/// What `func` gives as it's written, with what a `T!` fails with spelled out after the `!`,
/// worked out or not: `i64!(NotFound, ParseError)`. Without anything after it, it never
/// fails; `!…` can fail with what isn't known here.
pub fn returns_text(typed: &Typed, names: &Names, func: Id<Func>) -> String {
    let ret = typed.table.display(typed.func(func).ret, names);
    match typed.failures.sets.get(&func) {
        Some(set) => format!("{ret}{}", set.text(names)),
        None => ret,
    }
}

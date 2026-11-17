//! Functions' `@where`s: known to hold at each call, and relied on in their bodies.

use som_ast::{Expr, Func, Id, Where};
use som_base::message::code;
use som_base::msg;
use som_resolve::{DefId, DefKind};
use som_typeck::{Ty, TypeId};

use crate::facts::{Cond, Term, Value};
use crate::{Place, ProveError, Prover, as_is};

/// Stands for an argument that isn't followed, so what's said of it is never known.
const UNFOLLOWED: &str = "?";

/// A `@where` not known to hold at a call: the condition, how it reads there, and what
/// it means.
pub(crate) struct Unmet {
    condition: Id<Expr>,
    text: String,
    reason: Option<String>,
    /// It's known not to hold.
    never: bool,
}

impl Prover<'_> {
    /// Function `func`'s own `@where`s: none on a trait's method in an implementation, and
    /// each one that can be followed.
    pub(crate) fn check_wheres(&mut self, id: Id<Func>) {
        let ast = self.ast;
        let func = &ast.funcs[id];
        // A trait's method has the trait's conditions, all a caller through the trait knows.
        if let (Some(clause), Some(declared)) = (func.requires.first(), self.trait_declaration(id))
        {
            let trait_ = (ast.traits.iter())
                .find(|(_, t)| t.methods.contains(&declared))
                .map(|(_, t)| t.name.name.to_string())
                .unwrap_or_default();
            self.errors.push(ProveError::WhereOnImpl {
                method: func.name.name.to_string(),
                trait_,
                span: ast.exprs.span(clause.condition).clone(),
            });
        }
        for clause in &func.requires {
            if self.cond(clause.condition, true, &as_is).has_unknown() {
                self.errors.push(ProveError::UnfollowedWhere {
                    span: ast.exprs.span(clause.condition).clone(),
                });
            }
        }
    }

    /// A call `call` of `func`: each of its `@where`s has to be known to hold.
    pub(crate) fn check_call(
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
    pub(crate) fn unproven(
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
            let subst = |def: DefId| -> Option<Value<Place>> {
                if let Some((_, value)) = generics.iter().find(|(p, _)| *p == def) {
                    return Some(value.clone());
                }
                let index = params.iter().position(|&d| d == def)?;
                let unfollowed = || {
                    Value::place(Place::Path {
                        root: def,
                        path: vec![UNFOLLOWED.into()],
                    })
                };
                Some(
                    values
                        .get(index)
                        .cloned()
                        .flatten()
                        .unwrap_or_else(unfollowed),
                )
            };
            let cond = self.cond(condition, true, &subst);
            let all = Cond::All(needs.iter().cloned().chain([cond]).collect());
            if self.facts.holds(&all) {
                continue;
            }
            let written = self.written_args(&params, &given, &generics);
            failed.push(Unmet {
                condition,
                text: self.condition_text(condition, &written),
                reason: clause.reason.as_deref().map(String::from),
                never: self.facts.refuted(&all),
            });
        }
        failed
    }

    /// How each parameter reads at a call: the argument as written, or for `N`, the caller's
    /// own `M`.
    fn written_args(
        &self,
        params: &[DefId],
        given: &[Option<Id<Expr>>],
        generics: &[(DefId, Value<Place>)],
    ) -> Vec<(DefId, String)> {
        let args = (params.iter().copied())
            .zip(given)
            .filter_map(|(def, arg)| {
                Some((def, self.ast.exprs.span((*arg)?).get_text().to_string()))
            });
        let own = generics
            .iter()
            .filter_map(|(param, value)| match &value.term {
                Term::Place(Place::Len(own)) if value.offset == 0 => {
                    Some((*param, self.names.def(*own).name.to_string()))
                }
                _ => None,
            });
        args.chain(own).collect()
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
    fn requires_of(&self, func: Id<Func>) -> Vec<(Where, Id<Func>)> {
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
        let supers = (self.names.supers(t).iter()).filter_map(|&s| match self.names.def(s).kind {
            DefKind::Trait(t) => Some(t),
            _ => None,
        });
        std::iter::once(t).chain(supers).find_map(|t| {
            (self.ast.traits[t].methods.iter().copied())
                .find(|&m| self.ast.funcs[m].name.name == *name)
        })
    }

    /// Learns function `func`'s `@where`s, which its body relies on.
    pub(crate) fn assume_requires(&mut self, func: Id<Func>) {
        let own = self.names.params(func).to_vec();
        for (clause, declared) in self.requires_of(func) {
            let theirs = self.names.params(declared).to_vec();
            let subst = |def: DefId| {
                let index = theirs.iter().position(|&d| d == def)?;
                Some(Value::place(Place::var(*own.get(index)?)))
            };
            let cond = self.cond(clause.condition, true, &subst);
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
}

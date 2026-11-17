//! What constants are made of: which other constants, and whether they run code.

use som_ast::{Ast, Const, Expr, Id};
use std::collections::HashSet;

use crate::names::{DefKind, Names};
use crate::{ResolveError, Resolver};

impl Resolver<'_> {
    /// A constant can't be made of itself. One whose value runs code is worked out by
    /// running it, which finds a loop through functions then.
    pub(crate) fn check_consts(&mut self) {
        let ast = self.ast;
        for (id, c) in ast.consts.iter() {
            let mut seen = HashSet::new();
            let mut todo = vec![c.value];
            'search: while let Some(expr) = todo.pop() {
                for used in consts_in(ast, &self.names, expr) {
                    if used == id {
                        self.errors.push(ResolveError::ConstCycle {
                            name: c.name.name.to_string(),
                            span: c.name.span.clone(),
                        });
                        break 'search;
                    }
                    if seen.insert(used) {
                        todo.push(ast.consts[used].value);
                    }
                }
            }
        }
    }
}

/// The constants `expr` names, directly, through the parts a plain constant is made of.
fn consts_in(ast: &Ast, names: &Names, expr: Id<Expr>) -> Vec<Id<Const>> {
    let mut found = Vec::new();
    let mut todo = vec![expr];
    while let Some(expr) = todo.pop() {
        if let Expr::Var(_) = ast.exprs[expr]
            && let Some(def) = names.expr(expr)
            && let DefKind::Const(c) = names.def(def).kind
        {
            found.push(c);
        }
        todo.extend(const_parts(ast, expr));
    }
    found
}

/// The expressions a plain constant's value is made of: what's inside `expr`.
fn const_parts(ast: &Ast, expr: Id<Expr>) -> Vec<Id<Expr>> {
    match &ast.exprs[expr] {
        Expr::Array(items) => items.clone(),
        Expr::Cast { value, .. }
        | Expr::ArrayRepeat { value, .. }
        | Expr::Not(value)
        | Expr::Neg(value)
        | Expr::BitNot(value) => vec![*value],
        Expr::Binary { lhs, rhs, .. } => vec![*lhs, *rhs],
        Expr::Construct { fields, .. } => fields.iter().map(|(_, v)| *v).collect(),
        Expr::Conditional {
            condition,
            then,
            otherwise,
        } => vec![*condition, *then, *otherwise],
        _ => Vec::new(),
    }
}

/// Whether constant `c`'s value runs code, a call, so it's worked out by running it while
/// compiling. A plain one, made of literals, operators, casts, other plain constants, and
/// arrays and structs of those, is written out where it's used instead.
pub fn const_runs_code(ast: &Ast, names: &Names, c: Id<Const>) -> bool {
    let mut seen = HashSet::from([c]);
    runs_code(ast, names, ast.consts[c].value, &mut seen)
}

fn runs_code(ast: &Ast, names: &Names, expr: Id<Expr>, seen: &mut HashSet<Id<Const>>) -> bool {
    let plain = match &ast.exprs[expr] {
        Expr::Hole
        | Expr::Type(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::Str(_)
        | Expr::None
        | Expr::Array(_)
        | Expr::Cast { .. }
        | Expr::ArrayRepeat { .. }
        | Expr::Not(_)
        | Expr::Neg(_)
        | Expr::BitNot(_)
        | Expr::Binary { .. }
        | Expr::Construct { .. }
        | Expr::Conditional { .. } => true,
        // Another constant, `usize::MAX`, or a variant without values.
        Expr::Var(path) => match names.expr(expr).map(|def| &names.def(def).kind) {
            Some(&DefKind::Const(other)) => {
                !seen.insert(other) || !runs_code(ast, names, ast.consts[other].value, seen)
            }
            Some(DefKind::Variant { .. }) | None => true,
            Some(_) => path.int_limit().is_some(),
        },
        _ => false,
    };
    !plain || (const_parts(ast, expr).into_iter()).any(|part| runs_code(ast, names, part, seen))
}

use crate::{Expr, Id, Part, Stmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    Expr(Id<Expr>),
    Stmt(Id<Stmt>),
}

impl Expr {
    /// In source order. Patterns, types and paths aren't children.
    pub fn for_each_child(&self, mut f: impl FnMut(Child)) {
        let mut expr = |e: Id<Expr>| f(Child::Expr(e));
        match self {
            Expr::Hole
            | Expr::Type(_)
            | Expr::InPlace(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Var(_)
            | Expr::Unreachable
            | Expr::Out(_) => {}
            Expr::Fail(e)
            | Expr::Not(e)
            | Expr::Neg(e)
            | Expr::BitNot(e)
            | Expr::Deref(e)
            | Expr::Borrow { expr: e, .. }
            | Expr::Cast { value: e, .. }
            | Expr::ArrayRepeat { value: e, .. }
            | Expr::Field { object: e, .. } => expr(*e),
            Expr::Array(items) | Expr::Call { args: items, .. } => {
                items.iter().for_each(|&e| expr(e))
            }
            Expr::Range { start, end } => start.iter().chain(end).for_each(|&e| expr(e)),
            Expr::Binary { lhs: a, rhs: b, .. }
            | Expr::Assign {
                target: a,
                value: b,
            }
            | Expr::Index {
                object: a,
                index: b,
            }
            | Expr::Fallback {
                optional: a,
                fallback: b,
            } => {
                expr(*a);
                expr(*b);
            }
            Expr::MethodCall { receiver, args, .. }
            | Expr::CallValue {
                callee: receiver,
                args,
            } => {
                expr(*receiver);
                args.iter().for_each(|&e| expr(e));
            }
            Expr::Construct { fields, .. } => fields.iter().for_each(|&(_, e)| expr(e)),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => [*condition, *then, *otherwise].into_iter().for_each(expr),
            Expr::Match { scrutinee, arms } => {
                expr(*scrutinee);
                arms.iter().for_each(|arm| expr(arm.body));
            }
            Expr::Interpolated(parts) => parts.iter().for_each(|&part| match part {
                Part::Text(e) | Part::Value(e) => expr(e),
            }),
            Expr::Block { stmts, value } => {
                stmts.iter().for_each(|&s| f(Child::Stmt(s)));
                value.iter().for_each(|&e| f(Child::Expr(e)));
            }
        }
    }
}

impl Stmt {
    /// In source order.
    pub fn for_each_child(&self, mut f: impl FnMut(Child)) {
        let stmts = |body: &[Id<Stmt>], f: &mut dyn FnMut(Child)| {
            body.iter().for_each(|&s| f(Child::Stmt(s)))
        };
        match self {
            Stmt::Use(_) | Stmt::Const(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Return(value) => value.iter().for_each(|&e| f(Child::Expr(e))),
            Stmt::Let { value: e, .. } | Stmt::Expr(e) => f(Child::Expr(*e)),
            Stmt::Loop { body } => stmts(body, &mut f),
            Stmt::While { condition: e, body }
            | Stmt::For {
                iterable: e, body, ..
            } => {
                f(Child::Expr(*e));
                stmts(body, &mut f);
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                f(Child::Expr(*condition));
                stmts(then, &mut f);
                stmts(otherwise.as_deref().unwrap_or_default(), &mut f);
            }
        }
    }
}
